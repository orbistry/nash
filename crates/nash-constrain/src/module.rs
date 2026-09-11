//! Post-solve validator entry-point checks. These do not instantiate or default types.
use crate::Error;
use bumpalo::Bump;
use nash_ast::{
    AliasType, Annotation, Module, ModuleKind, Pred, Type,
    primitives::{Repr, ReprSet, ReprTrait},
};
use nash_can::kinds::{KindEnv, TypeInfo};
use nash_region::Located;

/// Check the solved lambda spine of a validator's main before code generation.
/// An unconstrained variable remains polymorphic here; validator code generation
/// chooses its boundary instantiation. Normal module roots are never defaulted.
pub fn check_main_parameters<'a>(
    bump: &'a Bump,
    module: &Module<'a>,
    annotation: Option<&'a Annotation<'a>>,
    kinds: &KindEnv<'a>,
) -> Result<(), Vec<Error<'a>>> {
    if !matches!(module.kind, ModuleKind::Validator(_)) {
        return Ok(());
    }
    let Some(annotation) = annotation else {
        return Ok(());
    };
    let mut typ = annotation.typ;
    let mut errors = vec![];
    let (parameter_regions, main_region) = main_regions(module);
    let mut index = 0;
    loop {
        typ = transparent(bump, kinds, typ);
        let Type::Lambda { from, to } = &typ.value else {
            break;
        };
        if is_term(bump, kinds, annotation.context, from) {
            errors.push(Error::MainParameterIsTerm {
                region: if from.region == nash_region::Region::zero() {
                    parameter_regions.get(index).copied().unwrap_or(main_region)
                } else {
                    from.region
                },
                index,
                typ: from,
            });
        }
        index += 1;
        typ = to;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn main_regions(module: &Module<'_>) -> (Vec<nash_region::Region>, nash_region::Region) {
    let mut decls = module.decls;
    loop {
        let (first, following, next) = match decls {
            nash_ast::Decls::Declare { definition, next } => (*definition, &[][..], *next),
            nash_ast::Decls::DeclareRec {
                definition,
                following,
                next,
            } => (*definition, *following, *next),
            nash_ast::Decls::Empty => return (vec![], nash_region::Region::zero()),
        };
        for def in std::iter::once(first).chain(following.iter().copied()) {
            match def {
                nash_ast::Def::Def { name, args, .. } if name.value == "main" => {
                    return (args.iter().map(|p| p.region).collect(), name.region);
                }
                nash_ast::Def::TypedDef { name, args, .. } if name.value == "main" => {
                    return (args.iter().map(|p| p.typ.region).collect(), name.region);
                }
                _ => {}
            }
        }
        decls = next;
    }
}

fn is_term<'a>(
    bump: &'a Bump,
    kinds: &KindEnv<'a>,
    context: &[Pred<'a>],
    typ: &'a Located<Type<'a>>,
) -> bool {
    if let Some(repr) = nash_can::kinds::repr_of(bump, kinds, typ) {
        return repr == Repr::Term;
    }
    let typ = transparent(bump, kinds, typ);
    let mut admitted = ReprSet::ALL;
    let mut pending = context.to_vec();
    let mut seen = std::collections::HashSet::new();
    while let Some(pred) = pending.pop() {
        if !seen.insert(pred.key()) {
            continue;
        }
        let Some(trait_) = pred.trait_ref() else {
            continue;
        };
        if let Some(repr) = ReprTrait::of(trait_) {
            if let [arg] = pred.args() {
                let arg = transparent(bump, kinds, arg);
                // Predicate keys compare canonical types without source regions.
                let left = Pred::Trait {
                    trait_,
                    args: bump.alloc_slice_copy(&[typ]),
                };
                let right = Pred::Trait {
                    trait_,
                    args: bump.alloc_slice_copy(&[arg]),
                };
                if left.key() == right.key() {
                    admitted = admitted.intersect(repr.admits());
                }
            }
        } else if let Some(supers) = kinds.superclasses.get(&trait_) {
            let substitution = supers
                .parameters
                .iter()
                .copied()
                .zip(pred.args().iter().copied())
                .collect();
            pending.extend(
                supers
                    .supers
                    .iter()
                    .map(|p| nash_can::kinds::substitute_predicate(bump, &substitution, *p)),
            );
        }
    }
    admitted.contains(Repr::Term)
        && !admitted.contains(Repr::Big)
        && !admitted.contains(Repr::Const)
}

/// Expand transparent aliases on the function spine and in predicate subjects.
/// A nominal record alias keeps its own Big/Term representation.
fn transparent<'a>(
    bump: &'a Bump,
    kinds: &KindEnv<'a>,
    mut typ: &'a Located<Type<'a>>,
) -> &'a Located<Type<'a>> {
    loop {
        typ = match &typ.value {
            Type::Alias {
                arguments,
                remaining: [],
                target,
                ..
            } => {
                let body = match target {
                    AliasType::Open(body) | AliasType::Filled { body, .. } => *body,
                };
                if matches!(body.value, Type::Record { .. }) {
                    return typ;
                }
                let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
                nash_can::types::substitute_type(bump, &substitution, body)
            }
            Type::Named { reference, args } => {
                let TypeInfo::Defined {
                    parameters,
                    alias: Some(body),
                    repr: None,
                    ..
                } = kinds.constructor(*reference)
                else {
                    return typ;
                };
                if parameters.len() != args.len() {
                    return typ;
                }
                let substitution = parameters
                    .iter()
                    .copied()
                    .zip(args.iter().copied())
                    .collect();
                nash_can::types::substitute_type(bump, &substitution, body)
            }
            Type::App { .. } => {
                let applied = nash_can::types::substitute_type(bump, &Default::default(), typ);
                if matches!(applied.value, Type::App { .. }) {
                    return applied;
                }
                applied
            }
            _ => return typ,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_ast::*;
    use nash_region::Region;
    fn module() -> Module<'static> {
        const DOCS: Docs<'static> = Docs::NoDocs(Region::zero());
        Module {
            kind: ModuleKind::Validator(Region::zero()),
            name: ModuleName {
                package: None,
                name: "V",
            },
            traits: &[],
            impls: &[],
            exports: Exports::Explicit(&[]),
            docs: &DOCS,
            decls: &Decls::Empty,
            unions: &[],
            aliases: &[],
            binops: &[],
        }
    }
    fn check<'a>(
        b: &'a Bump,
        kinds: &KindEnv<'a>,
        param: &'a Located<Type<'a>>,
        context: &'a [Pred<'a>],
    ) -> Result<(), Vec<Error<'a>>> {
        let unit = b.alloc(Located::at_zero(Type::unit()));
        let typ = b.alloc(Located::at_zero(Type::Lambda {
            from: param,
            to: unit,
        }));
        let ann = b.alloc(Annotation {
            free_vars: &[],
            context,
            typ,
        });
        check_main_parameters(b, &module(), Some(ann), kinds)
    }
    #[test]
    fn term_parameter() {
        let b = Bump::new();
        let mut kinds = KindEnv::default();
        let reference = QualifiedName {
            home: module().name,
            name: "option",
        };
        kinds.types.insert(
            reference,
            TypeInfo::Defined {
                kind: &Kind::Type,
                parameters: &[],
                context: &[],
                repr: Some(Repr::Term),
                alias: None,
            },
        );
        let typ = b.alloc(Located::at_zero(Type::Named {
            reference,
            args: &[],
        }));
        insta::assert_debug_snapshot!(check(&b, &kinds, typ, &[]));
    }
    #[test]
    fn constant_big_and_unconstrained_parameters() {
        let b = Bump::new();
        let kinds = KindEnv::default();
        for name in ["int", "Data", "unit"] {
            let typ = b.alloc(Located::at_zero(Type::Named {
                reference: QualifiedName {
                    home: primitives::builtin_home(),
                    name,
                },
                args: &[],
            }));
            assert!(check(&b, &kinds, typ, &[]).is_ok());
        }
        assert!(check(&b, &kinds, b.alloc(Located::at_zero(Type::Var("a"))), &[]).is_ok());
    }
    #[test]
    fn representation_context_distinguishes_term_from_storable() {
        let b = Bump::new();
        let kinds = KindEnv::default();
        let typ = b.alloc(Located::at_zero(Type::Var("a")));
        for repr in [
            ReprTrait::Term,
            ReprTrait::Big,
            ReprTrait::Const,
            ReprTrait::Storable,
            ReprTrait::Little,
        ] {
            let context = b.alloc_slice_fill_iter([Pred::Trait {
                trait_: repr.qualified(),
                args: b.alloc_slice_copy(&[&*typ]),
            }]);
            assert_eq!(
                check(&b, &kinds, typ, context).is_err(),
                repr == ReprTrait::Term
            );
        }
    }
    #[test]
    fn solved_zero_regions_use_the_source_parameter() {
        let b = Bump::new();
        let kinds = KindEnv::default();
        let unit = b.alloc(Located::at_zero(Type::unit()));
        let function = b.alloc(Located::at_zero(Type::Lambda {
            from: unit,
            to: unit,
        }));
        let typ = b.alloc(Located::at_zero(Type::Lambda {
            from: function,
            to: unit,
        }));
        let annotation = b.alloc(Annotation {
            free_vars: &[],
            context: &[],
            typ,
        });
        let pattern = b.alloc(Located::at(Region::one(), Pattern::Anything));
        let definition = b.alloc(Def::Def {
            name: b.alloc(Located::at_zero("main")),
            args: b.alloc_slice_copy(&[&*pattern]),
            body: b.alloc(Located::at_zero(Expr::Unit)),
        });
        let mut module = module();
        module.decls = b.alloc(Decls::Declare {
            definition,
            next: &Decls::Empty,
        });
        let errors = check_main_parameters(&b, &module, Some(annotation), &kinds).unwrap_err();
        assert!(
            matches!(errors[0], Error::MainParameterIsTerm { region, .. } if region == Region::one())
        );
    }

    #[test]
    fn superclass_term_requirement_is_checked() {
        let b = Bump::new();
        let mut kinds = KindEnv::default();
        let typ = b.alloc(Located::at_zero(Type::Var("a")));
        let bound = b.alloc(Located::at_zero(Type::Var("bound")));
        let trait_ = QualifiedName {
            home: module().name,
            name: "OnlyTerm",
        };
        kinds.superclasses.insert(
            trait_,
            nash_can::kinds::TraitContext {
                parameters: &["bound"],
                supers: b.alloc_slice_fill_iter([Pred::Trait {
                    trait_: ReprTrait::Term.qualified(),
                    args: b.alloc_slice_copy(&[&*bound]),
                }]),
            },
        );
        let context = b.alloc_slice_fill_iter([Pred::Trait {
            trait_,
            args: b.alloc_slice_copy(&[&*typ]),
        }]);
        assert!(check(&b, &kinds, typ, context).is_err());
    }

    #[test]
    fn nominal_records_preserve_their_representation() {
        let b = Bump::new();
        let kinds = KindEnv::default();
        let record = b.alloc(Located::at_zero(Type::Record { fields: &[] }));
        for (name, rejected) in [("Small", false), ("small", true)] {
            let typ = b.alloc(Located::at_zero(Type::Alias {
                reference: QualifiedName {
                    home: module().name,
                    name,
                },
                arguments: &[],
                remaining: &[],
                target: AliasType::Open(record),
            }));
            assert_eq!(check(&b, &kinds, typ, &[]).is_err(), rejected);
        }
    }

    #[test]
    fn transparent_function_alias_checks_its_parameters() {
        let b = Bump::new();
        let kinds = KindEnv::default();
        let unit = b.alloc(Located::at_zero(Type::unit()));
        let function = b.alloc(Located::at_zero(Type::Lambda {
            from: unit,
            to: unit,
        }));
        let spine = b.alloc(Located::at_zero(Type::Lambda {
            from: function,
            to: unit,
        }));
        let alias = b.alloc(Located::at_zero(Type::Alias {
            reference: QualifiedName {
                home: module().name,
                name: "Signature",
            },
            arguments: &[],
            remaining: &[],
            target: AliasType::Open(spine),
        }));
        let ann = b.alloc(Annotation {
            free_vars: &[],
            context: &[],
            typ: alias,
        });
        assert!(check_main_parameters(&b, &module(), Some(ann), &kinds).is_err());
        let mut normal = module();
        normal.kind = ModuleKind::Normal;
        assert!(check_main_parameters(&b, &normal, Some(ann), &kinds).is_ok());
    }
}
