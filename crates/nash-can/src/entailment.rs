//! Superclass entailment over rigid canonical types, before inference.
use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_ast::{Head, HeadCon, ImplKey, Pred, QualifiedName, Type};
use nash_region::Located;

use crate::Error;
use crate::environment::{ImplInfo, Tables};

// Flexible contexts need not decrease. Bound both resolution and structural work.
const WORK_LIMIT: usize = 16_384;
const DEPTH_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug)]
pub enum Failure {
    Missing,
    Cycle,
    Limit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Constructor<'a> {
    Var(&'a str),
    Named(QualifiedName<'a>),
    Unit,
    Tuple(usize),
    Function,
    Record {
        fields: &'a [&'a str],
        ext: Option<&'a str>,
    },
}

struct Term<'a> {
    con: Constructor<'a>,
    args: &'a [&'a Term<'a>],
}

#[derive(Clone, Copy)]
struct Predicate<'a> {
    trait_: QualifiedName<'a>,
    args: &'a [&'a Term<'a>],
}

type Substitution<'a> = BTreeMap<&'a str, &'a Term<'a>>;

struct Resolver<'t, 'a> {
    bump: &'a Bump,
    tables: &'t Tables<'a>,
    remaining: usize,
    kinds: crate::kinds::ImplKinds<'t, 'a>,
}

impl<'a> Resolver<'_, 'a> {
    fn canonical(
        &mut self,
        term: &'a Term<'a>,
        depth: usize,
    ) -> Result<&'a Located<Type<'a>>, Failure> {
        self.step(depth)?;
        let mut args = Vec::new();
        for arg in term.args {
            args.push(self.canonical(arg, depth + 1)?);
        }
        let args = self.bump.alloc_slice_fill_iter(args);
        let typ = match term.con {
            Constructor::Named(reference) => Type::Named { reference, args },
            Constructor::Var(name) if args.is_empty() => Type::Var(name),
            Constructor::Var(name) => Type::App {
                head: self.bump.alloc(Located::at_zero(Type::Var(name))),
                args,
            },
            Constructor::Unit => Type::Unit,
            Constructor::Tuple(_) => Type::Tuple {
                first: args[0],
                second: args[1],
                rest: &args[2..],
            },
            Constructor::Function => Type::Lambda {
                from: args[0],
                to: args[1],
            },
            Constructor::Record { fields, ext } => Type::Record {
                fields: self.bump.alloc_slice_fill_iter(
                    fields
                        .iter()
                        .zip(args.iter())
                        .enumerate()
                        .map(|(index, (field, typ))| nash_ast::FieldType {
                            index: index as u16,
                            field,
                            typ,
                        }),
                ),
                ext,
            },
        };
        Ok(self.bump.alloc(Located::at_zero(typ)))
    }
    fn step(&mut self, depth: usize) -> Result<(), Failure> {
        if depth >= DEPTH_LIMIT || self.remaining == 0 {
            return Err(Failure::Limit);
        }
        self.remaining -= 1;
        Ok(())
    }

    fn term(
        &mut self,
        typ: &'a Located<Type<'a>>,
        subst: &Substitution<'a>,
        depth: usize,
    ) -> Result<&'a Term<'a>, Failure> {
        self.step(depth)?;
        let (con, args): (_, Vec<_>) = match &typ.value {
            Type::Var(name) => {
                if let Some(term) = subst.get(name) {
                    return Ok(term);
                }
                (Constructor::Var(name), Vec::new())
            }
            Type::Named { reference, args } => (Constructor::Named(*reference), args.to_vec()),
            // Aliases are nominal. Their representation body is irrelevant to impl lookup.
            Type::Alias {
                reference,
                arguments,
                ..
            } => (
                Constructor::Named(*reference),
                arguments.iter().map(|a| a.typ).collect(),
            ),
            Type::Unit => (Constructor::Unit, Vec::new()),
            Type::Tuple {
                first,
                second,
                rest,
            } => (
                Constructor::Tuple(2 + rest.len()),
                [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .collect(),
            ),
            Type::Lambda { from, to } => (Constructor::Function, vec![*from, *to]),
            Type::Record { fields, ext } => (
                Constructor::Record {
                    fields: self
                        .bump
                        .alloc_slice_fill_iter(fields.iter().map(|f| f.field)),
                    ext: *ext,
                },
                fields.iter().map(|f| f.typ).collect(),
            ),
            Type::App { head, args } => {
                let head = self.term(head, subst, depth + 1)?;
                let mut applied = head.args.to_vec();
                for arg in *args {
                    applied.push(self.term(arg, subst, depth + 1)?);
                }
                return Ok(self.bump.alloc(Term {
                    con: head.con,
                    args: self.bump.alloc_slice_fill_iter(applied),
                }));
            }
        };
        let mut terms = Vec::new();
        for arg in args {
            terms.push(self.term(arg, subst, depth + 1)?);
        }
        Ok(self.bump.alloc(Term {
            con,
            args: self.bump.alloc_slice_fill_iter(terms),
        }))
    }

    fn predicate(
        &mut self,
        pred: &Pred<'a>,
        subst: &Substitution<'a>,
    ) -> Result<Predicate<'a>, Failure> {
        self.step(0)?;
        let mut args = Vec::new();
        for arg in pred.args {
            args.push(self.term(arg, subst, 0)?);
        }
        Ok(Predicate {
            trait_: pred.trait_,
            args: self.bump.alloc_slice_fill_iter(args),
        })
    }

    fn equal(&mut self, a: Predicate<'a>, b: Predicate<'a>) -> Result<bool, Failure> {
        self.step(0)?;
        if a.trait_ != b.trait_ || a.args.len() != b.args.len() {
            return Ok(false);
        }
        let mut pending: Vec<_> = a.args.iter().zip(b.args).collect();
        while let Some((a, b)) = pending.pop() {
            self.step(0)?;
            if a.con != b.con || a.args.len() != b.args.len() {
                return Ok(false);
            }
            pending.extend(a.args.iter().zip(b.args));
        }
        Ok(true)
    }

    fn givens(&mut self, context: &[Pred<'a>]) -> Result<Vec<Predicate<'a>>, Failure> {
        let mut pending = Vec::new();
        for pred in context {
            pending.push(self.predicate(pred, &BTreeMap::new())?);
        }
        let mut result = Vec::new();
        while let Some(pred) = pending.pop() {
            self.step(0)?;
            let mut seen = false;
            for old in &result {
                if self.equal(pred, *old)? {
                    seen = true;
                    break;
                }
            }
            if seen {
                continue;
            }
            result.push(pred);
            let trait_ = self.tables.traits[&pred.trait_];
            let subst = trait_
                .parameters
                .iter()
                .copied()
                .zip(pred.args.iter().copied())
                .collect();
            for sup in trait_.supers {
                pending.push(self.predicate(sup, &subst)?);
            }
        }
        Ok(result)
    }

    fn resolve(
        &mut self,
        given: &[Predicate<'a>],
        wanted: Predicate<'a>,
        active: &mut Vec<Predicate<'a>>,
    ) -> Result<(), Failure> {
        self.step(active.len())?;
        for pred in given {
            if self.equal(*pred, wanted)? {
                return Ok(());
            }
        }
        if self.tables.has_reflexive_lift()
            && wanted.trait_ == nash_ast::primitives::lift_trait()
            && wanted.args.len() == 2
            && self.equal(
                Predicate {
                    trait_: wanted.trait_,
                    args: &wanted.args[..1],
                },
                Predicate {
                    trait_: wanted.trait_,
                    args: &wanted.args[1..],
                },
            )?
        {
            let typ = self.canonical(wanted.args[0], 0)?;
            if self.kinds.proves_big(typ) {
                return Ok(());
            }
        }
        for pred in active.iter() {
            if self.equal(*pred, wanted)? {
                return Err(Failure::Cycle);
            }
        }
        let mut heads = Vec::new();
        for arg in wanted.args {
            heads.push(match arg.con {
                Constructor::Named(name) => HeadCon::Named(name),
                Constructor::Unit => HeadCon::Unit,
                Constructor::Tuple(n) => HeadCon::Tuple(n as u8),
                Constructor::Function => HeadCon::Fun,
                Constructor::Var(_) | Constructor::Record { .. } => return Err(Failure::Missing),
            });
        }
        let key = ImplKey {
            trait_: wanted.trait_,
            heads: &heads,
        };
        let impl_ = *self.tables.impls.get(&key).ok_or(Failure::Missing)?;
        let mut subst = BTreeMap::new();
        for (head, arg) in impl_.heads.iter().zip(wanted.args) {
            let vars = match &head.value {
                Head::Named { vars, .. } | Head::Tuple(vars) => *vars,
                Head::Unit => &[],
            };
            if vars.len() != arg.args.len() {
                return Err(Failure::Missing);
            }
            subst.extend(vars.iter().copied().zip(arg.args.iter().copied()));
        }
        active.push(wanted);
        for pred in impl_.context {
            let wanted = self.predicate(pred, &subst)?;
            self.resolve(given, wanted, active)?;
        }
        active.pop();
        Ok(())
    }
}

pub(crate) fn check<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    kind_env: &crate::kinds::KindEnv<'a>,
    impl_: &ImplInfo<'a>,
) -> Result<(), Vec<Error<'a>>> {
    let trait_ = tables.traits[&impl_.trait_];
    if trait_.supers.is_empty() {
        return Ok(());
    }
    let mut subst = BTreeMap::new();
    let mut canonical_subst = BTreeMap::new();
    for (parameter, head) in trait_.parameters.iter().zip(impl_.heads) {
        let (con, vars) = match &head.value {
            Head::Named { reference, vars } => (Constructor::Named(*reference), *vars),
            Head::Tuple(vars) => (Constructor::Tuple(vars.len()), *vars),
            Head::Unit => (Constructor::Unit, &[][..]),
        };
        let args = bump.alloc_slice_fill_iter(vars.iter().map(|v| {
            &*bump.alloc(Term {
                con: Constructor::Var(v),
                args: &[],
            })
        }));
        subst.insert(*parameter, &*bump.alloc(Term { con, args }));
        let args = bump.alloc_slice_fill_iter(
            vars.iter()
                .map(|v| &*bump.alloc(Located::at(head.region, Type::Var(v)))),
        );
        let typ = match &head.value {
            Head::Named { reference, .. } => Type::Named {
                reference: *reference,
                args,
            },
            Head::Tuple(_) => Type::Tuple {
                first: args[0],
                second: args[1],
                rest: &args[2..],
            },
            Head::Unit => Type::Unit,
        };
        canonical_subst.insert(*parameter, &*bump.alloc(Located::at(head.region, typ)));
    }
    let heads: Vec<_> = trait_
        .parameters
        .iter()
        .map(|p| canonical_subst[p])
        .collect();
    let mut variables = BTreeMap::new();
    for head in impl_.heads {
        let vars = match &head.value {
            Head::Named { vars, .. } | Head::Tuple(vars) => *vars,
            Head::Unit => &[],
        };
        for var in vars {
            variables.insert(*var, head.region);
        }
    }
    let kinds = crate::kinds::check_impl_heads(
        bump,
        kind_env,
        impl_.home,
        trait_,
        &heads,
        &variables,
        impl_.context,
    )?;
    let mut resolver = Resolver {
        bump,
        tables,
        remaining: WORK_LIMIT,
        kinds,
    };
    let givens = resolver.givens(impl_.context);
    for (index, superclass) in trait_.supers.iter().enumerate() {
        let checked = (|| {
            let givens = givens.as_ref().map_err(|reason| *reason)?;
            let wanted = resolver.predicate(superclass, &subst)?;
            resolver.resolve(givens, wanted, &mut Vec::new())
        })();
        if let Err(reason) = checked {
            return Err(vec![Error::MissingSuperclass {
                region: impl_.region,
                trait_: impl_.trait_,
                heads: impl_.heads,
                superclass: bump.alloc(Pred {
                    trait_: superclass.trait_,
                    args: bump.alloc_slice_fill_iter(
                        superclass
                            .args
                            .iter()
                            .map(|a| crate::types::substitute_type(bump, &canonical_subst, a)),
                    ),
                }),
                index: index as u16,
                reason,
            }]);
        }
    }
    Ok(())
}
