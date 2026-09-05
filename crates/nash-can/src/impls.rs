use crate::environment::{Env, TraitInfo, Type as EnvType, dups};
use crate::error::BadHead;
use crate::{Error, kinds, module, types, warning::Warning};
use bumpalo::Bump;
use nash_ast::{
    AliasArgument, AliasType, Annotation, Head, Impl, ImplKey, Pred, QualifiedName, Type,
};
use nash_region::{Located, Region};
use nash_source::Type as SourceType;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn info<'a>(
    bump: &'a Bump,
    home: nash_ast::ModuleName<'a>,
    impl_: &Located<Impl<'a>>,
) -> crate::environment::ImplInfo<'a> {
    crate::environment::ImplInfo {
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
    interfaces: Option<&'a BTreeMap<&'a str, crate::Interface<'a>>>,
    module: &nash_ast::Module<'a>,
) -> Result<crate::environment::Tables<'a>, Vec<Error<'a>>> {
    use crate::environment::{MethodInfo, Tables};
    let mut tables = Tables::default();
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
    Ok(tables)
}

fn insert_impl<'a>(
    bump: &'a Bump,
    table: &mut crate::environment::ImplTable<'a>,
    impl_: &'a crate::environment::ImplInfo<'a>,
) -> Result<(), Vec<Error<'a>>> {
    let key = ImplKey {
        trait_: impl_.trait_,
        heads: bump.alloc_slice_fill_iter(impl_.heads.iter().map(|h| h.value.con())),
    };
    if let Some(first) = table.insert(key, impl_) {
        return Err(vec![Error::OverlappingImpls {
            key: bump.alloc(key),
            first: first.region,
            second: impl_.region,
            first_home: first.home,
            second_home: impl_.home,
        }]);
    }
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
        let mut heads = Vec::new();
        let mut head_types = Vec::new();
        for arg in predicate.args {
            let (head, typ) = canonicalize_head(bump, env, arg, &mut variables)?;
            heads.push(head);
            head_types.push(typ);
        }
        if trait_.home != env.home
            && !heads.iter().any(|h| match &h.value {
                Head::Named { reference, .. } => reference.home == env.home,
                Head::Unit | Head::Tuple(_) => env.home.package == Some(nash_ast::primitives::CORE),
            })
        {
            return Err(vec![Error::OrphanImpl {
                region: source.region,
                trait_,
                heads: bump.alloc_slice_fill_iter(heads.iter().map(|h| h.value.con())),
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
        kinds::check_impl_heads(
            bump,
            kind_env,
            env.home,
            info,
            &head_types,
            &variables,
            context,
        )?;
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
                info,
                method.annotation,
                &head_types,
                context,
                &variables,
            );
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
) -> Result<CanonicalHead<'a>, Vec<Error<'a>>> {
    let mut distinct =
        |args: &[&'a Located<SourceType<'a>>]| -> Result<&'a [&'a str], Vec<Error<'a>>> {
            let mut names = Vec::new();
            for arg in args {
                let SourceType::Var(name) = arg.value else {
                    return Err(vec![Error::BadInstanceHead {
                        region: arg.region,
                        reason: BadHead::NonVariableArgument,
                    }]);
                };
                if let Some(first) = variables.insert(name, arg.region) {
                    return Err(vec![Error::RepeatedHeadVar {
                        name,
                        first,
                        second: arg.region,
                    }]);
                }
                names.push(name);
            }
            Ok(bump.alloc_slice_fill_iter(names))
        };
    let (head, can_type) = match &typ.value {
        SourceType::Type { name, args, .. } | SourceType::TypeQual { name, args, .. } => {
            let prefix = match &typ.value {
                SourceType::TypeQual { module, .. } => Some(*module),
                _ => None,
            };
            let info = types::find_type_info(bump, env, typ.region, prefix, name)?;
            let vars = distinct(args)?;
            let can_args: &[&Located<Type>] = bump.alloc_slice_fill_iter(
                args.iter()
                    .zip(vars)
                    .map(|(arg, var)| &*bump.alloc(Located::at(arg.region, Type::Var(var)))),
            );
            let (home, can_type) = match info {
                EnvType::Union { home, .. } => (
                    home,
                    Type::Named {
                        reference: QualifiedName { home, name },
                        args: can_args,
                    },
                ),
                EnvType::Alias {
                    home,
                    parameters,
                    typ: target,
                    arity,
                } => {
                    if vars.len() > arity {
                        return Err(vec![Error::KindTooManyArgs {
                            region: typ.region,
                            head: kinds::KindHead::Named(QualifiedName { home, name }),
                            applied: vars.len(),
                            accepted: arity,
                        }]);
                    }
                    (
                        home,
                        Type::Alias {
                            reference: QualifiedName { home, name },
                            arguments: bump.alloc_slice_fill_iter(
                                parameters
                                    .iter()
                                    .zip(can_args)
                                    .map(|(name, typ)| AliasArgument { name, typ }),
                            ),
                            target: AliasType::Open(target),
                        },
                    )
                }
            };
            (
                Head::Named {
                    reference: QualifiedName { home, name },
                    vars,
                },
                can_type,
            )
        }
        SourceType::Unit => (Head::Unit, Type::Unit),
        SourceType::Tuple {
            first,
            second,
            rest,
        } => {
            let args: Vec<_> = [*first, *second]
                .into_iter()
                .chain(rest.iter().copied())
                .collect();
            let vars = distinct(&args)?;
            let can_args: Vec<_> = args
                .iter()
                .zip(vars)
                .map(|(arg, name)| &*bump.alloc(Located::at(arg.region, Type::Var(name))))
                .collect();
            (
                Head::Tuple(vars),
                Type::Tuple {
                    first: can_args[0],
                    second: can_args[1],
                    rest: bump.alloc_slice_copy(&can_args[2..]),
                },
            )
        }
        other => {
            return Err(vec![Error::BadInstanceHead {
                region: typ.region,
                reason: match other {
                    SourceType::Var(_) => BadHead::BareVariable,
                    SourceType::Lambda { .. } => BadHead::Function,
                    SourceType::Record(_) => BadHead::Record,
                    SourceType::VarApp { .. } => BadHead::VariableApplication,
                    _ => unreachable!(),
                },
            }]);
        }
    };
    Ok((
        Located::at(typ.region, head),
        bump.alloc(Located::at(typ.region, can_type)),
    ))
}

fn instantiate_method<'a>(
    bump: &'a Bump,
    info: &TraitInfo<'a>,
    annotation: &'a Annotation<'a>,
    heads: &[&'a Located<Type<'a>>],
    context: &'a [Pred<'a>],
    head_vars: &BTreeMap<&'a str, Region>,
) -> &'a Annotation<'a> {
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
    bump.alloc(Annotation {
        free_vars: bump.alloc_slice_fill_iter(free),
        context: bump.alloc_slice_fill_iter(predicates),
        typ,
    })
}
