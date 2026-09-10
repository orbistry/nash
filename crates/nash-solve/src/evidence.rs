//! Impl evidence for ground canonical predicates, outside the inference loop.
use bumpalo::Bump;
use nash_ast::{Evidence, ImplRef, Pred, QualifiedName, Type};
use nash_can::environment::Tables;
use nash_region::Located;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    MissingImpl,
    NonGround,
    Limit,
}

#[derive(Debug)]
pub struct Error<'a> {
    pub predicate: Pred<'a>,
    pub reason: Failure,
}

/// Resolve a well-kinded canonical predicate using the complete impl table.
/// Nominal aliases retain their identity. Open types are rejected; resolution
/// never chooses a type or kind to make an impl match. Recursive contexts and
/// structural traversal share a bounded work budget.
pub fn resolve<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    pred: &Pred<'a>,
) -> Result<Option<Evidence<'a>>, Error<'a>> {
    Resolver {
        bump,
        tables,
        remaining: 16_384,
    }
    .resolve(pred, 0)
}

#[derive(PartialEq, Eq)]
enum Constructor<'a> {
    Named(QualifiedName<'a>),
    Tuple(usize),
    Function,
    Record(Vec<&'a str>),
}

// A region-independent, nominal view for reflexive equality. Alias bodies and
// unsupplied bound parameters are not part of a nominal application.
#[derive(PartialEq, Eq)]
struct Term<'a> {
    con: Constructor<'a>,
    args: Vec<Term<'a>>,
}

struct Resolver<'t, 'a> {
    bump: &'a Bump,
    tables: &'t Tables<'a>,
    remaining: usize,
}

enum Resolution<'a> {
    Complete(Evidence<'a>),
    Apply {
        children: Vec<Pred<'a>>,
    },
    Impl {
        impl_: ImplRef<'a>,
        type_args: &'a [&'a Located<Type<'a>>],
        children: Vec<Pred<'a>>,
    },
}

enum Work<'a> {
    Resolve(Pred<'a>, usize),
    Assemble(ImplRef<'a>, &'a [&'a Located<Type<'a>>], usize),
    Apply(usize),
}

impl<'a> Resolver<'_, 'a> {
    // Charge every node that substitute_type will copy before allocating the
    // result. Open alias bodies are retained; filled bodies are traversed.
    fn substitution_work(&mut self, typ: &Type<'a>, depth: usize) -> Result<(), Failure> {
        self.step(depth)?;
        match typ {
            Type::App { head, args } => {
                self.substitution_work(&head.value, depth + 1)?;
                for arg in *args {
                    self.substitution_work(&arg.value, depth + 1)?;
                }
            }
            Type::Named { args, .. } => {
                for arg in *args {
                    self.substitution_work(&arg.value, depth + 1)?;
                }
            }
            Type::Alias {
                arguments, target, ..
            } => {
                for arg in *arguments {
                    self.substitution_work(&arg.typ.value, depth + 1)?;
                }
                if let nash_ast::AliasType::Filled { typ, .. } = target {
                    self.substitution_work(&typ.value, depth + 1)?;
                }
            }
            Type::Lambda { from, to } => {
                self.substitution_work(&from.value, depth + 1)?;
                self.substitution_work(&to.value, depth + 1)?;
            }
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                self.substitution_work(&first.value, depth + 1)?;
                self.substitution_work(&second.value, depth + 1)?;
                for arg in *rest {
                    self.substitution_work(&arg.value, depth + 1)?;
                }
            }
            Type::Record { fields, .. } => {
                for field in *fields {
                    self.substitution_work(&field.typ.value, depth + 1)?;
                }
            }
            Type::Var(_) => {}
        }
        Ok(())
    }

    fn step(&mut self, depth: usize) -> Result<(), Failure> {
        if depth >= 128 || self.remaining == 0 {
            return Err(Failure::Limit);
        }
        self.remaining -= 1;
        Ok(())
    }

    fn term(&mut self, typ: &Type<'a>, depth: usize) -> Result<Term<'a>, Failure> {
        self.step(depth)?;
        let (con, args) = match typ {
            Type::Var(_) => return Err(Failure::NonGround),
            Type::App { head, args } => {
                let mut head = self.term(&head.value, depth + 1)?;
                if !matches!(head.con, Constructor::Named(_)) {
                    return Err(Failure::MissingImpl);
                }
                for arg in *args {
                    head.args.push(self.term(&arg.value, depth + 1)?);
                }
                return Ok(head);
            }
            Type::Named { reference, args } => (Constructor::Named(*reference), args.to_vec()),
            Type::Alias {
                reference,
                arguments,
                ..
            } => (
                Constructor::Named(*reference),
                arguments.iter().map(|arg| arg.typ).collect(),
            ),

            Type::Tuple {
                first,
                second,
                rest,
            } => (
                Constructor::Tuple(rest.len() + 2),
                [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .collect(),
            ),
            Type::Lambda { from, to } => (Constructor::Function, vec![*from, *to]),
            Type::Record { fields } => (
                Constructor::Record(fields.iter().map(|field| field.field).collect()),
                fields.iter().map(|field| field.typ).collect(),
            ),
        };
        let args = args
            .into_iter()
            .map(|arg| self.term(&arg.value, depth + 1))
            .collect::<Result<_, _>>()?;
        Ok(Term { con, args })
    }

    fn resolve(
        &mut self,
        pred: &Pred<'a>,
        depth: usize,
    ) -> Result<Option<Evidence<'a>>, Error<'a>> {
        let mut pending = vec![Work::Resolve(*pred, depth)];
        let mut evidence = Vec::new();
        while let Some(work) = pending.pop() {
            match work {
                Work::Resolve(pred, depth) => match self.resolve_step(&pred, depth)? {
                    Resolution::Complete(proof) => evidence.push(Some(proof)),
                    Resolution::Apply { children } => {
                        pending.push(Work::Apply(children.len()));
                        pending.extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|pred| Work::Resolve(pred, depth + 1)),
                        );
                    }
                    Resolution::Impl {
                        impl_,
                        type_args,
                        children,
                    } => {
                        pending.push(Work::Assemble(impl_, type_args, children.len()));
                        pending.extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|pred| Work::Resolve(pred, depth + 1)),
                        );
                    }
                },
                Work::Apply(count) => {
                    evidence.truncate(evidence.len() - count);
                    evidence.push(None);
                }
                Work::Assemble(impl_, type_args, count) => {
                    let children = evidence.split_off(evidence.len() - count);
                    evidence.push(Some(Evidence::Impl {
                        impl_,
                        type_args,
                        args: self.bump.alloc_slice_fill_iter(
                            children.into_iter().flatten().collect::<Vec<_>>(),
                        ),
                    }));
                }
            }
        }
        Ok(evidence.pop().expect("root evidence"))
    }

    fn resolve_step(&mut self, pred: &Pred<'a>, depth: usize) -> Result<Resolution<'a>, Error<'a>> {
        let error = |reason| Error {
            predicate: *pred,
            reason,
        };
        self.step(depth).map_err(error)?;
        let terms = pred
            .types()
            .map(|arg| self.term(&arg.value, 0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        let group = Default::default();
        let mut formation = nash_can::kinds::Formation::new(self.bump, &self.tables.kinds, &group);
        for typ in pred.types() {
            formation
                .typ(typ)
                .map_err(|_| error(Failure::MissingImpl))?;
        }
        // Every input is ground. Formation must fully discharge its internal
        // requirements before a ground evidence marker can be issued.
        if !formation.predicates.is_empty() {
            return Err(error(Failure::NonGround));
        }
        if matches!(pred, Pred::Apply { .. }) {
            formation
                .reduce(*pred)
                .map_err(|_| error(Failure::MissingImpl))?;
            return Ok(Resolution::Apply {
                children: formation.predicates,
            });
        }
        let trait_ = pred.trait_ref().expect("trait predicate");
        let args = pred.args();
        if let Some(required) = nash_ast::primitives::ReprTrait::of(trait_) {
            return match nash_can::kinds::repr_of(self.bump, &self.tables.kinds, args[0]) {
                Some(actual) if required.admits().contains(actual) => {
                    Ok(Resolution::Complete(Evidence::Repr {
                        trait_: required,
                        typ: args[0],
                    }))
                }
                _ => Err(error(Failure::MissingImpl)),
            };
        }
        let big = args.first().is_some_and(|typ| {
            nash_can::kinds::repr_of(self.bump, &self.tables.kinds, typ)
                == Some(nash_ast::primitives::Repr::Big)
        });
        if trait_ == nash_ast::primitives::eq_trait()
            && self.tables.has_structural_eq()
            && args.len() == 1
            && big
        {
            return Ok(Resolution::Complete(Evidence::StructuralEq {
                typ: args[0],
            }));
        }
        if trait_ == nash_ast::primitives::lift_trait()
            && self.tables.has_reflexive_lift()
            && matches!(terms.as_slice(), [first, second] if first == second)
            && big
        {
            return Ok(Resolution::Complete(Evidence::ReflexiveLift {
                typ: args[0],
            }));
        }
        let mut selected = None;
        for (key, info) in self.tables.impls_for(trait_) {
            if let nash_ast::head::Match::Yes(arguments) = nash_ast::head::matches(
                &mut nash_ast::head::Canonical,
                key.heads,
                args,
                info.variables.len(),
                &mut self.remaining,
            )
            .map_err(|_| error(Failure::Limit))?
            {
                selected = Some((*key, *info, arguments));
                break;
            }
        }
        let (key, info, type_args) = selected.ok_or_else(|| error(Failure::MissingImpl))?;
        let substitution = info
            .variables
            .iter()
            .copied()
            .zip(type_args.iter().copied())
            .collect();
        let mut children = Vec::new();
        for context in info.context {
            for typ in context.types() {
                self.substitution_work(&typ.value, 0).map_err(error)?;
            }
            children.push(nash_can::kinds::substitute_predicate(
                self.bump,
                &substitution,
                *context,
            ));
        }
        Ok(Resolution::Impl {
            impl_: ImplRef {
                home: info.home,
                key,
            },
            type_args: self.bump.alloc_slice_copy(&type_args),
            children,
        })
    }
}

#[cfg(test)]
mod predicate_tests {
    use super::*;
    use nash_ast::primitives::{self, ReprTrait};

    fn named<'a>(
        bump: &'a Bump,
        name: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    ) -> &'a Located<Type<'a>> {
        bump.alloc(Located::at_zero(Type::Named {
            reference: QualifiedName {
                home: primitives::builtin_home(),
                name,
            },
            args,
        }))
    }

    #[test]
    fn ground_representation_returns_a_compile_time_marker() {
        let bump = Bump::new();
        let tables = Tables::default();
        let int = named(&bump, "int", &[]);
        let pred = Pred::Trait {
            trait_: ReprTrait::Storable.qualified(),
            args: bump.alloc_slice_copy(&[int]),
        };
        assert!(matches!(
            resolve(&bump, &tables, &pred),
            Ok(Some(Evidence::Repr {
                trait_: ReprTrait::Storable,
                ..
            }))
        ));
    }

    #[test]
    fn ground_apply_checks_context_without_producing_evidence() {
        let bump = Bump::new();
        let tables = Tables::default();
        let int = named(&bump, "int", &[]);
        let pred = Pred::Apply {
            head: named(&bump, "list", &[]),
            args: bump.alloc_slice_copy(&[int]),
        };
        assert!(matches!(resolve(&bump, &tables, &pred), Ok(None)));
        let function = &*bump.alloc(Located::at_zero(Type::Lambda { from: int, to: int }));
        let pred = Pred::Apply {
            head: named(&bump, "list", &[]),
            args: bump.alloc_slice_copy(&[function]),
        };
        assert!(matches!(
            resolve(&bump, &tables, &pred),
            Err(Error {
                reason: Failure::MissingImpl,
                ..
            })
        ));
    }

    #[test]
    fn known_big_head_does_not_hide_an_invalid_nested_application() {
        let bump = Bump::new();
        let tables = Tables::default();
        let int = named(&bump, "int", &[]);
        let invalid = named(&bump, "List", bump.alloc_slice_copy(&[int]));
        let pred = Pred::Trait {
            trait_: ReprTrait::Big.qualified(),
            args: bump.alloc_slice_copy(&[invalid]),
        };
        assert!(matches!(
            resolve(&bump, &tables, &pred),
            Err(Error {
                reason: Failure::MissingImpl,
                ..
            })
        ));
    }
}
