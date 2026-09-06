use crate::environment::{Env, TraitInfo, dups};
use crate::error::BadHead;
use crate::{Error, kinds, module, types, warning::Warning};
use bumpalo::Bump;
use nash_ast::{Annotation, Head, Impl, ImplKey, Pred, QualifiedName, Type};
use nash_region::{Located, Region};
use nash_source::Type as SourceType;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn info<'a>(
    bump: &'a Bump,
    home: nash_ast::ModuleName<'a>,
    impl_: &Located<Impl<'a>>,
) -> crate::environment::ImplInfo<'a> {
    crate::environment::ImplInfo {
        variables: impl_.value.variables,
        kinds: impl_.value.kinds,
        home,
        region: impl_.region,
        trait_: impl_.value.trait_,
        context: impl_.value.context,
        heads: impl_.value.heads,
        methods: bump.alloc_slice_fill_iter(impl_.value.methods.iter().map(|m| match m {
            nash_ast::Def::Def { name, .. } | nash_ast::Def::TypedDef { name, .. } => name.value,
        })),
    }
}

pub(crate) fn tables<'a>(
    bump: &'a Bump,
    interfaces: Option<&BTreeMap<&'a str, crate::Interface<'a>>>,
    module: &nash_ast::Module<'a>,
    kind_env: &kinds::KindEnv<'a>,
) -> Result<crate::environment::Tables<'a>, Vec<Error<'a>>> {
    use crate::environment::{MethodInfo, Tables};
    let mut tables = Tables {
        kinds: kind_env.clone(),
        ..Tables::default()
    };
    // Resolution sees all build interfaces, independently of source import visibility.
    for interface in interfaces.into_iter().flat_map(|i| i.values()) {
        for trait_ in interface.traits {
            let name = QualifiedName {
                home: interface.home,
                name: trait_.name,
            };
            tables.traits.insert(
                name,
                bump.alloc(TraitInfo {
                    home: name.home,
                    name: name.name,
                    parameters: trait_.parameters,
                    kind: trait_.kind,
                    supers: trait_.supers,
                    methods: bump.alloc_slice_fill_iter(trait_.methods.iter().map(|m| {
                        MethodInfo {
                            name: m.name,
                            annotation: m.annotation,
                            has_default: m.has_default,
                        }
                    })),
                }),
            );
        }
        for impl_ in interface.impls {
            insert_impl(bump, &mut tables.impls, impl_)?;
        }
    }
    for trait_ in module.traits {
        let t = &trait_.value;
        let name = QualifiedName {
            home: module.name,
            name: t.name.value,
        };
        tables.traits.insert(
            name,
            bump.alloc(TraitInfo {
                home: name.home,
                name: name.name,
                parameters: t.parameters,
                kind: t.kind,
                supers: t.supers,
                methods: bump.alloc_slice_fill_iter(t.methods.iter().map(|m| MethodInfo {
                    name: m.name.value,
                    annotation: m.annotation,
                    has_default: m.default.is_some(),
                })),
            }),
        );
    }
    for impl_ in module.impls {
        insert_impl(
            bump,
            &mut tables.impls,
            bump.alloc(info(bump, module.name, impl_)),
        )?;
    }
    for impl_ in tables.impls.values().filter(|i| i.home == module.name) {
        crate::entailment::check(bump, &tables, kind_env, impl_)?;
    }
    Ok(tables)
}

fn insert_impl<'a>(
    bump: &'a Bump,
    table: &mut crate::environment::ImplTable<'a>,
    impl_: &'a crate::environment::ImplInfo<'a>,
) -> Result<(), Vec<Error<'a>>> {
    let key = ImplKey {
        trait_: impl_.trait_,
        heads: bump.alloc_slice_fill_iter(impl_.heads.iter().map(|h| h.value)),
        kinds: impl_.kinds,
    };
    let mut remaining = 16_384;
    for (candidate, first) in table
        .iter()
        .filter(|(candidate, _)| candidate.trait_ == key.trait_)
    {
        let overlaps = nash_ast::head::overlaps(candidate.heads, key.heads, &mut remaining)
            .map_err(|_| {
                vec![Error::ImplPatternLimit {
                    region: impl_.region,
                }]
            })?;
        if overlaps {
            return Err(vec![Error::OverlappingImpls {
                key: bump.alloc(key),
                first: first.region,
                second: impl_.region,
                first_home: first.home,
                second_home: impl_.home,
            }]);
        }
    }
    table.insert(key, impl_);
    Ok(())
}

pub(crate) fn canonicalize<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    kind_env: &kinds::KindEnv<'a>,
    sources: &'a [&'a Located<nash_source::Impl<'a>>],
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a [&'a Located<Impl<'a>>], Vec<Error<'a>>> {
    let mut result = Vec::new();
    for source in sources {
        let src = &source.value;
        if let Some(attribute) = src.attributes.first() {
            return Err(vec![Error::Unsupported {
                feature: "attributes",
                region: attribute.name.region,
            }]);
        }
        let predicate = &src.head.value;
        let info = env.find_trait(
            bump,
            predicate.class.region,
            predicate.module,
            predicate.class.value,
        )?;
        if predicate.args.len() != info.parameters.len() {
            return Err(vec![Error::TraitArity {
                region: src.head.region,
                name: info.name,
                expected: info.parameters.len(),
                actual: predicate.args.len(),
            }]);
        }
        let trait_ = QualifiedName {
            home: info.home,
            name: info.name,
        };
        let mut variables = BTreeMap::new();
        let mut order = Vec::new();
        let mut heads = Vec::new();
        let mut head_types = Vec::new();
        for arg in predicate.args {
            let (head, typ) = canonicalize_head(bump, env, arg, &mut variables, &mut order)?;
            heads.push(head);
            head_types.push(typ);
        }
        if trait_.home != env.home
            && !heads.iter().any(|h| match &h.value {
                Head::Named { reference, .. } => reference.home == env.home,
                Head::Unit | Head::Tuple(_) => env.home.package == Some(nash_ast::primitives::CORE),
                Head::Var(_) | Head::Function(..) => false,
            })
        {
            return Err(vec![Error::OrphanImpl {
                region: source.region,
                trait_,
                heads: bump.alloc_slice_fill_iter(
                    heads
                        .iter()
                        .map(|h| h.value.con().expect("checked outer head")),
                ),
            }]);
        }
        let context = types::canonicalize_context(bump, env, src.context)?;
        for predicate in context {
            for argument in predicate.args {
                let mut free = BTreeSet::new();
                types::collect_free_vars(&argument.value, &mut free);
                if let Some(name) = free.iter().find(|name| !variables.contains_key(**name)) {
                    return Err(vec![Error::ImplContextVarNotInHead {
                        region: argument.region,
                        name,
                    }]);
                }
            }
        }
        let mut checked_kinds = kinds::check_impl_heads(
            bump,
            kind_env,
            env.home,
            info,
            &head_types,
            &variables,
            context,
        )?;
        let head_kinds = checked_kinds.generalize(&order);
        let names = dups::detect(
            src.methods.iter().map(|m| {
                let nash_source::Def::Define { name, .. } = &m.value else {
                    unreachable!("named impl method")
                };
                (name.value, name.region)
            }),
            |name, first, second| Error::DuplicateMethod {
                name,
                first,
                second,
            },
        )?;
        let mut methods = Vec::new();
        for definition in src.methods {
            let nash_source::Def::Define { name, .. } = &definition.value else {
                unreachable!("named impl method")
            };
            let method = info
                .methods
                .iter()
                .find(|m| m.name == name.value)
                .ok_or_else(|| {
                    vec![Error::UnknownMethod {
                        region: name.region,
                        trait_: info.name,
                        name: name.value,
                    }]
                })?;
            let scheme = instantiate_method(
                bump,
                kind_env,
                info,
                method,
                &head_types,
                context,
                &variables,
            )?;
            methods.push(module::canonicalize_typed_value(
                bump, env, definition, scheme, warnings,
            )?);
        }
        for method in info.methods {
            if !method.has_default && !names.contains_key(method.name) {
                return Err(vec![Error::MissingMethod {
                    region: source.region,
                    trait_: info.name,
                    name: method.name,
                }]);
            }
        }
        result.push(&*bump.alloc(Located::at(
            source.region,
            Impl {
                variables: bump.alloc_slice_fill_iter(order),
                kinds: head_kinds,
                trait_,
                context,
                heads: bump.alloc_slice_fill_iter(heads),
                methods: bump.alloc_slice_fill_iter(methods),
            },
        )));
    }
    Ok(bump.alloc_slice_fill_iter(result))
}

type CanonicalHead<'a> = (Located<Head<'a>>, &'a Located<Type<'a>>);
fn canonicalize_head<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    typ: &'a Located<SourceType<'a>>,
    variables: &mut BTreeMap<&'a str, Region>,
    order: &mut Vec<&'a str>,
) -> Result<CanonicalHead<'a>, Vec<Error<'a>>> {
    let reason = match typ.value {
        SourceType::Var(_) => Some(BadHead::BareVariable),
        SourceType::Lambda { .. } => Some(BadHead::Function),
        SourceType::Record(_) => Some(BadHead::Record),
        SourceType::VarApp { .. } => Some(BadHead::VariableApplication),
        _ => None,
    };
    if let Some(reason) = reason {
        return Err(vec![Error::BadInstanceHead {
            region: typ.region,
            reason,
        }]);
    }
    let canonical = types::canonicalize_type(bump, env, typ)?;
    let head = canonicalize_pattern(bump, canonical, variables, order)?;
    Ok((Located::at(typ.region, head), canonical))
}

pub(crate) fn canonicalize_pattern<'a>(
    bump: &'a Bump,
    typ: &'a Located<Type<'a>>,
    variables: &mut BTreeMap<&'a str, Region>,
    order: &mut Vec<&'a str>,
) -> Result<Head<'a>, Vec<Error<'a>>> {
    let head = match &typ.value {
        Type::Var(name) => {
            variables.entry(name).or_insert(typ.region);
            let index = match order.iter().position(|existing| existing == name) {
                Some(index) => index,
                None => {
                    order.push(name);
                    order.len() - 1
                }
            };
            Head::Var(index.try_into().expect("impl variable count exceeds u16"))
        }
        Type::Named { reference, args } => {
            if *reference
                == (QualifiedName {
                    home: nash_ast::primitives::builtin_home(),
                    name: "unit",
                })
                && args.is_empty()
            {
                Head::Unit
            } else {
                let args = args
                    .iter()
                    .map(|arg| canonicalize_pattern(bump, arg, variables, order))
                    .collect::<Result<Vec<_>, _>>()?;
                Head::Named {
                    reference: *reference,
                    args: bump.alloc_slice_fill_iter(args),
                }
            }
        }
        Type::Alias {
            reference,
            arguments,
            ..
        } => {
            let args = arguments
                .iter()
                .map(|arg| canonicalize_pattern(bump, arg.typ, variables, order))
                .collect::<Result<Vec<_>, _>>()?;
            Head::Named {
                reference: *reference,
                args: bump.alloc_slice_fill_iter(args),
            }
        }
        Type::Unit => Head::Unit,
        Type::Tuple {
            first,
            second,
            rest,
        } => {
            let args = [*first, *second]
                .into_iter()
                .chain(rest.iter().copied())
                .map(|arg| canonicalize_pattern(bump, arg, variables, order))
                .collect::<Result<Vec<_>, _>>()?;
            Head::Tuple(bump.alloc_slice_fill_iter(args))
        }
        Type::Lambda { from, to } => Head::Function(
            bump.alloc(canonicalize_pattern(bump, from, variables, order)?),
            bump.alloc(canonicalize_pattern(bump, to, variables, order)?),
        ),
        Type::App { .. } | Type::Record { .. } => {
            return Err(vec![Error::BadInstanceHead {
                region: typ.region,
                reason: if matches!(typ.value, Type::App { .. }) {
                    BadHead::VariableApplication
                } else {
                    BadHead::Record
                },
            }]);
        }
    };
    Ok(head)
}

fn instantiate_method<'a>(
    bump: &'a Bump,
    kind_env: &kinds::KindEnv<'a>,
    info: &TraitInfo<'a>,
    method: &crate::environment::MethodInfo<'a>,
    heads: &[&'a Located<Type<'a>>],
    context: &'a [Pred<'a>],
    head_vars: &BTreeMap<&'a str, Region>,
) -> Result<&'a Annotation<'a>, Vec<Error<'a>>> {
    let annotation = method.annotation;
    let mut substitution: BTreeMap<_, _> = info
        .parameters
        .iter()
        .copied()
        .zip(heads.iter().copied())
        .collect();
    let mut used: BTreeSet<_> = annotation
        .free_vars
        .iter()
        .copied()
        .chain(head_vars.keys().copied())
        .collect();
    for var in annotation.free_vars {
        if !info.parameters.contains(var) && head_vars.contains_key(var) {
            let mut index = 0;
            let fresh = loop {
                let candidate = format!("$impl{index}");
                if !used.contains(candidate.as_str()) {
                    break &*bump.alloc_str(&candidate);
                }
                index += 1;
            };
            used.insert(fresh);
            substitution.insert(var, bump.alloc(Located::at_zero(Type::Var(fresh))));
        }
    }
    let typ = types::substitute_type(bump, &substitution, annotation.typ);
    let mut predicates: Vec<_> = context
        .iter()
        .map(|p| Pred {
            trait_: p.trait_,
            args: p.args,
        })
        .collect();
    predicates.extend(annotation.context.iter().skip(1).map(|p| {
        Pred {
            trait_: p.trait_,
            args: bump.alloc_slice_fill_iter(
                p.args
                    .iter()
                    .map(|t| types::substitute_type(bump, &substitution, t)),
            ),
        }
    }));
    let mut free: BTreeSet<_> = head_vars.keys().copied().collect();
    types::collect_free_vars(&typ.value, &mut free);
    for predicate in &predicates {
        for arg in predicate.args {
            types::collect_free_vars(&arg.value, &mut free);
        }
    }
    let annotation = Annotation {
        kinds: nash_ast::ValueKinds::unconstrained(bump, free.len()),
        free_vars: bump.alloc_slice_fill_iter(free),
        context: bump.alloc_slice_fill_iter(predicates),
        typ,
    };
    // Substitute the shared method kind signature too. Rechecking the owner
    // predicate alone would freshly instantiate polymorphic constructor kinds
    // and lose their relationship to the method-local variables.
    let arguments: Vec<_> = method
        .annotation
        .free_vars
        .iter()
        .map(|var| {
            substitution
                .get(var)
                .copied()
                .unwrap_or_else(|| bump.alloc(Located::at_zero(Type::Var(var))))
        })
        .collect();
    let kinds = kinds::check_annotation_specialization(
        bump,
        kind_env,
        info.home,
        method.name,
        &annotation,
        Some((method.annotation.kinds, &arguments)),
    )?;
    Ok(bump.alloc(Annotation {
        kinds,
        ..annotation
    }))
}
