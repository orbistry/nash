//! Close compile-time evidence references without introducing runtime dictionaries.
//!
//! Type substitution is simultaneous. Payloads may remain open when code erases
//! them, as a phantom parameter does when no use fixes it. Superclass
//! resolution accepts open types; it fails only where an open type would select
//! the impl. Runtime layout demands are a separate part of specialization identity.
use std::collections::HashMap;

use nash_ast::fuel::{OutOfFuel, tick};
use nash_ast::{AliasType, Evidence, Head, ImplRef, NodeId, Pred, QualifiedName, Type, primitives};
use nash_can::environment::Tables;
use nash_plutus::arena::Arena;
use nash_region::Located;

use crate::ty_of::Substitution;

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
    #[error("evidence still contains a given or superclass projection")]
    UnresolvedEvidence,
    #[error("code generation ran out of compilation steps")]
    OutOfFuel,
    #[error("cannot resolve superclass: {0:?}")]
    Resolution(nash_solve::evidence::Failure),
}

impl From<OutOfFuel> for Error<'_> {
    fn from(OutOfFuel: OutOfFuel) -> Self {
        Self::OutOfFuel
    }
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
    Grounder { arena, tables }.one(evidence, subst, givens)
}

pub fn ground_arguments<'a>(
    arena: &'a Arena,
    tables: &Tables<'a>,
    evidence: &[Evidence<'a>],
    subst: &Substitution<'a>,
    givens: &HashMap<NodeId, &[Evidence<'a>]>,
) -> Result<&'a [Evidence<'a>], Error<'a>> {
    Grounder { arena, tables }.arguments(evidence, subst, givens)
}

pub fn executable_identity<'a>(
    evidence: &Evidence<'a>,
) -> Result<ExecutableEvidence<'a>, Error<'a>> {
    fn visit<'a>(evidence: &Evidence<'a>) -> Result<ExecutableEvidence<'a>, Error<'a>> {
        tick()?;
        Ok(match evidence {
            Evidence::Repr { .. } => ExecutableEvidence::Erased,
            Evidence::ReflexiveLift { .. } => ExecutableEvidence::Identity,
            Evidence::StructuralEq { .. } => ExecutableEvidence::StructuralEq,
            Evidence::Impl {
                impl_,
                type_args,
                args,
            } => {
                check_heads(impl_.key.heads, type_args.len())?;
                let mut children = Vec::new();
                for arg in *args {
                    let child = visit(arg)?;
                    if child != ExecutableEvidence::Erased {
                        children.push(child);
                    }
                }
                ExecutableEvidence::Impl(*impl_, children)
            }
            Evidence::Given { .. } | Evidence::Super { .. } | Evidence::Specialize { .. } => {
                return Err(Error::UnresolvedEvidence);
            }
        })
    }
    visit(evidence)
}

// Impl keys participate in recursive ordering and hashing. Check them before
// looking them up or copying them into an executable key.
fn check_heads<'a>(heads: &[Head<'a>], variables: usize) -> Result<(), Error<'a>> {
    let mut pending: Vec<_> = heads.iter().collect();
    while let Some(head) = pending.pop() {
        tick()?;
        match head {
            Head::Var { index, .. } if usize::from(*index) >= variables => {
                return Err(Error::MalformedImpl);
            }
            Head::Var { .. } => {}
            Head::Named { args, .. } | Head::Tuple(args) => {
                if matches!(head, Head::Tuple(_)) && args.len() < 2 {
                    return Err(Error::MalformedImpl);
                }
                pending.extend(args.iter());
            }
            Head::Function(from, to) => {
                pending.push(from);
                pending.push(to);
            }
        }
    }
    Ok(())
}

struct Grounder<'a, 't> {
    arena: &'a Arena,
    tables: &'t Tables<'a>,
}

impl<'a> Grounder<'a, '_> {
    fn one(
        &mut self,
        evidence: &Evidence<'a>,
        subst: &Substitution<'a>,
        givens: &HashMap<NodeId, &[Evidence<'a>]>,
    ) -> Result<Evidence<'a>, Error<'a>> {
        tick()?;
        Ok(match evidence {
            Evidence::Given { binder, index } => {
                let values = givens.get(binder).ok_or(Error::MissingGiven(*binder))?;
                let value = values.get(usize::from(*index)).ok_or(Error::GivenIndex {
                    binder: *binder,
                    index: *index,
                })?;
                return self.one(value, subst, givens);
            }
            Evidence::Super { of, index } => {
                let of = self.one(of, subst, givens)?;
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
                let predicate = self.super_predicate(&of, *index)?;
                self.validate_predicate(predicate)?;
                let resolved =
                    nash_solve::evidence::resolve(self.arena.as_bump(), self.tables, &predicate)
                        .map_err(resolution_error)?
                        .ok_or(Error::NoEvidence)?;
                return self.one(&resolved, &Substitution::new(), &HashMap::new());
            }
            Evidence::Specialize { trait_, args } => {
                // Method B: the specialization key holds these types, so each
                // use selects its impl here, with the types substituted.
                let types = args
                    .iter()
                    .map(|typ| self.typ(typ, subst))
                    .collect::<Result<Vec<_>, _>>()?;
                let predicate = Pred::Trait {
                    trait_: *trait_,
                    args: self.arena.alloc_slice_copy(&types),
                };
                let resolved =
                    nash_solve::evidence::resolve(self.arena.as_bump(), self.tables, &predicate)
                        .map_err(resolution_error)?
                        .ok_or(Error::NoEvidence)?;
                return self.one(&resolved, &Substitution::new(), &HashMap::new());
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
                check_heads(impl_.key.heads, type_args.len())?;
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
                    args: self.arguments(args, subst, givens)?,
                }
            }
        })
    }

    fn arguments(
        &mut self,
        evidence: &[Evidence<'a>],
        subst: &Substitution<'a>,
        givens: &HashMap<NodeId, &[Evidence<'a>]>,
    ) -> Result<&'a [Evidence<'a>], Error<'a>> {
        let values = evidence
            .iter()
            .map(|e| self.one(e, subst, givens))
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
        self.inspect_type(typ, subst)?;
        Ok(nash_can::types::substitute_type(
            self.arena.as_bump(),
            subst,
            typ,
        ))
    }

    fn inspect_type(
        &mut self,
        typ: &'a Located<Type<'a>>,
        subst: &Substitution<'a>,
    ) -> Result<(), Error<'a>> {
        let mut pending = vec![(typ, true)];
        while let Some((typ, replace)) = pending.pop() {
            tick()?;
            let mut push = |child| pending.push((child, replace));
            match &typ.value {
                Type::Var(name) => {
                    if replace && let Some(value) = subst.get(name) {
                        pending.push((*value, false));
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
    ) -> Result<Pred<'a>, Error<'a>> {
        let (trait_, args) = match evidence {
            Evidence::StructuralEq { typ } => (primitives::eq_trait(), vec![*typ]),
            Evidence::ReflexiveLift { typ } => (primitives::lift_trait(), vec![*typ, *typ]),
            Evidence::Impl {
                impl_, type_args, ..
            } => {
                let args = impl_
                    .key
                    .heads
                    .iter()
                    .map(|head| self.head_type(head, type_args))
                    .collect::<Result<Vec<_>, _>>()?;
                (impl_.key.trait_, args)
            }
            // `one` has resolved givens/projections and handled representation
            // superclasses directly before calling this private helper.
            Evidence::Repr { .. }
            | Evidence::Given { .. }
            | Evidence::Super { .. }
            | Evidence::Specialize { .. } => {
                unreachable!("superclass predicate requires resolved non-representation evidence")
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
    ) -> Result<&'a Located<Type<'a>>, Error<'a>> {
        tick()?;
        let typ = match head {
            Head::Var { index, .. } => {
                return variables
                    .get(usize::from(*index))
                    .copied()
                    .ok_or(Error::MalformedImpl);
            }
            Head::Named { reference, args } => {
                let args = args
                    .iter()
                    .map(|head| self.head_type(head, variables))
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
                    .map(|head| self.head_type(head, variables))
                    .collect::<Result<Vec<_>, _>>()?;
                Type::Tuple {
                    first: args[0],
                    second: args[1],
                    rest: self.arena.alloc_slice_copy(&args[2..]),
                }
            }
            Head::Function(from, to) => Type::Lambda {
                from: self.head_type(from, variables)?,
                to: self.head_type(to, variables)?,
            },
        };
        Ok(self.arena.alloc(Located::at_zero(typ)))
    }

    fn validate_predicate(&self, predicate: Pred<'a>) -> Result<(), Error<'a>> {
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
        Ok(())
    }
}

/// A superclass or specialization lookup fails for a reason of its own, or because
/// the compile ran out of fuel.
fn resolution_error(error: nash_solve::evidence::Error<'_>) -> Error<'_> {
    match error.reason {
        nash_solve::evidence::Failure::OutOfFuel => Error::OutOfFuel,
        reason => Error::Resolution(reason),
    }
}

#[cfg(test)]
mod tests;
