//! Close compile-time evidence references without introducing runtime dictionaries.
//!
//! Type substitution is simultaneous. Payloads may remain open when code erases
//! them; only instance superclass resolution requires a concrete predicate. Runtime layout
//! demands are a separate part of specialization identity.
use std::collections::HashMap;

use nash_ast::{AliasType, Evidence, Head, ImplRef, NodeId, Pred, QualifiedName, Type, primitives};
use nash_can::environment::Tables;
use nash_plutus::arena::Arena;
use nash_region::Located;

use crate::ty_of::Substitution;

const MAX_DEPTH: usize = 128;
const MAX_WORK: usize = 16_384;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error<'a> {
    #[error("no evidence supplied for binder {0:?}")]
    MissingGiven(NodeId),
    #[error("binder {binder:?} has no evidence slot {index}")]
    GivenIndex { binder: NodeId, index: u16 },
    #[error("trait {trait_:?} has no superclass {index}")]
    SuperIndex {
        trait_: QualifiedName<'a>,
        index: u16,
    },
    #[error("unknown trait {0:?}")]
    UnknownTrait(QualifiedName<'a>),
    #[error("unknown implementation {0:?}")]
    UnknownImpl(Box<ImplRef<'a>>),
    #[error("implementation evidence has invalid arguments")]
    MalformedImpl,
    #[error("trait predicate has invalid arguments")]
    MalformedPredicate,
    #[error("superclass does not have an evidence slot")]
    NoEvidence,
    #[error("superclass resolution requires a ground type, found {0}")]
    NonGround(&'a str),
    #[error("evidence still contains a given or superclass projection")]
    UnresolvedEvidence,
    #[error("evidence normalization exceeded its work or depth limit")]
    Limit,
    #[error("cannot resolve superclass: {0:?}")]
    Resolution(nash_solve::evidence::Failure),
}

/// Executable choice only: type and representation layout demands belong in a
/// separate key. Representation proofs are omitted from implementation children.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ExecutableEvidence<'a> {
    Impl(ImplRef<'a>, Vec<Self>),
    Identity,
    StructuralEq,
    Erased,
}

pub fn ground<'a>(
    arena: &'a Arena,
    tables: &Tables<'a>,
    evidence: &Evidence<'a>,
    subst: &Substitution<'a>,
    givens: &HashMap<NodeId, &[Evidence<'a>]>,
) -> Result<Evidence<'a>, Error<'a>> {
    Grounder {
        arena,
        tables,
        remaining: MAX_WORK,
    }
    .one(evidence, subst, givens, 0)
}

pub fn ground_arguments<'a>(
    arena: &'a Arena,
    tables: &Tables<'a>,
    evidence: &[Evidence<'a>],
    subst: &Substitution<'a>,
    givens: &HashMap<NodeId, &[Evidence<'a>]>,
) -> Result<&'a [Evidence<'a>], Error<'a>> {
    Grounder {
        arena,
        tables,
        remaining: MAX_WORK,
    }
    .arguments(evidence, subst, givens, 0)
}

pub fn executable_identity<'a>(
    evidence: &Evidence<'a>,
) -> Result<ExecutableEvidence<'a>, Error<'a>> {
    fn visit<'a>(
        evidence: &Evidence<'a>,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<ExecutableEvidence<'a>, Error<'a>> {
        step(remaining, depth)?;
        Ok(match evidence {
            Evidence::Repr { .. } => ExecutableEvidence::Erased,
            Evidence::ReflexiveLift { .. } => ExecutableEvidence::Identity,
            Evidence::StructuralEq { .. } => ExecutableEvidence::StructuralEq,
            Evidence::Impl {
                impl_,
                type_args,
                args,
            } => {
                check_heads(remaining, impl_.key.heads, type_args.len(), depth)?;
                let mut children = Vec::new();
                for arg in *args {
                    let child = visit(arg, remaining, depth + 1)?;
                    if child != ExecutableEvidence::Erased {
                        children.push(child);
                    }
                }
                ExecutableEvidence::Impl(*impl_, children)
            }
            Evidence::Given { .. } | Evidence::Super { .. } => {
                return Err(Error::UnresolvedEvidence);
            }
        })
    }
    let mut remaining = MAX_WORK;
    visit(evidence, &mut remaining, 0)
}

fn step<'a>(remaining: &mut usize, depth: usize) -> Result<(), Error<'a>> {
    if depth > MAX_DEPTH || *remaining == 0 {
        return Err(Error::Limit);
    }
    *remaining -= 1;
    Ok(())
}

// Impl keys participate in recursive ordering and hashing. Check them before
// looking them up or copying them into an executable key.
fn check_heads<'a>(
    remaining: &mut usize,
    heads: &[Head<'a>],
    variables: usize,
    depth: usize,
) -> Result<(), Error<'a>> {
    if heads.len() > *remaining {
        return Err(Error::Limit);
    }
    let mut pending: Vec<_> = heads.iter().map(|head| (head, depth)).collect();
    while let Some((head, depth)) = pending.pop() {
        step(remaining, depth)?;
        match head {
            Head::Var(index) if usize::from(*index) >= variables => {
                return Err(Error::MalformedImpl);
            }
            Head::Var(_) => {}
            Head::Named { args, .. } | Head::Tuple(args) => {
                if matches!(head, Head::Tuple(_)) && args.len() < 2 {
                    return Err(Error::MalformedImpl);
                }
                if args.len() > *remaining {
                    return Err(Error::Limit);
                }
                pending.extend(args.iter().map(|arg| (arg, depth + 1)));
            }
            Head::Function(from, to) => {
                pending.push((from, depth + 1));
                pending.push((to, depth + 1));
            }
        }
    }
    Ok(())
}

struct Grounder<'a, 't> {
    arena: &'a Arena,
    tables: &'t Tables<'a>,
    remaining: usize,
}

impl<'a> Grounder<'a, '_> {
    fn one(
        &mut self,
        evidence: &Evidence<'a>,
        subst: &Substitution<'a>,
        givens: &HashMap<NodeId, &[Evidence<'a>]>,
        depth: usize,
    ) -> Result<Evidence<'a>, Error<'a>> {
        step(&mut self.remaining, depth)?;
        Ok(match evidence {
            Evidence::Given { binder, index } => {
                let values = givens.get(binder).ok_or(Error::MissingGiven(*binder))?;
                let value = values.get(usize::from(*index)).ok_or(Error::GivenIndex {
                    binder: *binder,
                    index: *index,
                })?;
                return self.one(value, subst, givens, depth + 1);
            }
            Evidence::Super { of, index } => {
                let of = self.one(of, subst, givens, depth + 1)?;
                // A known representation proof entails its builtin parents
                // even when the carried type is erased and remains open.
                if let Evidence::Repr { trait_, typ } = of {
                    let superclass =
                        trait_
                            .supers()
                            .get(usize::from(*index))
                            .ok_or(Error::SuperIndex {
                                trait_: trait_.qualified(),
                                index: *index,
                            })?;
                    return Ok(Evidence::Repr {
                        trait_: *superclass,
                        typ,
                    });
                }
                let predicate = self.super_predicate(&of, *index, depth + 1)?;
                self.validate_predicate(predicate)?;
                let resolved =
                    nash_solve::evidence::resolve(self.arena.as_bump(), self.tables, &predicate)
                        .map_err(|error| {
                            if error.reason == nash_solve::evidence::Failure::Limit {
                                Error::Limit
                            } else {
                                Error::Resolution(error.reason)
                            }
                        })?
                        .ok_or(Error::NoEvidence)?;
                return self.one(&resolved, &Substitution::new(), &HashMap::new(), depth + 1);
            }
            Evidence::Repr { trait_, typ } => Evidence::Repr {
                trait_: *trait_,
                typ: self.typ(typ, subst)?,
            },
            Evidence::ReflexiveLift { typ } => Evidence::ReflexiveLift {
                typ: self.typ(typ, subst)?,
            },
            Evidence::StructuralEq { typ } => Evidence::StructuralEq {
                typ: self.typ(typ, subst)?,
            },
            Evidence::Impl {
                impl_,
                type_args,
                args,
            } => {
                check_heads(&mut self.remaining, impl_.key.heads, type_args.len(), depth)?;
                let info = self
                    .tables
                    .impls
                    .get(&impl_.key)
                    .filter(|info| info.home == impl_.home)
                    .ok_or_else(|| Error::UnknownImpl(Box::new(*impl_)))?;
                if type_args.len() != info.variables.len()
                    || args.len()
                        != info
                            .context
                            .iter()
                            .filter(|pred| pred.trait_ref().is_some())
                            .count()
                {
                    return Err(Error::MalformedImpl);
                }
                let types = type_args
                    .iter()
                    .map(|typ| self.typ(typ, subst))
                    .collect::<Result<Vec<_>, _>>()?;
                Evidence::Impl {
                    impl_: *impl_,
                    type_args: self.arena.alloc_slice_copy(&types),
                    args: self.arguments(args, subst, givens, depth + 1)?,
                }
            }
        })
    }

    fn arguments(
        &mut self,
        evidence: &[Evidence<'a>],
        subst: &Substitution<'a>,
        givens: &HashMap<NodeId, &[Evidence<'a>]>,
        depth: usize,
    ) -> Result<&'a [Evidence<'a>], Error<'a>> {
        if evidence.len() > self.remaining {
            return Err(Error::Limit);
        }
        let values = evidence
            .iter()
            .map(|e| self.one(e, subst, givens, depth))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.arena.alloc_slice_fill_iter(values))
    }

    fn typ(
        &mut self,
        typ: &'a Located<Type<'a>>,
        subst: &Substitution<'a>,
    ) -> Result<&'a Located<Type<'a>>, Error<'a>> {
        // substitute_type is recursive, so bound its input and replacements
        // before calling it. Replacement types are not substituted again.
        self.inspect_type(typ, Some(subst), false)?;
        Ok(nash_can::types::substitute_type(
            self.arena.as_bump(),
            subst,
            typ,
        ))
    }

    fn inspect_type(
        &mut self,
        typ: &'a Located<Type<'a>>,
        subst: Option<&Substitution<'a>>,
        require_ground: bool,
    ) -> Result<(), Error<'a>> {
        let mut pending = vec![(typ, 0, true)];
        while let Some((typ, depth, replace)) = pending.pop() {
            step(&mut self.remaining, depth)?;
            let mut push = |child| pending.push((child, depth + 1, replace));
            match &typ.value {
                Type::Var(name) => {
                    if let Some(value) = subst.filter(|_| replace).and_then(|subst| subst.get(name))
                    {
                        pending.push((*value, depth + 1, false));
                    } else if require_ground {
                        return Err(Error::NonGround(name));
                    }
                }
                Type::App { head, args } => {
                    push(*head);
                    for arg in *args {
                        push(*arg);
                    }
                }
                Type::Named { args, .. } => {
                    for arg in *args {
                        push(*arg);
                    }
                }
                Type::Lambda { from, to } => {
                    push(*from);
                    push(*to);
                }
                Type::Tuple {
                    first,
                    second,
                    rest,
                } => {
                    push(*first);
                    push(*second);
                    for item in *rest {
                        push(*item);
                    }
                }
                Type::Record { fields } => {
                    for field in *fields {
                        push(field.typ);
                    }
                }
                Type::Alias {
                    arguments, target, ..
                } => {
                    for arg in *arguments {
                        push(arg.typ);
                    }
                    // An open body binds alias parameters. A filled body uses
                    // caller variables and participates in substitution.
                    if let AliasType::Filled { typ, .. } = target {
                        push(*typ);
                    }
                }
            }
        }
        Ok(())
    }

    fn super_predicate(
        &mut self,
        evidence: &Evidence<'a>,
        index: u16,
        depth: usize,
    ) -> Result<Pred<'a>, Error<'a>> {
        let (trait_, args) = match evidence {
            Evidence::Repr { trait_, typ } => {
                let super_ = trait_
                    .supers()
                    .get(usize::from(index))
                    .ok_or(Error::SuperIndex {
                        trait_: trait_.qualified(),
                        index,
                    })?;
                return Ok(Pred::Implied {
                    trait_: super_.qualified(),
                    args: self.arena.alloc_slice_copy(&[*typ]),
                });
            }
            Evidence::StructuralEq { typ } => (primitives::eq_trait(), vec![*typ]),
            Evidence::ReflexiveLift { typ } => (primitives::lift_trait(), vec![*typ, *typ]),
            Evidence::Impl {
                impl_, type_args, ..
            } => {
                let args = impl_
                    .key
                    .heads
                    .iter()
                    .map(|head| self.head_type(head, type_args, depth))
                    .collect::<Result<Vec<_>, _>>()?;
                (impl_.key.trait_, args)
            }
            Evidence::Given { .. } | Evidence::Super { .. } => {
                return Err(Error::UnresolvedEvidence);
            }
        };
        let info = self
            .tables
            .traits
            .get(&trait_)
            .ok_or(Error::UnknownTrait(trait_))?;
        if args.len() != info.parameters.len() {
            return Err(Error::MalformedPredicate);
        }
        let super_ = *info
            .supers
            .get(usize::from(index))
            .ok_or(Error::SuperIndex { trait_, index })?;
        let subst = info.parameters.iter().copied().zip(args).collect();
        let args = super_
            .args()
            .iter()
            .map(|typ| self.typ(typ, &subst))
            .collect::<Result<Vec<_>, _>>()?;
        let args = self.arena.alloc_slice_copy(&args);
        Ok(match super_ {
            Pred::Trait { trait_, .. } => Pred::Trait { trait_, args },
            Pred::Implied { trait_, .. } => Pred::Implied { trait_, args },
            Pred::Apply { head, .. } => Pred::Apply {
                head: self.typ(head, &subst)?,
                args,
            },
        })
    }

    fn head_type(
        &mut self,
        head: &Head<'a>,
        variables: &[&'a Located<Type<'a>>],
        depth: usize,
    ) -> Result<&'a Located<Type<'a>>, Error<'a>> {
        step(&mut self.remaining, depth)?;
        let typ = match head {
            Head::Var(index) => {
                return variables
                    .get(usize::from(*index))
                    .copied()
                    .ok_or(Error::MalformedImpl);
            }
            Head::Named { reference, args } => {
                let args = args
                    .iter()
                    .map(|head| self.head_type(head, variables, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Type::Named {
                    reference: *reference,
                    args: self.arena.alloc_slice_copy(&args),
                }
            }
            Head::Tuple(args) => {
                if args.len() < 2 {
                    return Err(Error::MalformedImpl);
                }
                let args = args
                    .iter()
                    .map(|head| self.head_type(head, variables, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Type::Tuple {
                    first: args[0],
                    second: args[1],
                    rest: self.arena.alloc_slice_copy(&args[2..]),
                }
            }
            Head::Function(from, to) => Type::Lambda {
                from: self.head_type(from, variables, depth + 1)?,
                to: self.head_type(to, variables, depth + 1)?,
            },
        };
        Ok(self.arena.alloc(Located::at_zero(typ)))
    }

    fn validate_predicate(&mut self, predicate: Pred<'a>) -> Result<(), Error<'a>> {
        if let Some(trait_) = predicate.trait_ref() {
            let arity = if primitives::ReprTrait::of(trait_).is_some() {
                1
            } else {
                self.tables
                    .traits
                    .get(&trait_)
                    .ok_or(Error::UnknownTrait(trait_))?
                    .parameters
                    .len()
            };
            if predicate.args().len() != arity {
                return Err(Error::MalformedPredicate);
            }
        }
        for typ in predicate.types() {
            self.inspect_type(typ, None, true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
