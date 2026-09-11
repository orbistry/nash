//! Structural Core lowering. Trait evidence has already been specialized.
use nash_ir::core::{Binder, Branch, CaseKind, Core, Name as CoreName, Test};
use nash_plutus::{arena::Arena, binder::Name, builtin::DefaultFunction, term::Term};

type Uplc<'a> = &'a Term<'a, Name<'a>>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid field {index} for constructor arity {arity}")]
    InvalidField { index: u16, arity: u16 },
    #[error("invalid Core case: {0}")]
    InvalidCase(&'static str),
    #[error("Core builtin has too many arguments")]
    BuiltinArity,
    #[error("Core {0} must be expanded before structural lowering")]
    Unlowered(&'static str),
    #[error("program uses too many unique names")]
    NameOverflow,
}

pub fn lower<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Result<Uplc<'a>, Error> {
    let next_unique = largest_name(core)
        .checked_add(1)
        .ok_or(Error::NameOverflow)?;
    Lower { arena, next_unique }.term(core)
}

struct Lower<'a> {
    arena: &'a Arena,
    next_unique: usize,
}

impl<'a> Lower<'a> {
    fn name(&self, name: CoreName<'a>) -> &'a Name<'a> {
        Name::new(self.arena, name.text, name.unique as usize)
    }

    fn fresh(&mut self) -> Result<&'a Name<'a>, Error> {
        let unique = self.next_unique;
        self.next_unique = unique.checked_add(1).ok_or(Error::NameOverflow)?;
        Ok(Name::new(self.arena, "generated", unique))
    }

    fn builtin(&self, func: DefaultFunction, args: &[Uplc<'a>]) -> Uplc<'a> {
        let mut term = Term::builtin(self.arena, self.arena.alloc(func));
        for _ in 0..func.force_count() {
            term = term.force(self.arena);
        }
        for arg in args {
            term = term.apply(self.arena, arg);
        }
        term
    }

    fn lazy_if(&self, condition: Uplc<'a>, yes: Uplc<'a>, no: Uplc<'a>) -> Uplc<'a> {
        self.builtin(
            DefaultFunction::IfThenElse,
            &[condition, yes.delay(self.arena), no.delay(self.arena)],
        )
        .force(self.arena)
    }

    fn lambda(&self, params: &[Binder<'a>], mut body: Uplc<'a>) -> Uplc<'a> {
        for param in params.iter().rev() {
            body = body.lambda(self.arena, self.name(param.name));
        }
        body
    }

    fn term(&mut self, core: &'a Core<'a>) -> Result<Uplc<'a>, Error> {
        Ok(match core {
            Core::Var(n) => Term::var(self.arena, self.name(*n)),
            Core::Lit(c) => Term::constant(self.arena, c),
            Core::Lam { params, body } => {
                let body = self.term(body)?;
                self.lambda(params, body)
            }
            Core::App { func, args } => {
                let mut term = self.term(func)?;
                for arg in *args {
                    term = term.apply(self.arena, self.term(arg)?);
                }
                term
            }
            Core::Let {
                binder,
                value,
                body,
            } => {
                let body = self.term(body)?;
                let value = self.term(value)?;
                body.lambda(self.arena, self.name(binder.name))
                    .apply(self.arena, value)
            }
            Core::Builtin { func, args } => {
                if args.len() > func.arity() {
                    return Err(Error::BuiltinArity);
                }
                let args = args
                    .iter()
                    .map(|arg| self.term(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                self.builtin(*func, &args)
            }
            Core::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => self.case(*kind, scrutinee, branches, *default)?,
            Core::Constr { tag, fields } => {
                let fields = fields
                    .iter()
                    .map(|f| self.term(f))
                    .collect::<Result<Vec<_>, _>>()?;
                Term::constr(
                    self.arena,
                    *tag as usize,
                    self.arena.alloc_slice_copy(&fields),
                )
            }
            Core::Field {
                record,
                index,
                arity,
            } => {
                if index >= arity {
                    return Err(Error::InvalidField {
                        index: *index,
                        arity: *arity,
                    });
                }
                let params = (0..*arity)
                    .map(|_| self.fresh())
                    .collect::<Result<Vec<_>, _>>()?;
                let mut selector = Term::var(self.arena, params[*index as usize]);
                for param in params.into_iter().rev() {
                    selector = selector.lambda(self.arena, param);
                }
                Term::case(
                    self.arena,
                    self.term(record)?,
                    self.arena.alloc_slice_copy(&[selector]),
                )
            }
            Core::Trace { message, body } => {
                let message = self.term(message)?;
                let body = self.term(body)?;
                self.builtin(DefaultFunction::Trace, &[message, body.delay(self.arena)])
                    .force(self.arena)
            }
            Core::Error => Term::error(self.arena),
            Core::Delay(t) => self.term(t)?.delay(self.arena),
            Core::Force(t) => self.term(t)?.force(self.arena),
            Core::LetRec { .. } => return Err(Error::Unlowered("recursion")),
            Core::Cast { .. } => return Err(Error::Unlowered("cast")),
        })
    }

    fn case(
        &mut self,
        kind: CaseKind,
        scrutinee: &'a Core<'a>,
        branches: &'a [Branch<'a>],
        default: Option<&'a Core<'a>>,
    ) -> Result<Uplc<'a>, Error> {
        if kind == CaseKind::Tag {
            if default.is_some() {
                return Err(Error::InvalidCase(
                    "tag defaults must be expanded using constructor arities",
                ));
            }
            let mut ordered = branches.iter().collect::<Vec<_>>();
            ordered.sort_by_key(|b| {
                if let Test::Tag(tag) = b.test {
                    tag
                } else {
                    u16::MAX
                }
            });
            let mut arms = Vec::new();
            for (i, b) in ordered.into_iter().enumerate() {
                if b.test
                    != Test::Tag(
                        u16::try_from(i)
                            .map_err(|_| Error::InvalidCase("too many constructors"))?,
                    )
                {
                    return Err(Error::InvalidCase(
                        "tag branches must cover consecutive unique tags",
                    ));
                }
                let body = self.term(b.body)?;
                arms.push(self.lambda(b.binders, body));
            }
            return Ok(Term::case(
                self.arena,
                self.term(scrutinee)?,
                self.arena.alloc_slice_copy(&arms),
            ));
        }
        // A match evaluates its scrutinee exactly once, including default-only matches.
        let name = self.fresh()?;
        let value = Term::var(self.arena, name);
        let fallback = match default {
            Some(c) => self.term(c)?,
            None => Term::error(self.arena),
        };
        let result = match kind {
            CaseKind::Bool => {
                let mut yes = None;
                let mut no = None;
                for b in branches {
                    if !b.binders.is_empty() {
                        return Err(Error::InvalidCase("boolean branches bind no fields"));
                    }
                    let slot = match b.test {
                        Test::True => &mut yes,
                        Test::False => &mut no,
                        _ => return Err(Error::InvalidCase("non-boolean test")),
                    };
                    if slot.is_some() {
                        return Err(Error::InvalidCase("duplicate boolean branch"));
                    }
                    *slot = Some(self.term(b.body)?);
                }
                self.lazy_if(value, yes.unwrap_or(fallback), no.unwrap_or(fallback))
            }
            CaseKind::Int | CaseKind::Bytes => {
                let mut rest = fallback;
                let mut seen = Vec::new();
                for b in branches.iter().rev() {
                    if !b.binders.is_empty() || seen.contains(&b.test) {
                        return Err(Error::InvalidCase("invalid literal branch"));
                    }
                    seen.push(b.test);
                    let (func, literal) = match (kind, b.test) {
                        (CaseKind::Int, Test::Int(i)) => {
                            (DefaultFunction::EqualsInteger, Term::integer(self.arena, i))
                        }
                        (CaseKind::Bytes, Test::Bytes(bytes)) => (
                            DefaultFunction::EqualsByteString,
                            Term::byte_string(self.arena, bytes),
                        ),
                        _ => return Err(Error::InvalidCase("literal test has wrong kind")),
                    };
                    let condition = self.builtin(func, &[value, literal]);
                    let body = self.term(b.body)?;
                    rest = self.lazy_if(condition, body, rest);
                }
                rest
            }
            CaseKind::List => {
                let mut nil = None;
                let mut cons = None;
                for b in branches {
                    match b.test {
                        Test::Nil if nil.is_none() && b.binders.is_empty() => {
                            nil = Some(self.term(b.body)?)
                        }
                        Test::Cons if cons.is_none() && b.binders.len() == 2 => {
                            let head = self.builtin(DefaultFunction::HeadList, &[value]);
                            let tail = self.builtin(DefaultFunction::TailList, &[value]);
                            let body = self.term(b.body)?;
                            cons = Some(
                                self.lambda(b.binders, body)
                                    .apply(self.arena, head)
                                    .apply(self.arena, tail),
                            );
                        }
                        _ => return Err(Error::InvalidCase("invalid list branch")),
                    }
                }
                self.lazy_if(
                    self.builtin(DefaultFunction::NullList, &[value]),
                    nil.unwrap_or(fallback),
                    cons.unwrap_or(fallback),
                )
            }
            CaseKind::Data => {
                let mut arms = [None; 5];
                for b in branches {
                    let (index, unwrap, arity) = match b.test {
                        Test::DataConstr => (0, DefaultFunction::UnConstrData, 2),
                        Test::DataMap => (1, DefaultFunction::UnMapData, 1),
                        Test::DataList => (2, DefaultFunction::UnListData, 1),
                        Test::DataI => (3, DefaultFunction::UnIData, 1),
                        Test::DataB => (4, DefaultFunction::UnBData, 1),
                        _ => return Err(Error::InvalidCase("non-Data test")),
                    };
                    if arms[index].is_some() || b.binders.len() != arity {
                        return Err(Error::InvalidCase("invalid Data branch"));
                    }
                    let body = self.term(b.body)?;
                    let function = self.lambda(b.binders, body);
                    let unwrapped = self.builtin(unwrap, &[value]);
                    arms[index] = Some(if index == 0 {
                        let pair_name = self.fresh()?;
                        let pair = Term::var(self.arena, pair_name);
                        function
                            .apply(self.arena, self.builtin(DefaultFunction::FstPair, &[pair]))
                            .apply(self.arena, self.builtin(DefaultFunction::SndPair, &[pair]))
                            .lambda(self.arena, pair_name)
                            .apply(self.arena, unwrapped)
                    } else {
                        function.apply(self.arena, unwrapped)
                    });
                }
                let mut args = vec![value];
                args.extend(arms.map(|arm| arm.unwrap_or(fallback).delay(self.arena)));
                self.builtin(DefaultFunction::ChooseData, &args)
                    .force(self.arena)
            }
            CaseKind::Tag => unreachable!("tag case handled above"),
        };
        Ok(result
            .lambda(self.arena, name)
            .apply(self.arena, self.term(scrutinee)?))
    }
}

fn largest_name(core: &Core<'_>) -> usize {
    let mut largest = 0;
    let mut pending = vec![core];
    while let Some(core) = pending.pop() {
        let mut name = |n: CoreName<'_>| largest = largest.max(n.unique as usize);
        match core {
            Core::Var(n) => name(*n),
            Core::Lam { params, body } => {
                for p in *params {
                    name(p.name);
                }
                pending.push(body);
            }
            Core::App { func, args } => {
                pending.push(func);
                pending.extend(*args);
            }
            Core::Let {
                binder,
                value,
                body,
            } => {
                name(binder.name);
                pending.extend([*value, *body]);
            }
            Core::LetRec { binders, body } => {
                for b in *binders {
                    name(b.binder.name);
                    for p in b.params {
                        name(p.name);
                    }
                    pending.push(b.body);
                }
                pending.push(body);
            }
            Core::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                pending.push(scrutinee);
                for b in *branches {
                    for p in b.binders {
                        name(p.name);
                    }
                    pending.push(b.body);
                }
                pending.extend(*default);
            }
            Core::Constr { fields, .. } | Core::Builtin { args: fields, .. } => {
                pending.extend(*fields)
            }
            Core::Field { record, .. } => pending.push(record),
            Core::Cast { arg, .. } | Core::Delay(arg) | Core::Force(arg) => pending.push(arg),
            Core::Trace { message, body } => pending.extend([*message, *body]),
            Core::Lit(_) | Core::Error => {}
        }
    }
    largest
}

#[cfg(test)]
mod tests;
