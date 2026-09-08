use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_ast::{ModuleName, Type as CanType};
use nash_region::{Located, Region};
use nash_source::{Exposed, Exposing, Import as SourceImport, Privacy};

use super::{Binop, Ctor, Env, Info, Type, Var, merge_exposed, merge_qualified};
use crate::error::Error;
use crate::interface::Interface;

/// Per-type import info, mirroring Elm's `rawTypeInfo`: the env type plus
/// the constructors it exposes, both already filtered through interface
/// privacy (`toPublicUnion` / `toPublicAlias`).
type RawTypeInfo<'a> = BTreeMap<&'a str, (Type<'a>, BTreeMap<&'a str, Ctor<'a>>)>;

/// Decide the twin namespace using full metadata, before export privacy can
/// hide the other twin. The supplied ctor itself has already passed privacy.
fn ctor_in_namespace(interface: &Interface<'_>, ctor: Ctor<'_>, qualified: bool) -> bool {
    let Ctor::Union { type_name, .. } = ctor else {
        return true;
    };
    let Some(union) = interface
        .unions
        .iter()
        .find(|union| union.name == type_name)
    else {
        return true;
    };
    let paired = interface.unions.iter().any(|other| {
        super::twin_unions(
            union.name,
            union.parameters.len(),
            union.ctors,
            other.name,
            other.parameters.len(),
            other.ctors,
        )
    });
    !paired || qualified == type_name.as_bytes()[0].is_ascii_uppercase()
}

pub fn create_initial_env<'a>(
    bump: &'a Bump,
    home: ModuleName<'a>,
    interfaces: Option<&BTreeMap<&'a str, Interface<'a>>>,
    imports: &'a [&'a SourceImport<'a>],
) -> Result<Env<'a>, Vec<Error<'a>>> {
    let mut env = Env {
        kinds: crate::kinds::KindEnv::from_interfaces(interfaces),
        traits: BTreeMap::new(),
        q_traits: BTreeMap::new(),
        home,
        vars: BTreeMap::new(),
        types: BTreeMap::new(),
        ctors: BTreeMap::new(),
        binops: BTreeMap::new(),
        q_vars: BTreeMap::new(),
        q_types: BTreeMap::new(),
        q_ctors: BTreeMap::new(),
    };

    // Compiler-known types are always in scope, independently of value imports.
    let builtin_home = nash_ast::primitives::builtin_home();
    for primitive in nash_ast::primitives::PRIMITIVES {
        env.types.insert(
            primitive.name,
            Info::Specific(
                builtin_home,
                Type::Union {
                    arity: primitive.kind.arity(),
                    home: builtin_home,
                },
            ),
        );
    }

    let mut errors = Vec::new();

    for import in imports {
        let interface = match find_interface(interfaces, import) {
            Ok(interface) => interface,
            Err(errs) => {
                errors.extend(errs);
                continue;
            }
        };
        // Filter the synthetic interface before every import route, including
        // explicit exposure and diagnostics. Only exact nash/core may name casts.
        let restricted_interface;
        let interface =
            if interface.home == builtin_home && home.package != Some(nash_ast::primitives::CORE) {
                restricted_interface = Interface {
                    values: bump.alloc_slice_fill_iter(
                        interface
                            .values
                            .iter()
                            .filter(|value| {
                                !nash_ast::primitives::BUILTINS.iter().any(|builtin| {
                                    builtin.name == value.name && builtin.lowering.is_core_only()
                                })
                            })
                            .copied()
                            .collect::<Vec<_>>(),
                    ),
                    ..*interface
                };
                &restricted_interface
            } else {
                interface
            };
        let prefix = import.alias.unwrap_or(import.import.value);

        let raw_type_info = build_raw_type_info(bump, interface);

        // Qualified access is always available, from the same
        // privacy-filtered tables (Elm's `qvs2`/`qts2`/`qcs2`).
        for (name, (typ, ctors)) in &raw_type_info {
            merge_qualified(&mut env.q_types, prefix, name, interface.home, *typ);
            for (ctor_name, ctor) in ctors {
                if ctor_in_namespace(interface, *ctor, true) {
                    merge_qualified(&mut env.q_ctors, prefix, ctor_name, interface.home, *ctor);
                }
            }
        }
        for value in interface.values {
            merge_qualified(
                &mut env.q_vars,
                prefix,
                value.name,
                interface.home,
                super::QualifiedValue {
                    annotation: value.annotation,
                    trait_: None,
                },
            );
        }
        for trait_ in interface.traits.iter().filter(|t| t.exported) {
            let info = trait_info(bump, interface.home, trait_);
            merge_qualified(&mut env.q_traits, prefix, info.name, info.home, info);
            for method in info.methods {
                merge_qualified(
                    &mut env.q_vars,
                    prefix,
                    method.name,
                    info.home,
                    super::QualifiedValue {
                        annotation: method.annotation,
                        trait_: Some(nash_ast::QualifiedName {
                            home: info.home,
                            name: info.name,
                        }),
                    },
                );
            }
        }

        // Unqualified exposure depends on the exposing clause.
        match &import.exposing {
            Exposing::Open => {
                for trait_ in interface.traits.iter().filter(|t| t.exported) {
                    expose_trait(&mut env, trait_info(bump, interface.home, trait_));
                }
                for (name, (typ, ctors)) in &raw_type_info {
                    merge_exposed(&mut env.types, name, interface.home, *typ);
                    for (ctor_name, ctor) in ctors {
                        if ctor_in_namespace(interface, *ctor, false) {
                            merge_exposed(&mut env.ctors, ctor_name, interface.home, *ctor);
                        }
                    }
                }
                for value in interface.values {
                    add_single_value(&mut env, interface.home, value.name, value.annotation);
                }
                for binop in interface.binops {
                    let info = to_env_binop(interface.home, binop);
                    merge_exposed(&mut env.binops, binop.symbol, interface.home, info);
                }
            }
            Exposing::Explicit(exposed) => {
                if let Err(errs) =
                    add_explicit_exposing(bump, &mut env, interface, &raw_type_info, exposed)
                {
                    errors.extend(errs);
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(env)
    } else {
        Err(errors)
    }
}

/// Mirrors Elm's `rawTypeInfo`: build the importable view of an interface,
/// applying union/alias privacy. Closed unions come through with no
/// constructors; private types are absent entirely.
fn build_raw_type_info<'a>(bump: &'a Bump, interface: &Interface<'a>) -> RawTypeInfo<'a> {
    let mut info: RawTypeInfo<'a> = BTreeMap::new();

    for union in interface.unions {
        if let Some(public) = union.to_public() {
            let can_union = bump.alloc(nash_ast::Union {
                name: bump.alloc(Located::at(Region::zero(), public.name)),
                kind: public.kind,
                context: public.context,
                parameters: public.parameters,
                ctors: public.ctors,
                alternatives: public.alternatives,
                options: public.options,
            });
            let typ = Type::Union {
                arity: public.parameters.len(),
                home: interface.home,
            };
            let mut ctors = BTreeMap::new();
            for ctor in public.ctors {
                ctors.insert(
                    ctor.name,
                    make_union_ctor(interface.home, public.name, can_union, ctor),
                );
            }
            info.insert(public.name, (typ, ctors));
        }
    }

    for alias in interface.aliases {
        if let Some(public) = alias.to_public() {
            let typ = Type::Alias {
                arity: public.parameters.len(),
                home: interface.home,
                parameters: public.parameters,
                typ: public.typ,
            };
            let mut ctors = BTreeMap::new();
            if let CanType::Record { fields, ext: None } = &public.typ.value {
                ctors.insert(
                    public.name,
                    super::make_record_ctor(
                        bump,
                        interface.home,
                        public.name,
                        public.parameters,
                        public.typ,
                        fields,
                    ),
                );
            }
            // Elm's `Map.union` is left-biased (unions win), though a
            // union/alias name collision cannot survive canonicalization.
            info.entry(public.name).or_insert((typ, ctors));
        }
    }

    info
}

/// Mirrors Elm's `addExposedValue`.
fn add_explicit_exposing<'a>(
    bump: &'a Bump,
    env: &mut Env<'a>,
    interface: &Interface<'a>,
    raw_type_info: &RawTypeInfo<'a>,
    exposed: &[&'a Exposed<'a>],
) -> Result<(), Vec<Error<'a>>> {
    let mut errors = Vec::new();

    for item in exposed {
        match item {
            Exposed::Lower(name) => {
                if let Some(value) = interface.values.iter().find(|v| v.name == name.value) {
                    add_single_value(env, interface.home, value.name, value.annotation);
                } else if let Some((trait_, method)) = interface
                    .traits
                    .iter()
                    .filter(|t| t.exported)
                    .find_map(|t| {
                        t.methods
                            .iter()
                            .find(|m| m.name == name.value)
                            .map(|m| (t, m))
                    })
                {
                    add_imported_var(
                        env,
                        interface.home,
                        method.name,
                        Var::Method {
                            trait_: nash_ast::QualifiedName {
                                home: interface.home,
                                name: trait_.name,
                            },
                            annotation: method.annotation,
                            local_region: None,
                        },
                    );
                } else {
                    errors.push(Error::ImportExposingNotFound {
                        region: name.region,
                        module: interface.home,
                        name: name.value,
                        available: available_values(bump, interface),
                    });
                }
            }
            Exposed::Upper { name, privacy } | Exposed::LowerType { name, privacy } => {
                if let Some(trait_) = interface
                    .traits
                    .iter()
                    .find(|t| t.exported && t.name == name.value)
                {
                    match privacy {
                        Privacy::Private => {
                            expose_trait(env, trait_info(bump, interface.home, trait_))
                        }
                        Privacy::Public(region) => errors.push(Error::ImportOpenTrait {
                            region: *region,
                            name: name.value,
                        }),
                    }
                    continue;
                }
                match privacy {
                    Privacy::Private => match raw_type_info.get(name.value) {
                        Some((typ, ctors)) => {
                            // Elm overwrites the type entry (`Map.insert`), and
                            // only aliases bring their (record) ctor along.
                            env.types
                                .insert(name.value, Info::Specific(interface.home, *typ));
                            if matches!(typ, Type::Alias { .. }) {
                                for (ctor_name, ctor) in ctors {
                                    merge_exposed(&mut env.ctors, ctor_name, interface.home, *ctor);
                                }
                            }
                        }
                        None => {
                            if let Some(type_name) =
                                check_for_ctor_mistake(raw_type_info, name.value)
                            {
                                errors.push(Error::ImportCtorByName {
                                    region: name.region,
                                    name: name.value,
                                    type_name,
                                });
                            } else {
                                errors.push(Error::ImportExposingNotFound {
                                    region: name.region,
                                    module: interface.home,
                                    name: name.value,
                                    available: available_types(bump, raw_type_info),
                                });
                            }
                        }
                    },
                    Privacy::Public(dot_dot_region) => match raw_type_info.get(name.value) {
                        Some((typ @ Type::Union { .. }, ctors)) => {
                            env.types
                                .insert(name.value, Info::Specific(interface.home, *typ));
                            for (ctor_name, ctor) in ctors {
                                if ctor_in_namespace(interface, *ctor, false) {
                                    merge_exposed(&mut env.ctors, ctor_name, interface.home, *ctor);
                                }
                            }
                        }
                        Some((Type::Alias { .. }, _)) => {
                            errors.push(Error::ImportOpenAlias {
                                region: *dot_dot_region,
                                name: name.value,
                            });
                        }
                        None => {
                            errors.push(Error::ImportExposingNotFound {
                                region: name.region,
                                module: interface.home,
                                name: name.value,
                                available: available_types(bump, raw_type_info),
                            });
                        }
                    },
                }
            }
            Exposed::Operator { region, op } => {
                if let Some(binop) = interface.binops.iter().find(|b| b.symbol == *op) {
                    let info = to_env_binop(interface.home, binop);
                    // Elm overwrites binops (`Map.insert`).
                    env.binops
                        .insert(binop.symbol, Info::Specific(interface.home, info));
                } else {
                    errors.push(Error::ImportExposingNotFound {
                        region: *region,
                        module: interface.home,
                        name: op,
                        available: available_binops(bump, interface),
                    });
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// --- Single-item helpers ---

fn add_single_value<'a>(
    env: &mut Env<'a>,
    home: ModuleName<'a>,
    name: &'a str,
    annotation: &'a nash_ast::Annotation<'a>,
) {
    add_imported_var(env, home, name, Var::Foreign(home, annotation));
}

fn add_imported_var<'a>(env: &mut Env<'a>, home: ModuleName<'a>, name: &'a str, var: Var<'a>) {
    use std::collections::btree_map::Entry;
    match env.vars.entry(name) {
        Entry::Vacant(e) => {
            e.insert(var);
        }
        Entry::Occupied(mut e) => match e.get() {
            // Full canonical comparison, like Elm's `mergeInfo`.
            Var::Foreign(existing, _) if *existing != home => {
                let first = *existing;
                e.insert(Var::Foreigns(first, vec![home]));
            }
            Var::Method { trait_, .. } if trait_.home != home => {
                let first = trait_.home;
                e.insert(Var::Foreigns(first, vec![home]));
            }
            Var::Foreigns(..) => {
                if let Var::Foreigns(_, others) = e.get_mut() {
                    others.push(home);
                }
            }
            _ => {}
        },
    }
}

fn to_env_binop<'a>(
    home: ModuleName<'a>,
    binop: &crate::interface::InterfaceBinop<'a>,
) -> Binop<'a> {
    Binop {
        symbol: binop.symbol,
        home,
        function: binop.function,
        annotation: binop.annotation,
        associativity: binop.associativity,
        precedence: binop.precedence,
    }
}

/// Mirrors Elm's `checkForCtorMistake`: did the user try to expose a
/// constructor by name? Returns the (alphabetically first) owning type.
fn check_for_ctor_mistake<'a>(
    raw_type_info: &RawTypeInfo<'a>,
    given_name: &str,
) -> Option<&'a str> {
    for (_, ctors) in raw_type_info.values() {
        for (ctor_name, ctor) in ctors {
            if *ctor_name != given_name {
                continue;
            }
            match ctor {
                Ctor::Union { type_name, .. } => return Some(type_name),
                Ctor::Bool { union, .. } => return Some(union.name.value),
                Ctor::RecordCtor { .. } => {}
            }
        }
    }
    None
}

fn available_values<'a>(bump: &'a Bump, interface: &Interface<'a>) -> &'a [&'a str] {
    let mut names: Vec<&'a str> = interface.values.iter().map(|v| v.name).collect();
    names.extend(
        interface
            .traits
            .iter()
            .filter(|t| t.exported)
            .flat_map(|t| t.methods.iter().map(|m| m.name)),
    );
    names.sort_unstable();
    bump.alloc_slice_fill_iter(names)
}

fn available_types<'a>(bump: &'a Bump, raw_type_info: &RawTypeInfo<'a>) -> &'a [&'a str] {
    bump.alloc_slice_fill_iter(raw_type_info.keys().copied())
}

fn available_binops<'a>(bump: &'a Bump, interface: &Interface<'a>) -> &'a [&'a str] {
    let mut symbols: Vec<&'a str> = interface.binops.iter().map(|b| b.symbol).collect();
    symbols.sort_unstable();
    bump.alloc_slice_fill_iter(symbols)
}

// --- Construction helpers ---

fn make_union_ctor<'a>(
    home: ModuleName<'a>,
    union_name: &'a str,
    can_union: &'a nash_ast::Union<'a>,
    ctor: &nash_ast::Ctor<'a>,
) -> Ctor<'a> {
    if home == nash_ast::primitives::builtin_home() && union_name == "bool" {
        return Ctor::Bool {
            home,
            union: can_union,
            index: ctor.index,
        };
    }
    Ctor::Union {
        home,
        type_name: union_name,
        type_vars: can_union.parameters,
        union: can_union,
        index: ctor.index,
        arity: ctor.arity,
        arguments: ctor.arguments,
        options: can_union.options,
        alternatives: can_union.alternatives,
    }
}

fn find_interface<'a, 'i>(
    interfaces: Option<&'i BTreeMap<&'a str, Interface<'a>>>,
    import: &SourceImport<'a>,
) -> Result<&'i Interface<'a>, Vec<Error<'a>>> {
    interfaces
        .and_then(|m| m.get(import.import.value))
        .ok_or_else(|| {
            vec![Error::ImportNotFound {
                region: import.import.region,
                module: import.import.value,
            }]
        })
}

fn trait_info<'a>(
    bump: &'a Bump,
    home: ModuleName<'a>,
    trait_: &crate::InterfaceTrait<'a>,
) -> &'a super::TraitInfo<'a> {
    bump.alloc(super::TraitInfo {
        home,
        name: trait_.name,
        parameters: trait_.parameters,
        kinds: trait_.kinds,
        supers: trait_.supers,
        methods: bump.alloc_slice_fill_iter(trait_.methods.iter().map(|m| super::MethodInfo {
            name: m.name,
            annotation: m.annotation,
            has_default: m.has_default,
        })),
    })
}

fn expose_trait<'a>(env: &mut Env<'a>, trait_: &'a super::TraitInfo<'a>) {
    merge_exposed(&mut env.traits, trait_.name, trait_.home, trait_);
    for method in trait_.methods {
        add_imported_var(
            env,
            trait_.home,
            method.name,
            Var::Method {
                trait_: nash_ast::QualifiedName {
                    home: trait_.home,
                    name: trait_.name,
                },
                annotation: method.annotation,
                local_region: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_types_are_pre_seeded() {
        let bump = Bump::new();
        let home = ModuleName {
            package: None,
            name: "Test",
        };
        let env = create_initial_env(&bump, home, None, &[]).unwrap();
        for primitive in nash_ast::primitives::PRIMITIVES {
            match env.types.get(primitive.name) {
                Some(Info::Specific(module, Type::Union { arity, .. })) => {
                    assert_eq!(*module, nash_ast::primitives::builtin_home());
                    assert_eq!(*arity, primitive.kind.arity());
                }
                other => panic!("Expected primitive {}, got {other:?}", primitive.name),
            }
        }
    }
}
