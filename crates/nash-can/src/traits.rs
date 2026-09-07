use bumpalo::Bump;
use nash_ast::{Annotation, Method, Pred, QualifiedName, Trait, Type};
use nash_region::Located;

use crate::environment::{Env, Info, MethodInfo, QualifiedValue, TraitInfo, Var, dups};
use crate::{Error, kinds, module, scc, types, warning::Warning};

pub(crate) struct PreTrait<'a> {
    pub source: &'a Located<nash_source::Trait<'a>>,
    pub parameters: &'a [&'a str],
    pub supers: &'a [Pred<'a>],
    pub methods: &'a [Method<'a>],
}

pub(crate) fn canonicalize<'a>(
    bump: &'a Bump,
    env: &mut Env<'a>,
    kind_env: &mut kinds::KindEnv<'a>,
    source: &nash_source::Module<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a [&'a Located<Trait<'a>>], Vec<Error<'a>>> {
    dups::detect(
        source
            .traits
            .iter()
            .map(|t| (t.value.name.value, t.value.name.region)),
        |name, first, second| Error::DuplicateTrait {
            name,
            first,
            second,
        },
    )?;
    dups::detect(
        source
            .traits
            .iter()
            .flat_map(|t| {
                t.value
                    .methods
                    .iter()
                    .map(|m| (m.name.value, m.name.region))
            })
            .chain(
                source
                    .values
                    .iter()
                    .map(|v| (v.value.name.value, v.value.name.region)),
            ),
        |name, first, second| Error::DuplicateMethod {
            name,
            first,
            second,
        },
    )?;
    for source in source.traits {
        let t = &source.value;
        if let Some(attribute) = t.attributes.first() {
            return Err(vec![Error::Unsupported {
                feature: "attributes",
                region: attribute.name.region,
            }]);
        }
        dups::detect(
            t.params.iter().map(|p| (p.name.value, p.name.region)),
            |name, first, second| Error::DuplicateTraitParameter {
                name,
                first,
                second,
            },
        )?;
        let parameters = bump.alloc_slice_fill_iter(t.params.iter().map(|p| p.name.value));
        let info = bump.alloc(TraitInfo {
            home: env.home,
            name: t.name.value,
            parameters,
            kinds: &[],
            supers: &[],
            methods: &[],
        });
        insert_trait(env, info);
    }
    let mut pre = Vec::new();
    for source in source.traits {
        let t = &source.value;
        let info = env.find_trait(bump, t.name.region, None, t.name.value)?;
        let mut supers = types::canonicalize_context(bump, env, t.supers)?.to_vec();
        supers.extend(t.params.iter().filter_map(|p| {
            p.repr.map(|repr| Pred::Trait {
                trait_: types::repr_trait(repr.value).qualified(),
                args: bump.alloc_slice_copy(&[
                    &*bump.alloc(Located::at(p.name.region, Type::Var(p.name.value)))
                ]),
            })
        }));
        let supers = &*bump.alloc_slice_fill_iter(supers);
        for predicate in supers {
            for argument in predicate.args() {
                if !matches!(&argument.value, Type::Var(name) if info.parameters.contains(name)) {
                    return Err(vec![Error::SuperclassBadArg {
                        region: argument.region,
                        trait_: t.name.value,
                    }]);
                }
            }
        }
        let mut methods = Vec::new();
        for method in t.methods {
            let own = types::to_annotation(bump, env, method.annotation)?;
            if let Some(parameter) = info.parameters.iter().find(|p| !own.free_vars.contains(p)) {
                return Err(vec![Error::MethodMissingParameter {
                    region: method.name.region,
                    method: method.name.value,
                    parameter,
                }]);
            }
            let trait_pred =
                Pred::Trait {
                    trait_: QualifiedName {
                        home: env.home,
                        name: t.name.value,
                    },
                    args: bump.alloc_slice_fill_iter(t.params.iter().map(|p| {
                        &*bump.alloc(Located::at(p.name.region, Type::Var(p.name.value)))
                    })),
                };
            let context = bump.alloc_slice_fill_iter(
                std::iter::once(trait_pred)
                    .chain(own.context.iter().copied())
                    .collect::<Vec<_>>(),
            );
            methods.push(Method {
                name: method.name,
                annotation: bump.alloc(Annotation {
                    free_vars: own.free_vars,
                    context,
                    typ: own.typ,
                }),
                default: None,
            });
        }
        pre.push(PreTrait {
            source,
            parameters: info.parameters,
            supers,
            methods: bump.alloc_slice_fill_iter(methods),
        });
    }
    let nodes = pre
        .iter()
        .map(|t| scc::Node {
            key: t.source.value.name.value,
            value: t.source.value.name,
            deps: t
                .supers
                .iter()
                .filter_map(|p| p.trait_ref())
                .filter(|trait_| trait_.home == env.home)
                .map(|trait_| trait_.name)
                .collect(),
        })
        .collect();
    for group in scc::strongly_connected_components(nodes) {
        if let scc::Scc::Cyclic(names) = group {
            return Err(vec![Error::RecursiveSuperclass {
                names: bump.alloc_slice_fill_iter(names),
            }]);
        }
    }
    let schemes = kinds::infer_traits(bump, kind_env, env.home, &pre)?;
    env.kinds = kind_env.clone();
    for t in &mut pre {
        let methods: Result<Vec<_>, Vec<Error<'a>>> = t
            .methods
            .iter()
            .map(|method| {
                Ok(Method {
                    name: method.name,
                    annotation: kinds::check_annotation(
                        bump,
                        kind_env,
                        env.home,
                        method.name.value,
                        method.annotation,
                    )?,
                    default: method.default,
                })
            })
            .collect();
        t.methods = bump.alloc_slice_fill_iter(methods?);
    }
    // Install every method before any default body is canonicalized.
    for t in &pre {
        let reference = QualifiedName {
            home: env.home,
            name: t.source.value.name.value,
        };
        let info = bump.alloc(TraitInfo {
            home: env.home,
            name: reference.name,
            parameters: t.parameters,
            kinds: schemes[reference.name],
            supers: t.supers,
            methods: bump.alloc_slice_fill_iter(t.methods.iter().zip(t.source.value.methods).map(
                |(m, source)| MethodInfo {
                    name: m.name.value,
                    annotation: m.annotation,
                    has_default: source.default.is_some(),
                },
            )),
        });
        insert_trait(env, info);
        for method in t.methods {
            env.vars.insert(
                method.name.value,
                Var::Method {
                    trait_: reference,
                    annotation: method.annotation,
                    local_region: Some(method.name.region),
                },
            );
            env.q_vars.entry(env.home.name).or_default().insert(
                method.name.value,
                Info::Specific(
                    env.home,
                    QualifiedValue {
                        annotation: method.annotation,
                        trait_: Some(reference),
                    },
                ),
            );
        }
    }
    let mut result = Vec::new();
    for t in pre {
        let mut methods = Vec::new();
        for (method, source) in t.methods.iter().zip(t.source.value.methods) {
            let default = source
                .default
                .map(|def| {
                    module::canonicalize_typed_value(bump, env, def, method.annotation, warnings)
                })
                .transpose()?;
            methods.push(Method {
                name: method.name,
                annotation: method.annotation,
                default,
            });
        }
        result.push(&*bump.alloc(Located::at(
            t.source.region,
            Trait {
                name: t.source.value.name,
                parameters: t.parameters,
                kinds: schemes[t.source.value.name.value],
                supers: t.supers,
                methods: bump.alloc_slice_fill_iter(methods),
            },
        )));
    }
    Ok(bump.alloc_slice_fill_iter(result))
}

fn insert_trait<'a>(env: &mut Env<'a>, info: &'a TraitInfo<'a>) {
    env.traits
        .insert(info.name, Info::Specific(info.home, info));
    env.q_traits
        .entry(env.home.name)
        .or_default()
        .insert(info.name, Info::Specific(info.home, info));
}
