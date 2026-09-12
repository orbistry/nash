use std::collections::{BTreeMap, BTreeSet};

use bumpalo::Bump;
use nash_ast::{
    AliasArgument as CanAliasArgument, AliasType as CanAliasType, Annotation,
    FieldType as CanFieldType, FreeVars, QualifiedName, Type as CanType,
};
use nash_region::{Located, Region};
use nash_source::Type as SourceType;

use crate::Error;
use crate::accumulate;
use crate::environment::{self, Env, Info};
use crate::error::BadArityContext;

/// Canonicalize a source type and wrap it in an Annotation with free type variables.
/// Mirrors Elm's `Type.toAnnotation`.
pub fn to_annotation<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    annotation: &'a nash_source::Annotation<'a>,
) -> Result<&'a Annotation<'a>, Vec<Error<'a>>> {
    let typ = canonicalize_type(bump, env, annotation.typ)?;
    let mut predicates = canonicalize_context(bump, env, annotation.constraints)?.to_vec();
    predicates.extend(repr_predicates(bump, env, annotation.typ)?);
    let context: &'a [nash_ast::Pred<'a>] = bump.alloc_slice_fill_iter(predicates);
    let mut free_var_set: BTreeSet<&'a str> = BTreeSet::new();
    collect_free_vars(&typ.value, &mut free_var_set);
    for predicate in context {
        for argument in predicate.types() {
            let mut variables = BTreeSet::new();
            collect_free_vars(&argument.value, &mut variables);
            if let Some(name) = variables.difference(&free_var_set).next() {
                return Err(vec![Error::ContextVarNotInType {
                    region: argument.region,
                    name,
                }]);
            }
        }
    }
    let free_vars: FreeVars<'a> = bump.alloc_slice_fill_iter(free_var_set);
    Ok(bump.alloc(Annotation {
        context,
        free_vars,
        typ,
    }))
}

pub fn canonicalize_context<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    constraints: &'a [&'a Located<nash_source::Constraint<'a>>],
) -> Result<&'a [nash_ast::Pred<'a>], Vec<Error<'a>>> {
    accumulate::try_all_alloc(
        bump,
        constraints.iter().map(|constraint| {
            let source = &constraint.value;
            let info =
                env.find_trait(bump, source.class.region, source.module, source.class.value)?;
            if source.args.len() != info.parameters.len() {
                return Err(vec![Error::TraitArity {
                    region: constraint.region,
                    name: info.name,
                    expected: info.parameters.len(),
                    actual: source.args.len(),
                }]);
            }
            Ok(nash_ast::Pred::Trait {
                trait_: QualifiedName {
                    home: info.home,
                    name: info.name,
                },
                args: canonicalize_type_arguments(bump, env, source.args)?,
            })
        }),
    )
}

/// Desugar every inline representation annotation, including nested arguments.
/// Callers retain these predicates alongside the resulting canonical type.
pub(crate) fn repr_predicates<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
) -> Result<Vec<nash_ast::Pred<'a>>, Vec<Error<'a>>> {
    collect_repr_predicates(bump, env, typ, false)
}

pub(crate) fn alias_repr_predicates<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
) -> Result<Vec<nash_ast::Pred<'a>>, Vec<Error<'a>>> {
    collect_repr_predicates(bump, env, typ, true)
}

fn collect_repr_predicates<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
    alias_body: bool,
) -> Result<Vec<nash_ast::Pred<'a>>, Vec<Error<'a>>> {
    let mut result = Vec::new();
    let mut pending = vec![(typ, alias_body)];
    while let Some((typ, alias_body)) = pending.pop() {
        match &typ.value {
            SourceType::Repr { typ, repr } => {
                let trait_ = repr_trait(repr.value).qualified();
                let arg = if alias_body {
                    canonicalize_alias_body(bump, env, typ)?
                } else {
                    canonicalize_type(bump, env, typ)?
                };
                result.push(nash_ast::Pred::Trait {
                    trait_,
                    args: bump.alloc_slice_copy(&[arg]),
                });
                pending.push((typ, alias_body));
            }
            SourceType::Lambda { from, to } => pending.extend([(*to, false), (*from, false)]),
            SourceType::VarApp { args, .. }
            | SourceType::Type { args, .. }
            | SourceType::TypeQual { args, .. } => {
                pending.extend(args.iter().rev().map(|typ| (*typ, false)))
            }
            SourceType::Record(fields) => {
                pending.extend(fields.iter().rev().map(|field| (field.typ, false)))
            }
            SourceType::Tuple {
                first,
                second,
                rest,
            } => {
                pending.extend(rest.iter().rev().map(|typ| (*typ, false)));
                pending.extend([(*second, false), (*first, false)]);
            }
            SourceType::Var(_) | SourceType::Unit => {}
        }
    }
    Ok(result)
}

pub(crate) fn repr_trait(repr: nash_source::Repr) -> nash_ast::primitives::ReprTrait {
    use nash_ast::primitives::ReprTrait;
    match repr {
        nash_source::Repr::Big => ReprTrait::Big,
        nash_source::Repr::Const => ReprTrait::Const,
        nash_source::Repr::Term => ReprTrait::Term,
        nash_source::Repr::Storable => ReprTrait::Storable,
    }
}

/// Canonicalize a source type using the environment.
/// Mirrors Elm's `Type.canonicalize`.
pub fn canonicalize_type<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
) -> Result<&'a Located<CanType<'a>>, Vec<Error<'a>>> {
    Ok(bump.alloc(Located::at(
        typ.region,
        canonicalize_type_value(bump, env, typ.region, &typ.value)?,
    )))
}

/// Canonicalize a direct record alias body; nested types use the ordinary rules.
pub fn canonicalize_alias_body<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
) -> Result<&'a Located<CanType<'a>>, Vec<Error<'a>>> {
    match &typ.value {
        SourceType::Repr { typ, .. } => canonicalize_alias_body(bump, env, typ),
        SourceType::Record(fields) => {
            let field_dict = check_fields(fields)?;
            let fields = accumulate::try_all_alloc(
                bump,
                field_dict
                    .into_iter()
                    .map(|(_, (index, field))| canonicalize_field_type(bump, env, index, field)),
            )?;
            Ok(bump.alloc(Located::at(typ.region, CanType::Record { fields })))
        }
        _ => canonicalize_type(bump, env, typ),
    }
}

/// Canonicalize a slice of type arguments.
pub fn canonicalize_type_arguments<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    args: &'a [&'a Located<SourceType<'a>>],
) -> Result<&'a [&'a Located<CanType<'a>>], Vec<Error<'a>>> {
    accumulate::try_all_alloc_ref(
        bump,
        args.iter()
            .copied()
            .map(|arg| canonicalize_type(bump, env, arg)),
    )
}

fn canonicalize_type_value<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    region: Region,
    typ: &SourceType<'a>,
) -> Result<CanType<'a>, Vec<Error<'a>>> {
    Ok(match typ {
        SourceType::Repr { typ, .. } => canonicalize_type_value(bump, env, region, &typ.value)?,
        SourceType::Lambda { from, to } => {
            let (from, to) = accumulate::accumulate2(
                canonicalize_type(bump, env, from),
                canonicalize_type(bump, env, to),
            )?;
            CanType::Lambda { from, to }
        }
        SourceType::Var(name) => CanType::Var(name),
        SourceType::VarApp {
            region: name_region,
            name,
            args,
        } => CanType::App {
            head: bump.alloc(Located::at(*name_region, CanType::Var(name))),
            args: canonicalize_type_arguments(bump, env, args)?,
        },
        SourceType::Type {
            region: name_region,
            name,
            args,
        } => {
            let info = find_type(bump, env, *name_region, name)?;
            canonicalize_env_type(bump, env, region, name, args, info)?
        }
        SourceType::TypeQual {
            region: name_region,
            module: type_module,
            name,
            args,
        } => {
            let info = find_type_qual(bump, env, *name_region, type_module, name)?;
            canonicalize_env_type(bump, env, region, name, args, info)?
        }
        SourceType::Record(_) => return Err(vec![Error::RecordTypeOutsideAlias { region }]),
        SourceType::Unit => CanType::unit(),
        SourceType::Tuple {
            first,
            second,
            rest,
        } => {
            let (first, second, rest) = accumulate::accumulate3(
                canonicalize_type(bump, env, first),
                canonicalize_type(bump, env, second),
                canonicalize_type_arguments(bump, env, rest),
            )?;
            CanType::Tuple {
                first,
                second,
                rest,
            }
        }
    })
}

/// Mirrors Elm's `Dups.checkFields`: one `DuplicateField` per duplicated
/// name, in name order, with the first two occurrences. On success the
/// fields come back keyed (and therefore ordered) by name, each carrying
/// its source-position index.
fn check_fields<'a, 'f>(
    fields: &'f [&'a nash_source::FieldType<'a>],
) -> Result<BTreeMap<&'a str, (u16, &'f nash_source::FieldType<'a>)>, Vec<Error<'a>>> {
    let mut occurrences: BTreeMap<&'a str, Vec<(Region, u16, &nash_source::FieldType<'a>)>> =
        BTreeMap::new();
    for (index, field) in fields.iter().enumerate() {
        let index: u16 = index.try_into().expect("record field index exceeds u16");
        occurrences
            .entry(field.field.value)
            .or_default()
            .push((field.field.region, index, field));
    }

    let mut result = BTreeMap::new();
    let mut errors = Vec::new();
    for (name, mut entries) in occurrences {
        if entries.len() > 1 {
            errors.push(Error::DuplicateField {
                name,
                first: entries[0].0,
                second: entries[1].0,
            });
        } else {
            let (_, index, field) = entries.remove(0);
            result.insert(name, (index, field));
        }
    }

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

/// Mirrors Elm's `Env.findType`. Lookup errors point at the type name
/// itself, not the whole application.
fn find_type<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    name_region: Region,
    name: &'a str,
) -> Result<environment::Type<'a>, Vec<Error<'a>>> {
    match env.types.get(name) {
        Some(Info::Specific(_, typ)) => Ok(*typ),
        Some(Info::Ambiguous(first, others)) => Err(vec![Error::AmbiguousType {
            region: name_region,
            prefix: None,
            name,
            first_module: *first,
            other_modules: bump.alloc_slice_fill_iter(others.iter().copied()),
        }]),
        None => Err(vec![Error::NotFoundType {
            region: name_region,
            prefix: None,
            name,
            suggestions: env.possible_type_names(bump),
        }]),
    }
}

/// Mirrors Elm's `Env.findTypeQual`.
fn find_type_qual<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    name_region: Region,
    prefix: &'a str,
    name: &'a str,
) -> Result<environment::Type<'a>, Vec<Error<'a>>> {
    let info = env
        .q_types
        .get(prefix)
        .and_then(|m| m.get(name))
        .ok_or_else(|| {
            vec![Error::NotFoundType {
                region: name_region,
                prefix: Some(prefix),
                name,
                suggestions: env.possible_type_names(bump),
            }]
        })?;

    match info {
        Info::Specific(_, typ) => Ok(*typ),
        Info::Ambiguous(first, others) => Err(vec![Error::AmbiguousType {
            region: name_region,
            prefix: Some(prefix),
            name,
            first_module: *first,
            other_modules: bump.alloc_slice_fill_iter(others.iter().copied()),
        }]),
    }
}

/// Mirrors Elm's `Type.canonicalizeType`: arguments are canonicalized
/// first, and the arity check only runs once they all succeed.
fn canonicalize_env_type<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    region: Region,
    name: &'a str,
    args: &'a [&'a Located<SourceType<'a>>],
    typ: environment::Type<'a>,
) -> Result<CanType<'a>, Vec<Error<'a>>> {
    let can_args = canonicalize_type_arguments(bump, env, args)?;
    match typ {
        environment::Type::Alias {
            arity,
            home,
            parameters,
            typ: alias_typ,
        } => {
            check_max_arity(region, name, arity, args.len())?;
            let arguments = bump.alloc_slice_fill_iter(
                parameters
                    .iter()
                    .copied()
                    .zip(can_args.iter().copied())
                    .map(|(parameter, typ)| CanAliasArgument {
                        name: parameter,
                        typ,
                    }),
            );
            Ok(CanType::Alias {
                reference: QualifiedName { home, name },
                arguments,
                target: CanAliasType::Open(alias_typ),
                remaining: &parameters[args.len()..],
            })
        }
        environment::Type::Union { arity, home } => {
            check_max_arity(region, name, arity, args.len())?;
            Ok(CanType::Named {
                reference: QualifiedName { home, name },
                args: can_args,
            })
        }
    }
}

fn check_max_arity<'a>(
    region: Region,
    name: &'a str,
    expected: usize,
    actual: usize,
) -> Result<(), Vec<Error<'a>>> {
    // Partial constructors are valid arguments to higher-kinded parameters.
    // The kind checker rejects them where a value type is required.
    if actual <= expected {
        Ok(())
    } else {
        Err(vec![Error::BadArity {
            region,
            context: BadArityContext::TypeArity,
            name,
            expected,
            actual,
        }])
    }
}

fn canonicalize_field_type<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    index: u16,
    field: &nash_source::FieldType<'a>,
) -> Result<CanFieldType<'a>, Vec<Error<'a>>> {
    Ok(CanFieldType {
        index,
        field: field.field.value,
        typ: canonicalize_type(bump, env, field.typ)?,
    })
}

pub fn collect_free_vars<'a>(typ: &CanType<'a>, vars: &mut BTreeSet<&'a str>) {
    match typ {
        CanType::App { head, args } => {
            collect_free_vars(&head.value, vars);
            for arg in *args {
                collect_free_vars(&arg.value, vars);
            }
        }
        CanType::Var(name) => {
            vars.insert(name);
        }
        CanType::Lambda { from, to } => {
            collect_free_vars(&from.value, vars);
            collect_free_vars(&to.value, vars);
        }
        CanType::Named { args, .. } => {
            for arg in *args {
                collect_free_vars(&arg.value, vars);
            }
        }
        CanType::Record { fields } => {
            for f in *fields {
                collect_free_vars(&f.typ.value, vars);
            }
        }
        CanType::Alias { arguments, .. } => {
            for arg in *arguments {
                collect_free_vars(&arg.typ.value, vars);
            }
        }

        CanType::Tuple {
            first,
            second,
            rest,
        } => {
            collect_free_vars(&first.value, vars);
            collect_free_vars(&second.value, vars);
            for r in *rest {
                collect_free_vars(&r.value, vars);
            }
        }
    }
}

/// Mirrors Elm's `Type.dealias`: fill a `Holey` alias body by substituting
/// the alias arguments for its type variables. `Filled` bodies are already
/// substituted.
pub fn dealias<'a>(
    bump: &'a Bump,
    arguments: &'a [CanAliasArgument<'a>],
    target: &CanAliasType<'a>,
) -> &'a Located<CanType<'a>> {
    match target {
        CanAliasType::Filled { typ, .. } => typ,
        CanAliasType::Open(typ) => {
            let table: BTreeMap<&'a str, &'a Located<CanType<'a>>> =
                arguments.iter().map(|arg| (arg.name, arg.typ)).collect();
            substitute_type(bump, &table, typ)
        }
    }
}

/// Apply a known canonical head without opening an alias's bound body.
pub(crate) fn apply_type<'a>(
    bump: &'a Bump,
    region: Region,
    head: &'a Located<CanType<'a>>,
    args: &'a [&'a Located<CanType<'a>>],
) -> &'a Located<CanType<'a>> {
    if args.is_empty() {
        return head;
    }
    let typ = match &head.value {
        CanType::App {
            head,
            args: existing,
        } => {
            let combined = bump
                .alloc_slice_fill_iter(existing.iter().chain(args).copied().collect::<Vec<_>>());
            return apply_type(bump, region, head, combined);
        }
        CanType::Named {
            reference,
            args: existing,
        } => CanType::Named {
            reference: *reference,
            args: bump
                .alloc_slice_fill_iter(existing.iter().chain(args).copied().collect::<Vec<_>>()),
        },
        CanType::Alias {
            reference,
            arguments,
            remaining,
            target,
        } => {
            let consumed = remaining.len().min(args.len());
            let applied = bump.alloc(Located::at(
                region,
                CanType::Alias {
                    reference: *reference,
                    arguments: bump.alloc_slice_fill_iter(
                        arguments
                            .iter()
                            .map(|a| CanAliasArgument {
                                name: a.name,
                                typ: a.typ,
                            })
                            .chain(
                                remaining
                                    .iter()
                                    .zip(args)
                                    .map(|(name, typ)| CanAliasArgument { name, typ }),
                            )
                            .collect::<Vec<_>>(),
                    ),
                    remaining: &remaining[consumed..],
                    target: match target {
                        CanAliasType::Open(t) => CanAliasType::Open(t),
                        CanAliasType::Filled { body, typ } => CanAliasType::Filled { body, typ },
                    },
                },
            ));
            if consumed == args.len() {
                return applied;
            }
            CanType::App {
                head: applied,
                args: &args[consumed..],
            }
        }
        _ => CanType::App { head, args },
    };
    bump.alloc(Located::at(region, typ))
}

pub fn substitute_type<'a>(
    bump: &'a Bump,
    table: &BTreeMap<&'a str, &'a Located<CanType<'a>>>,
    typ: &'a Located<CanType<'a>>,
) -> &'a Located<CanType<'a>> {
    let substituted = match &typ.value {
        CanType::Var(name) => return table.get(name).copied().unwrap_or(typ),
        CanType::App { head, args } => {
            let head = substitute_type(bump, table, head);
            let args = bump
                .alloc_slice_fill_iter(args.iter().map(|arg| substitute_type(bump, table, arg)));
            return apply_type(bump, typ.region, head, args);
        }

        CanType::Lambda { from, to } => CanType::Lambda {
            from: substitute_type(bump, table, from),
            to: substitute_type(bump, table, to),
        },
        CanType::Named { reference, args } => CanType::Named {
            reference: *reference,
            args: bump
                .alloc_slice_fill_iter(args.iter().map(|arg| substitute_type(bump, table, arg))),
        },
        CanType::Record { fields } => CanType::Record {
            fields: bump.alloc_slice_fill_iter(fields.iter().map(|f| CanFieldType {
                index: f.index,
                field: f.field,
                typ: substitute_type(bump, table, f.typ),
            })),
        },
        // Open bodies bind alias parameters; filled bodies contain caller variables.
        CanType::Alias {
            reference,
            arguments,
            remaining,
            target,
        } => CanType::Alias {
            reference: *reference,
            remaining,
            arguments: bump.alloc_slice_fill_iter(arguments.iter().map(|arg| CanAliasArgument {
                name: arg.name,
                typ: substitute_type(bump, table, arg.typ),
            })),
            target: match target {
                CanAliasType::Open(t) => CanAliasType::Open(t),
                CanAliasType::Filled { body, typ } => CanAliasType::Filled {
                    body,
                    typ: substitute_type(bump, table, typ),
                },
            },
        },
        CanType::Tuple {
            first,
            second,
            rest,
        } => CanType::Tuple {
            first: substitute_type(bump, table, first),
            second: substitute_type(bump, table, second),
            rest: bump.alloc_slice_fill_iter(rest.iter().map(|r| substitute_type(bump, table, r))),
        },
    };
    bump.alloc(Located::at(typ.region, substituted))
}

/// Mirrors Elm's `Type.iteratedDealias`.
pub fn iterated_dealias<'a>(
    bump: &'a Bump,
    typ: &'a Located<CanType<'a>>,
) -> &'a Located<CanType<'a>> {
    match &typ.value {
        CanType::Alias {
            arguments,
            target,
            remaining: [],
            ..
        } => iterated_dealias(bump, dealias(bump, arguments, target)),
        _ => typ,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use nash_ast::ModuleName;
    use nash_can::types::{canonicalize_type, to_annotation};

    use nash_can::environment::{Env, Info, Type as EnvType};

    #[test]
    fn partial_alias_keeps_formal_parameters_bound() {
        let bump = Bump::new();
        let interface = {
            let source = &bump;
            let var = |name| &*source.alloc(Located::at_zero(CanType::Var(name)));
            let body = source.alloc(Located::at_zero(CanType::Lambda {
                from: var("left"),
                to: var("right"),
            }));
            let partial = source.alloc(Located::at_zero(CanType::Alias {
                reference: QualifiedName {
                    home: ModuleName {
                        package: None,
                        name: source.alloc_str("Original"),
                    },
                    name: source.alloc_str("Arrow"),
                },
                arguments: source.alloc_slice_fill_iter([CanAliasArgument {
                    name: "left",
                    typ: var("right"),
                }]),
                remaining: source.alloc_slice_fill_iter([&*source.alloc_str("right")]),
                target: CanAliasType::Open(body),
            }));
            let mut interface = nash_can::kinds::builtin_interface(source);
            interface.values = source.alloc_slice_fill_iter([nash_can::InterfaceValue {
                name: "partial",
                annotation: source.alloc(nash_ast::Annotation {
                    free_vars: &["right"],
                    context: &[],
                    typ: partial,
                }),
            }]);
            interface
        };
        let partial = interface.values[0].annotation.typ;
        assert!(std::ptr::eq(iterated_dealias(&bump, partial), partial));
        let unit = bump.alloc(Located::at_zero(CanType::unit()));
        let applied = apply_type(
            &bump,
            Region::zero(),
            partial,
            bump.alloc_slice_copy(&[&*unit]),
        );
        let expanded = iterated_dealias(&bump, applied);
        let CanType::Lambda { from, to } = &expanded.value else {
            panic!("alias body")
        };
        assert!(matches!(from.value, CanType::Var("right")));
        assert!((to.value == CanType::unit()));
        let excess = apply_type(
            &bump,
            Region::zero(),
            partial,
            bump.alloc_slice_copy(&[&*unit, &*unit]),
        );
        assert!(matches!(&excess.value, CanType::App { args, .. } if args.len() == 1));
        let CanType::Alias {
            reference,
            arguments,
            ..
        } = &applied.value
        else {
            panic!("saturated alias")
        };
        let filled = bump.alloc(Located::at_zero(CanType::Alias {
            reference: *reference,
            arguments,
            remaining: &[],
            target: CanAliasType::Filled {
                body: match &applied.value {
                    CanType::Alias {
                        target: CanAliasType::Open(body),
                        ..
                    } => body,
                    _ => panic!("closed alias body"),
                },
                typ: expanded,
            },
        }));
        let replaced = substitute_type(&bump, &BTreeMap::from([("right", &*unit)]), filled);
        let (
            CanType::Alias {
                target: CanAliasType::Filled { body: original, .. },
                ..
            },
            CanType::Alias {
                target: CanAliasType::Filled { body: retained, .. },
                ..
            },
        ) = (&filled.value, &replaced.value)
        else {
            panic!("filled alias template")
        };
        assert!(std::ptr::eq(*original, *retained));
        assert!(
            matches!(&retained.value, CanType::Lambda { to, .. } if matches!(to.value, CanType::Var("right")))
        );
        let replaced = iterated_dealias(&bump, replaced);
        assert!(
            matches!(&replaced.value, CanType::Lambda { from, to } if (from.value == CanType::unit()) && (to.value == CanType::unit()))
        );
        insta::with_settings!({omit_expression => true}, {
            insta::assert_debug_snapshot!((partial, applied, expanded));
        });
    }

    fn empty_env<'a>(bump: &'a Bump) -> Env<'a> {
        let _ = bump;
        Env {
            kinds: nash_can::kinds::KindEnv::from_interfaces(None),
            traits: Default::default(),
            q_traits: Default::default(),
            home: ModuleName {
                package: None,
                name: "Main",
            },
            vars: Default::default(),
            types: Default::default(),
            ctors: Default::default(),
            binops: Default::default(),
            q_vars: Default::default(),
            q_types: Default::default(),
            q_ctors: Default::default(),
        }
    }

    fn env_with_int<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Basics",
        };
        let mut env = empty_env(bump);
        env.types.insert(
            "Int",
            Info::Specific(home, EnvType::Union { arity: 0, home }),
        );
        env
    }

    fn env_with_list_and_int<'a>(bump: &'a Bump) -> Env<'a> {
        let basics = ModuleName {
            package: None,
            name: "Basics",
        };
        let list_mod = nash_ast::primitives::builtin_home();
        let mut env = empty_env(bump);
        env.types.insert(
            "Int",
            Info::Specific(
                basics,
                EnvType::Union {
                    arity: 0,
                    home: basics,
                },
            ),
        );
        env.types.insert(
            "List",
            Info::Specific(
                list_mod,
                EnvType::Union {
                    arity: 1,
                    home: list_mod,
                },
            ),
        );
        env
    }

    fn env_with_maybe_alias<'a>(bump: &'a Bump) -> Env<'a> {
        let home = ModuleName {
            package: None,
            name: "Maybe",
        };
        let mut env = empty_env(bump);
        let alias_type = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        env.types.insert(
            "Maybe",
            Info::Specific(
                home,
                EnvType::Alias {
                    arity: 1,
                    home,
                    parameters: bump.alloc_slice_fill_iter(["a"]),
                    typ: alias_type,
                },
            ),
        );
        env
    }

    fn parse_type<'a>(bump: &'a Bump, input: &str) -> &'a Located<SourceType<'a>> {
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(bump, src);
        let (typ, _end) = parser.type_expr().expect("expected successful parse");
        typ
    }

    macro_rules! assert_type_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let typ = parse_type(&bump, $input);
            let result = canonicalize_type(&bump, &env, typ);
            insta::with_settings!({
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result.unwrap());
            });
        }};
    }

    macro_rules! assert_type_error_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let typ = parse_type(&bump, $input);
            let result = canonicalize_type(&bump, &env, typ);
            insta::with_settings!({info => &"diagnostic",
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_snapshot!(crate::snapshot_support::errors($input, &result.unwrap_err()));
            });
        }};
    }

    macro_rules! assert_annotation_snapshot {
        ($input:expr, $env_fn:ident) => {{
            let bump = Bump::new();
            let env = $env_fn(&bump);
            let typ = parse_type(&bump, $input);
            let annotation = bump.alloc(nash_source::Annotation { constraints: &[], typ });
            let result = to_annotation(&bump, &env, annotation);
            insta::with_settings!({
                description => $input,
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(result.unwrap());
            });
        }};
    }

    #[test]
    fn annotation_simple_var() {
        assert_annotation_snapshot!("'a", empty_env);
    }

    #[test]
    fn annotation_function() {
        assert_annotation_snapshot!("'a -> 'b -> 'a", empty_env);
    }

    #[test]
    fn annotation_no_free_vars() {
        assert_annotation_snapshot!("Int", env_with_int);
    }

    #[test]
    fn annotation_mixed() {
        assert_annotation_snapshot!("'a -> List 'a", env_with_list_and_int);
    }

    #[test]
    fn type_tuple_three() {
        assert_type_snapshot!("( 'a, 'b, 'c )", empty_env);
    }

    #[test]
    fn type_tuple_four() {
        assert_type_snapshot!("( 'a, 'b, 'c, 'd )", empty_env);
    }

    #[test]
    fn type_alias_expansion() {
        assert_type_snapshot!("Maybe 'a", env_with_maybe_alias);
    }

    #[test]
    fn type_union_reference() {
        assert_type_snapshot!("List 'a", env_with_list_and_int);
    }

    #[test]
    fn type_var_application() {
        assert_type_snapshot!("'f 'a", empty_env);
    }

    #[test]
    fn anonymous_record_type_errors() {
        assert_type_error_snapshot!("{ z : Int, a : Int }", env_with_int);
    }

    #[test]
    fn bad_args_reported_before_arity() {
        assert_type_error_snapshot!("Maybe Bogus Other", env_with_maybe_alias);
    }

    #[test]
    fn iterated_dealias_substitutes_arguments() {
        let bump = Bump::new();
        let home = ModuleName {
            package: None,
            name: "Main",
        };
        // type alias Transform a = a -> a, applied to Int
        let var_a = bump.alloc(Located::at(Region::zero(), CanType::Var("a")));
        let body = bump.alloc(Located::at(
            Region::zero(),
            CanType::Lambda {
                from: var_a,
                to: var_a,
            },
        ));
        let int = bump.alloc(Located::at(
            Region::zero(),
            CanType::Named {
                reference: QualifiedName { home, name: "Int" },
                args: &[],
            },
        ));
        let arguments = bump.alloc_slice_fill_iter([CanAliasArgument {
            name: "a",
            typ: &*int,
        }]);
        let aliased = bump.alloc(Located::at(
            Region::zero(),
            CanType::Alias {
                reference: QualifiedName {
                    home,
                    name: "Transform",
                },
                arguments,
                target: CanAliasType::Open(body),
                remaining: &[],
            },
        ));
        let dealiased = iterated_dealias(&bump, aliased);
        match &dealiased.value {
            CanType::Lambda { from, to } => {
                assert!(matches!(
                    &from.value,
                    CanType::Named { reference, .. } if reference.name == "Int"
                ));
                assert!(matches!(
                    &to.value,
                    CanType::Named { reference, .. } if reference.name == "Int"
                ));
            }
            other => panic!("expected substituted lambda, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use crate::snapshot_support::SnapshotInputs;
    use nash_ast::{Kind, ModuleName};
    use nash_can::environment::TraitInfo;
    use nash_can::environment::{self, Env, Info};
    use nash_can::types::to_annotation;

    fn source_annotation<'a>(
        bump: &'a Bump,
        annotation: &str,
    ) -> (&'a str, &'a nash_source::Annotation<'a>) {
        let source = bump.alloc_str(&format!(
            "module Main exposing (..)\n\nf : {annotation}\nf x = x\n"
        ));
        let mut parser = nash_parse::Parser::new(bump, source);
        (
            source,
            parser.module().unwrap().values[0].value.annotation.unwrap(),
        )
    }

    fn trait_env(bump: &Bump) -> Env<'_> {
        let mut env = environment::foreign::create_initial_env(
            bump,
            ModuleName {
                package: None,
                name: "Main",
            },
            None,
            &[],
        )
        .unwrap();
        let info = bump.alloc(TraitInfo {
            home: ModuleName {
                package: None,
                name: "Equality",
            },
            name: "Eq",
            parameters: &["a"],
            kinds: &[&Kind::Type],
            supers: &[],
            methods: &[],
        });
        env.traits.insert("Eq", Info::Specific(info.home, info));
        env.q_traits
            .entry("Equality")
            .or_default()
            .insert("Eq", Info::Specific(info.home, info));
        env
    }

    #[test]
    fn qualified_annotation_context() {
        let snapshot_inputs = SnapshotInputs::default();
        let bump = Bump::new();
        let env = trait_env(&bump);
        let (source, annotation) =
            source_annotation(&bump, snapshot_inputs.record("Equality.Eq 'a => 'a -> 'a"));
        let result = to_annotation(&bump, &env, annotation).unwrap();
        insta::with_settings!({description => source, omit_expression => true}, {
            insta::assert_debug_snapshot!(result);
        });
    }

    #[test]
    fn context_variable_absent_from_type() {
        let snapshot_inputs = SnapshotInputs::default();
        let bump = Bump::new();
        let env = trait_env(&bump);
        let (source, annotation) =
            source_annotation(&bump, snapshot_inputs.record("Eq 'b => 'a -> 'a"));
        insta::with_settings!({info => &"diagnostic", description => source, omit_expression => true}, {
            insta::assert_snapshot!(crate::snapshot_support::errors(source, &to_annotation(&bump, &env, annotation).unwrap_err()));
        });
    }

    #[test]
    fn context_trait_arity() {
        let snapshot_inputs = SnapshotInputs::default();
        let bump = Bump::new();
        let env = trait_env(&bump);
        let (source, annotation) =
            source_annotation(&bump, snapshot_inputs.record("Eq 'a 'b => 'a -> 'b"));
        insta::with_settings!({info => &"diagnostic", description => source, omit_expression => true}, {
            insta::assert_snapshot!(crate::snapshot_support::errors(source, &to_annotation(&bump, &env, annotation).unwrap_err()));
        });
    }
}
