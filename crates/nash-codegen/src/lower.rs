//! Structural Core lowering. Trait evidence has already been specialized.
use nash_ir::core::{Binder, Branch, CaseKind, Core, CoreKind, Name as CoreName, Test};
use nash_plutus::{arena::Arena, binder::Name, builtin::DefaultFunction, term::Term};

mod constant_sharing;

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
    lower_inner(arena, core, Sharing::None)
}

/// Cache fully forced builtin values once, outside every program argument.
/// Applied arguments remain at their original evaluation sites. This includes
/// builtins introduced by lowering, and leaves the O0 entry point unchanged.
pub fn lower_with_builtin_sharing<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
) -> Result<Uplc<'a>, Error> {
    lower_inner(arena, core, Sharing::Forces)
}

/// Accepted Chunk 5 sharing: forced references plus one-literal builtin
/// prefixes with at least two occurrences. Normal O0 assembly stays separate.
pub fn lower_with_constant_sharing<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
) -> Result<Uplc<'a>, Error> {
    lower_inner(arena, core, Sharing::ForcesAndConstants)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sharing {
    None,
    Forces,
    ForcesAndConstants,
}

fn lower_inner<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
    sharing: Sharing,
) -> Result<Uplc<'a>, Error> {
    let next_unique = largest_name(core)
        .checked_add(1)
        .ok_or(Error::NameOverflow)?;
    let mut lower = Lower {
        arena,
        next_unique,
        share: sharing != Sharing::None,
        shared: Vec::new(),
    };
    let mut term = lower.term(core)?;
    if sharing == Sharing::ForcesAndConstants {
        term = constant_sharing::share(&mut lower, term)?;
    }
    // Exhaustive cases can discard an already-lowered default. Only bind
    // references that survive in the emitted program.
    let mut referenced = std::collections::HashSet::new();
    if !lower.shared.is_empty() {
        let mut pending = vec![term];
        while let Some(node) = pending.pop() {
            match node {
                Term::Var(name) => {
                    referenced.insert(name.unique());
                }
                Term::Lambda { body, .. } | Term::Delay(body) | Term::Force(body) => {
                    pending.push(body)
                }
                Term::Apply { function, argument } => pending.extend([*function, *argument]),
                Term::Case { constr, branches } => {
                    pending.push(constr);
                    pending.extend(*branches);
                }
                Term::Constr { fields, .. } => pending.extend(*fields),
                Term::Constant(_) | Term::Builtin(_) | Term::Error => {}
            }
        }
    }
    for (func, name) in lower.shared.iter().rev() {
        if referenced.contains(&name.unique()) {
            term = term
                .lambda(arena, name)
                .apply(arena, lower.forced_builtin(*func));
        }
    }
    Ok(term)
}

struct Lower<'a> {
    arena: &'a Arena,
    next_unique: usize,
    share: bool,
    shared: Vec<(DefaultFunction, &'a Name<'a>)>,
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

    fn forced_builtin(&self, func: DefaultFunction) -> Uplc<'a> {
        let mut term = Term::builtin(self.arena, self.arena.alloc(func));
        for _ in 0..func.force_count() {
            term = term.force(self.arena);
        }
        term
    }

    fn builtin(&mut self, func: DefaultFunction, args: &[Uplc<'a>]) -> Result<Uplc<'a>, Error> {
        let mut term = if self.share && func.force_count() > 0 {
            let name =
                if let Some((_, name)) = self.shared.iter().find(|(cached, _)| *cached == func) {
                    *name
                } else {
                    let fresh = self.fresh()?;
                    let name = Name::new(self.arena, "builtin", fresh.unique());
                    self.shared.push((func, name));
                    name
                };
            Term::var(self.arena, name)
        } else {
            self.forced_builtin(func)
        };
        for arg in args {
            term = term.apply(self.arena, arg);
        }
        Ok(term)
    }

    fn branch(&self, condition: Uplc<'a>, yes: Uplc<'a>, no: Uplc<'a>) -> Uplc<'a> {
        Term::case(
            self.arena,
            condition,
            self.arena.alloc_slice_copy(&[no, yes]),
        )
    }

    fn lambda(&self, params: &[Binder<'a>], mut body: Uplc<'a>) -> Uplc<'a> {
        for param in params.iter().rev() {
            body = body.lambda(self.arena, self.name(param.name));
        }
        body
    }

    fn term(&mut self, core: &'a Core<'a>) -> Result<Uplc<'a>, Error> {
        struct CaseState<'a> {
            kind: CaseKind,
            scrutinee: Uplc<'a>,
            fallback: Uplc<'a>,
            branches: Vec<&'a Branch<'a>>,
            index: usize,
            name: Option<&'a Name<'a>>,
            arms: Vec<Option<Uplc<'a>>>,
            seen: Vec<Test<'a>>,
        }
        enum Task<'a> {
            Visit(&'a Core<'a>),
            Finish(&'a Core<'a>, usize, bool),
            Field(Uplc<'a>),
            CaseStart(CaseKind, &'a [Branch<'a>], bool),
            CaseNext(CaseState<'a>),
            CaseBody(
                CaseState<'a>,
                usize,
                Option<DefaultFunction>,
                Option<Uplc<'a>>,
            ),
        }
        let mut pending = vec![Task::Visit(core)];
        let mut results: Vec<Uplc<'a>> = Vec::new();
        while let Some(task) = pending.pop() {
            match task {
                Task::Visit(core) => {
                    let start = results.len();
                    match core.kind {
                        CoreKind::Var(n) => results.push(Term::var(self.arena, self.name(n))),
                        CoreKind::Lit(c) => results.push(Term::constant(self.arena, c)),
                        CoreKind::Error => results.push(Term::error(self.arena)),
                        CoreKind::LetRec { .. } => return Err(Error::Unlowered("recursion")),
                        CoreKind::Field {
                            record,
                            index,
                            arity,
                        } => {
                            if index >= arity {
                                return Err(Error::InvalidField { index, arity });
                            }
                            let params = (0..arity)
                                .map(|_| self.fresh())
                                .collect::<Result<Vec<_>, _>>()?;
                            let mut selector = Term::var(self.arena, params[index as usize]);
                            for param in params.into_iter().rev() {
                                selector = selector.lambda(self.arena, param);
                            }
                            pending.push(Task::Field(selector));
                            pending.push(Task::Visit(record));
                        }
                        CoreKind::Case {
                            kind,
                            scrutinee,
                            branches,
                            default,
                        } => {
                            pending.push(Task::CaseStart(kind, branches, default.is_some()));
                            if let Some(body) = default {
                                pending.push(Task::Visit(body));
                            }
                            pending.push(Task::Visit(scrutinee));
                        }
                        CoreKind::Let {
                            binder,
                            value,
                            body,
                        } => {
                            let unit = binder.ty
                                == nash_ir::ty::Ty::Const(&nash_ir::ty::ConstTy::Unit)
                                && !crate::build::names(body).contains(&binder.name.unique);
                            pending.push(Task::Finish(core, start, unit));
                            pending.push(Task::Visit(value));
                            pending.push(Task::Visit(body));
                        }
                        _ => {
                            if let CoreKind::Builtin { func, args } = core.kind
                                && args.len() > func.arity()
                            {
                                return Err(Error::BuiltinArity);
                            }
                            pending.push(Task::Finish(core, start, false));
                            let mut children = Vec::new();
                            core.push_children_reversed(&mut children);
                            pending.extend(children.into_iter().map(Task::Visit));
                        }
                    }
                }
                Task::Field(selector) => {
                    let record = results.pop().unwrap();
                    results.push(Term::case(
                        self.arena,
                        record,
                        self.arena.alloc_slice_copy(&[selector]),
                    ));
                }
                Task::Finish(core, start, unit) => {
                    let mut children = results.drain(start..);
                    let result = match core.kind {
                        CoreKind::Lam { params, .. } => {
                            self.lambda(params, children.next().unwrap())
                        }
                        CoreKind::App { .. } => {
                            let mut func = children.next().unwrap();
                            for arg in children.by_ref() {
                                func = func.apply(self.arena, arg);
                            }
                            func
                        }
                        CoreKind::Let { binder, .. } => {
                            let body = children.next().unwrap();
                            let value = children.next().unwrap();
                            if unit {
                                Term::case(self.arena, value, self.arena.alloc_slice_copy(&[body]))
                            } else {
                                body.lambda(self.arena, self.name(binder.name))
                                    .apply(self.arena, value)
                            }
                        }
                        CoreKind::Builtin { func, .. } => {
                            self.builtin(func, &children.by_ref().collect::<Vec<_>>())?
                        }
                        CoreKind::Constr { tag, .. } => Term::constr(
                            self.arena,
                            tag as usize,
                            self.arena
                                .alloc_slice_copy(&children.by_ref().collect::<Vec<_>>()),
                        ),
                        CoreKind::Trace { .. } => {
                            let message = children.next().unwrap();
                            let body = children.next().unwrap();
                            self.builtin(
                                DefaultFunction::Trace,
                                &[message, body.delay(self.arena)],
                            )?
                            .force(self.arena)
                        }
                        CoreKind::Delay(_) => children.next().unwrap().delay(self.arena),
                        CoreKind::Force(_) => children.next().unwrap().force(self.arena),
                        _ => unreachable!(),
                    };
                    drop(children);
                    results.push(result);
                }
                Task::CaseStart(kind, branches, has_default) => {
                    let fallback = if has_default {
                        results.pop().unwrap()
                    } else {
                        Term::error(self.arena)
                    };
                    let scrutinee = results.pop().unwrap();
                    if kind == CaseKind::Pair {
                        let [branch] = branches else {
                            return Err(Error::InvalidCase("pair case requires one branch"));
                        };
                        if branch.test != Test::Pair || branch.binders.len() != 2 || has_default {
                            return Err(Error::InvalidCase("pair branch must bind its two fields"));
                        }
                    }
                    if kind == CaseKind::Tag && has_default {
                        return Err(Error::InvalidCase(
                            "tag defaults must be expanded using constructor arities",
                        ));
                    }
                    let mut branches: Vec<_> = branches.iter().collect();
                    let name = match kind {
                        CaseKind::Int | CaseKind::Bytes => {
                            branches.reverse();
                            Some(self.fresh()?)
                        }
                        CaseKind::Data => Some(self.fresh()?),
                        CaseKind::Tag => {
                            branches.sort_by_key(|b| {
                                if let Test::Tag(tag) = b.test {
                                    tag
                                } else {
                                    u16::MAX
                                }
                            });
                            None
                        }
                        _ => None,
                    };
                    let count = match kind {
                        CaseKind::Bool | CaseKind::List => 2,
                        CaseKind::Data => 5,
                        _ => branches.len(),
                    };
                    pending.push(Task::CaseNext(CaseState {
                        kind,
                        scrutinee,
                        fallback,
                        branches,
                        index: 0,
                        name,
                        arms: vec![None; count],
                        seen: Vec::new(),
                    }));
                }
                Task::CaseNext(state) => {
                    if let Some(branch) = state.branches.get(state.index) {
                        let mut unwrap = None;
                        let mut condition = None;
                        let slot = match state.kind {
                            CaseKind::Pair => 0,
                            CaseKind::Bool => {
                                if !branch.binders.is_empty() {
                                    return Err(Error::InvalidCase(
                                        "boolean branches bind no fields",
                                    ));
                                }
                                let slot = match branch.test {
                                    Test::False => 0,
                                    Test::True => 1,
                                    _ => return Err(Error::InvalidCase("non-boolean test")),
                                };
                                if state.arms[slot].is_some() {
                                    return Err(Error::InvalidCase("duplicate boolean branch"));
                                }
                                slot
                            }
                            CaseKind::Int | CaseKind::Bytes => {
                                if !branch.binders.is_empty() || state.seen.contains(&branch.test) {
                                    return Err(Error::InvalidCase("invalid literal branch"));
                                }
                                let (func, literal) = match (state.kind, branch.test) {
                                    (CaseKind::Int, Test::Int(i)) => (
                                        DefaultFunction::EqualsInteger,
                                        Term::integer(self.arena, i),
                                    ),
                                    (CaseKind::Bytes, Test::Bytes(bytes)) => (
                                        DefaultFunction::EqualsByteString,
                                        Term::byte_string(self.arena, bytes),
                                    ),
                                    _ => {
                                        return Err(Error::InvalidCase(
                                            "literal test has wrong kind",
                                        ));
                                    }
                                };
                                condition = Some(self.builtin(
                                    func,
                                    &[Term::var(self.arena, state.name.unwrap()), literal],
                                )?);
                                state.index
                            }
                            CaseKind::List => match branch.test {
                                Test::Nil
                                    if state.arms[1].is_none() && branch.binders.is_empty() =>
                                {
                                    1
                                }
                                Test::Cons
                                    if state.arms[0].is_none() && branch.binders.len() == 2 =>
                                {
                                    0
                                }
                                _ => return Err(Error::InvalidCase("invalid list branch")),
                            },
                            CaseKind::Data => {
                                let (slot, func) = match branch.test {
                                    Test::DataConstr => (0, DefaultFunction::UnConstrData),
                                    Test::DataMap => (1, DefaultFunction::UnMapData),
                                    Test::DataList => (2, DefaultFunction::UnListData),
                                    Test::DataI => (3, DefaultFunction::UnIData),
                                    Test::DataB => (4, DefaultFunction::UnBData),
                                    _ => return Err(Error::InvalidCase("non-Data test")),
                                };
                                if state.arms[slot].is_some() || branch.binders.len() != 1 {
                                    return Err(Error::InvalidCase("invalid Data branch"));
                                }
                                unwrap = Some(func);
                                slot
                            }
                            CaseKind::Tag => {
                                if branch.test
                                    != Test::Tag(
                                        u16::try_from(state.index).map_err(|_| {
                                            Error::InvalidCase("too many constructors")
                                        })?,
                                    )
                                {
                                    return Err(Error::InvalidCase(
                                        "tag branches must cover consecutive unique tags",
                                    ));
                                }
                                state.index
                            }
                        };
                        let body = branch.body;
                        pending.push(Task::CaseBody(state, slot, unwrap, condition));
                        pending.push(Task::Visit(body));
                    } else {
                        let result =
                            match state.kind {
                                CaseKind::Pair | CaseKind::Tag => Term::case(
                                    self.arena,
                                    state.scrutinee,
                                    self.arena.alloc_slice_copy(
                                        &state
                                            .arms
                                            .into_iter()
                                            .map(Option::unwrap)
                                            .collect::<Vec<_>>(),
                                    ),
                                ),
                                CaseKind::Bool => self.branch(
                                    state.scrutinee,
                                    state.arms[1].unwrap_or(state.fallback),
                                    state.arms[0].unwrap_or(state.fallback),
                                ),
                                CaseKind::Int | CaseKind::Bytes => state
                                    .fallback
                                    .lambda(self.arena, state.name.unwrap())
                                    .apply(self.arena, state.scrutinee),
                                CaseKind::List => {
                                    let cons = match state.arms[0] {
                                        Some(cons) => cons,
                                        None => state
                                            .fallback
                                            .lambda(self.arena, self.fresh()?)
                                            .lambda(self.arena, self.fresh()?),
                                    };
                                    Term::case(
                                        self.arena,
                                        state.scrutinee,
                                        self.arena.alloc_slice_copy(&[
                                            cons,
                                            state.arms[1].unwrap_or(state.fallback),
                                        ]),
                                    )
                                }
                                CaseKind::Data => {
                                    let value = Term::var(self.arena, state.name.unwrap());
                                    let mut args = vec![value];
                                    args.extend(state.arms.into_iter().map(|arm| {
                                        arm.unwrap_or(state.fallback).delay(self.arena)
                                    }));
                                    self.builtin(DefaultFunction::ChooseData, &args)?
                                        .force(self.arena)
                                        .lambda(self.arena, state.name.unwrap())
                                        .apply(self.arena, state.scrutinee)
                                }
                            };
                        results.push(result);
                    }
                }
                Task::CaseBody(mut state, slot, unwrap, condition) => {
                    let body = results.pop().unwrap();
                    let branch = state.branches[state.index];
                    if let Some(condition) = condition {
                        state.fallback = self.branch(condition, body, state.fallback);
                        state.seen.push(branch.test);
                    } else {
                        state.arms[slot] = Some(if let Some(unwrap) = unwrap {
                            if crate::build::names(branch.body)
                                .contains(&branch.binders[0].name.unique)
                            {
                                let value = Term::var(self.arena, state.name.unwrap());
                                let unwrapped = self.builtin(unwrap, &[value])?;
                                self.lambda(branch.binders, body)
                                    .apply(self.arena, unwrapped)
                            } else {
                                body
                            }
                        } else {
                            self.lambda(branch.binders, body)
                        });
                    }
                    state.index += 1;
                    pending.push(Task::CaseNext(state));
                }
            }
        }
        Ok(results.pop().unwrap())
    }
}

fn largest_name(core: &Core<'_>) -> usize {
    let mut largest = 0;
    let mut pending = vec![core];
    while let Some(core) = pending.pop() {
        let mut name = |n: CoreName<'_>| largest = largest.max(n.unique as usize);
        match &core.kind {
            CoreKind::Var(n) => name(*n),
            CoreKind::Lam { params, body } => {
                for p in *params {
                    name(p.name);
                }
                pending.push(body);
            }
            CoreKind::App { func, args } => {
                pending.push(func);
                pending.extend(*args);
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                name(binder.name);
                pending.extend([*value, *body]);
            }
            CoreKind::LetRec { binders, body } => {
                for b in *binders {
                    name(b.binder.name);
                    for p in b.params {
                        name(p.name);
                    }
                    pending.push(b.body);
                }
                pending.push(body);
            }
            CoreKind::Case {
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
            CoreKind::Constr { fields, .. } | CoreKind::Builtin { args: fields, .. } => {
                pending.extend(*fields)
            }
            CoreKind::Field { record, .. } => pending.push(record),
            CoreKind::Delay(arg) | CoreKind::Force(arg) => pending.push(arg),
            CoreKind::Trace { message, body } => pending.extend([*message, *body]),
            CoreKind::Lit(_) | CoreKind::Error => {}
        }
    }
    largest
}

#[cfg(test)]
mod tests;
