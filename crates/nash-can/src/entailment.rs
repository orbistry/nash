//! Superclass entailment over rigid canonical types, before inference.
use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_ast::{Pred, QualifiedName, Type};
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
    Record { fields: &'a [&'a str] },
}

struct Term<'a> {
    con: Constructor<'a>,
    args: &'a [&'a Term<'a>],
}

#[derive(Clone, Copy)]
struct Predicate<'a> {
    trait_: Option<QualifiedName<'a>>,
    args: &'a [&'a Term<'a>],
}

type Substitution<'a> = BTreeMap<&'a str, &'a Term<'a>>;

struct Resolver<'t, 'a> {
    bump: &'a Bump,
    tables: &'t Tables<'a>,
    remaining: usize,
    kinds: &'t crate::kinds::KindEnv<'a>,
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
            Constructor::Record { fields } => Type::Record {
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
            Type::Record { fields } => (
                Constructor::Record {
                    fields: self
                        .bump
                        .alloc_slice_fill_iter(fields.iter().map(|f| f.field)),
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
        for arg in pred.types() {
            args.push(self.term(arg, subst, 0)?);
        }
        Ok(Predicate {
            trait_: pred.trait_ref(),
            args: self.bump.alloc_slice_fill_iter(args),
        })
    }

    fn equal(&mut self, a: Predicate<'a>, b: Predicate<'a>) -> Result<bool, Failure> {
        self.step(0)?;
        if a.trait_ != b.trait_ || a.args.len() != b.args.len() {
            return Ok(false);
        }
        let mut left = a.args.to_vec();
        let mut right = b.args.to_vec();
        if a.trait_
            .and_then(nash_ast::primitives::ReprTrait::of)
            .is_some()
        {
            for term in left.iter_mut().chain(&mut right) {
                let canonical = self.canonical(term, 0)?;
                let subject =
                    crate::kinds::representation_subject(self.bump, self.kinds, canonical);
                *term = self.term(subject, &BTreeMap::new(), 0)?;
            }
        }
        let mut pending: Vec<_> = left.into_iter().zip(right).collect();
        while let Some((a, b)) = pending.pop() {
            self.step(0)?;
            if a.con != b.con || a.args.len() != b.args.len() {
                return Ok(false);
            }
            pending.extend(a.args.iter().copied().zip(b.args.iter().copied()));
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
            let Some(name) = pred.trait_ else {
                continue;
            };
            if let Some(repr) = nash_ast::primitives::ReprTrait::of(name) {
                for superclass in repr.supers() {
                    pending.push(Predicate {
                        trait_: Some(superclass.qualified()),
                        args: pred.args,
                    });
                }
                continue;
            }
            let trait_ = self.tables.traits[&name];
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
        if let Some(required) = wanted.trait_.and_then(nash_ast::primitives::ReprTrait::of) {
            let typ = self.canonical(wanted.args[0], 0)?;
            return match crate::kinds::repr_of(self.bump, self.kinds, typ) {
                Some(actual) if required.admits().contains(actual) => Ok(()),
                _ => Err(Failure::Missing),
            };
        }
        if wanted.trait_.is_none() {
            let head = self.canonical(wanted.args[0], 0)?;
            let args: Vec<_> = wanted.args[1..]
                .iter()
                .map(|arg| self.canonical(arg, 0))
                .collect::<Result<_, _>>()?;
            let group = Default::default();
            let mut formation = crate::kinds::Formation::new(self.bump, self.kinds, &group);
            formation
                .reduce(Pred::Apply {
                    head,
                    args: self.bump.alloc_slice_fill_iter(args),
                })
                .map_err(|_| Failure::Missing)?;
            for pred in formation.predicates {
                let child = self.predicate(&pred, &BTreeMap::new())?;
                if self.equal(child, wanted)? {
                    return Err(Failure::Missing);
                }
                self.resolve(given, child, active)?;
            }
            return Ok(());
        }
        if self.tables.has_structural_eq()
            && wanted.trait_ == Some(nash_ast::primitives::eq_trait())
            && wanted.args.len() == 1
        {
            let big = Predicate {
                trait_: Some(nash_ast::primitives::ReprTrait::Big.qualified()),
                args: &wanted.args[..1],
            };
            match self.resolve(given, big, active) {
                Ok(()) => return Ok(()),
                Err(Failure::Missing) => {}
                Err(reason) => return Err(reason),
            }
        }
        if self.tables.has_reflexive_lift()
            && wanted.trait_ == Some(nash_ast::primitives::lift_trait())
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
            let big = Predicate {
                trait_: Some(nash_ast::primitives::ReprTrait::Big.qualified()),
                args: &wanted.args[..1],
            };
            match self.resolve(given, big, active) {
                Ok(()) => return Ok(()),
                Err(Failure::Missing) => {}
                Err(reason) => return Err(reason),
            }
        }
        for pred in active.iter() {
            if self.equal(*pred, wanted)? {
                return Err(Failure::Cycle);
            }
        }
        let canonical_args: Vec<_> = wanted
            .args
            .iter()
            .map(|arg| self.canonical(arg, 0))
            .collect::<Result<_, _>>()?;
        let mut selected = None;
        for (key, info) in self
            .tables
            .impls
            .iter()
            .filter(|(key, _)| Some(key.trait_) == wanted.trait_)
        {
            if let nash_ast::head::Match::Yes(arguments) = nash_ast::head::matches(
                &mut nash_ast::head::Canonical,
                key.heads,
                &canonical_args,
                info.variables.len(),
                &mut self.remaining,
            )
            .map_err(|_| Failure::Limit)?
            {
                selected = Some((*info, arguments));
                break;
            }
        }
        let (impl_, arguments) = selected.ok_or(Failure::Missing)?;
        let mut subst = BTreeMap::new();
        for (name, argument) in impl_.variables.iter().zip(arguments) {
            subst.insert(*name, self.term(argument, &BTreeMap::new(), 0)?);
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
    let canonical_subst: BTreeMap<_, _> = trait_
        .parameters
        .iter()
        .zip(impl_.heads)
        .map(|(parameter, head)| {
            (
                *parameter,
                head.value.to_type(bump, impl_.variables, head.region),
            )
        })
        .collect();
    let mut resolver = Resolver {
        bump,
        tables,
        remaining: WORK_LIMIT,
        kinds: kind_env,
    };
    let givens = resolver.givens(impl_.context);
    for (index, superclass) in trait_.supers.iter().enumerate() {
        let checked = (|| {
            let givens = givens.as_ref().map_err(|reason| *reason)?;
            let mut subst = BTreeMap::new();
            for (parameter, typ) in &canonical_subst {
                subst.insert(*parameter, resolver.term(typ, &BTreeMap::new(), 0)?);
            }
            let wanted = resolver.predicate(superclass, &subst)?;
            resolver.resolve(givens, wanted, &mut Vec::new())
        })();
        if let Err(reason) = checked {
            return Err(vec![Error::MissingSuperclass {
                region: impl_.region,
                trait_: impl_.trait_,
                heads: impl_.heads,
                superclass: bump.alloc(crate::kinds::substitute_predicate(
                    bump,
                    &canonical_subst,
                    *superclass,
                )),
                index: index as u16,
                reason,
            }]);
        }
    }
    Ok(())
}
