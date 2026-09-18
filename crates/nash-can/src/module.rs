use std::collections::{BTreeMap, BTreeSet};

use bumpalo::Bump;
use nash_ast::{
    Alias as CanAlias, Binop as CanBinop, Ctor as CanCtor, CtorOpts, Decls, Export, Exports,
    Module as CanModule, ModuleName, PackageName, Union as CanUnion,
};
use nash_region::{Located, Region};
use nash_source::{
    Alias as SourceAlias, Ctor as SourceCtor, CtorArgs as SourceCtorArgs, Exposed, Exposing, Infix,
    Module as SourceModule, Privacy, Type as SourceType, Union as SourceUnion,
    Value as SourceValue,
};

use crate::accumulate;
use crate::environment::{self, Env, dups};
use crate::error::{DuplicatePatternContext, VarKind};
use crate::expression;
use crate::kinds;
use crate::pattern;
use crate::scc;
use crate::types;
use crate::warning::{Warning, WarningContext};
use crate::{Error, Interface};

#[derive(Clone, Copy, Debug, Default)]
pub struct Context<'a, 'i> {
    pub package: Option<PackageName<'a>>,
    pub interfaces: Option<&'i BTreeMap<&'a str, Interface<'a>>>,
}

#[derive(Debug)]
pub struct CanResult<'a> {
    pub tables: environment::Tables<'a>,
    pub module: CanModule<'a>,
    pub warnings: Vec<Warning<'a>>,
}

pub fn canonicalize<'a>(
    bump: &'a Bump,
    context: Context<'a, '_>,
    module: &SourceModule<'a>,
) -> Result<CanResult<'a>, Vec<Error<'a>>> {
    if let Some(tests) = module.tests {
        let region = tests.tests.first().map_or_else(
            || {
                tests
                    .imports
                    .first()
                    .map_or(Region::zero(), |import| import.import.region)
            },
            |test| test.region,
        );
        return Err(vec![Error::Unsupported {
            feature: "tests block",
            region,
        }]);
    }
    let name = module
        .name
        .ok_or_else(|| vec![Error::MissingModuleHeader])?;
    let home = ModuleName {
        package: context.package,
        name: name.value,
    };

    let mut env =
        environment::foreign::create_initial_env(bump, home, context.interfaces, module.imports)?;

    // Phase order mirrors Elm's `Local.add`: addTypes (type dups, union
    // free-var checks, alias SCC + canonicalization), then addVars, then
    // addCtors (which canonicalizes constructor argument types).
    environment::local::add_union_types(&mut env, module.unions, module.aliases)?;
    for union in module.unions {
        check_union_free_vars(bump, union)?;
    }
    let pre_aliases = canonicalize_aliases(bump, &mut env, module.aliases)?;
    environment::local::add_vars(&mut env, module.values)?;
    if let nash_ast::ModuleKind::Validator(region) = module.kind
        && !module
            .values
            .iter()
            .any(|value| value.value.name.value == "main")
    {
        return Err(vec![Error::ValidatorMissingMain {
            region,
            module: home.name,
        }]);
    }
    let pre_unions = canonicalize_unions(bump, &env, module.unions)?;
    let mut kind_env = kinds::KindEnv::from_interfaces(context.interfaces);
    let schemes = kinds::infer_declarations(bump, &mut kind_env, home, &pre_unions, &pre_aliases)?;
    env.kinds = kind_env.clone();
    let unions = bump.alloc_slice_fill_iter(pre_unions.iter().map(|u| {
        &*bump.alloc(Located::at(
            u.source.region,
            CanUnion {
                name: u.name,
                parameters: u.parameters,
                ctors: u.ctors,
                alternatives: u.alternatives,
                options: u.options,
                kind: schemes.kind(u.name.value),
                context: schemes.context(u.name.value),
            },
        ))
    }));
    let aliases = bump.alloc_slice_fill_iter(pre_aliases.iter().map(|a| {
        &*bump.alloc(Located::at(
            a.source.region,
            CanAlias {
                name: a.name,
                parameters: a.parameters,
                typ: a.typ,
                kind: schemes.kind(a.name.value),
                context: schemes.context(a.name.value),
            },
        ))
    }));
    environment::local::add_ctors(bump, &mut env, module.unions, unions, aliases)?;

    let mut warnings = Vec::new();
    let traits = crate::traits::canonicalize(bump, &mut env, &mut kind_env, module, &mut warnings)?;
    environment::local::check_binops(&env, module.binops)?;
    let impls = crate::impls::canonicalize(bump, &env, &kind_env, module.impls, &mut warnings)?;
    let decls = canonicalize_decls(bump, &env, module.values, &mut warnings)?;
    let binops = canonicalize_binops(bump, &env, module.binops);
    let exports = canonicalize_exports(bump, module)?;
    if matches!(module.kind, nash_ast::ModuleKind::Validator(_))
        && matches!(&exports, Exports::Explicit(items) if !items.iter().any(|e| matches!(e.value, Export::Value("main"))))
    {
        return Err(vec![Error::ValidatorMainNotExposed {
            region: module.exports.region,
            module: home.name,
        }]);
    }

    let can_module = CanModule {
        traits,
        impls,
        kind: module.kind,
        name: env.home,
        exports,
        docs: module.docs,
        decls,
        unions,
        aliases,
        binops,
    };

    let used_modules = collect_used_modules(&can_module);
    for import in module.imports {
        let module_name = import.import.value;
        if !used_modules.contains(module_name) {
            warnings.push(Warning::UnusedImport {
                region: import.import.region,
                module_name,
            });
        }
    }

    let mut tables = crate::impls::tables(bump, context.interfaces, &can_module, &kind_env)?;
    tables.fields = environment::visible_fields(bump, &env);
    Ok(CanResult {
        tables,
        module: can_module,
        warnings,
    })
}

fn canonicalize_decls<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    values: &'a [&'a Located<SourceValue<'a>>],
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Decls<'a>, Vec<Error<'a>>> {
    if values.is_empty() {
        return Ok(bump.alloc(Decls::Empty));
    }

    let mut errors = Vec::new();
    let mut nodes: Vec<NodeOne<'a>> = Vec::with_capacity(values.len());
    for value in values {
        match to_node_one(bump, env, value, None, warnings) {
            Ok(node) => nodes.push(node),
            Err(errs) => errors.extend(errs),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let top_level_names: BTreeSet<&str> = nodes.iter().map(|n| n.name).collect();

    // Phase 1: SCC on ALL dependencies
    let scc_nodes: Vec<scc::Node<'_, NodeOne<'a>>> = nodes
        .into_iter()
        .map(|node| {
            let deps: Vec<&str> = node
                .free_locals
                .keys()
                .filter(|k| top_level_names.contains(*k))
                .copied()
                .collect();
            scc::Node {
                key: node.name,
                value: node,
                deps,
            }
        })
        .collect();
    let phase1_sccs = scc::strongly_connected_components(scc_nodes);

    let mut decls: &'a Decls<'a> = bump.alloc(Decls::Empty);
    for scc_group in phase1_sccs.into_iter().rev() {
        match scc_group {
            scc::Scc::Acyclic(node) => {
                decls = bump.alloc(Decls::Declare {
                    definition: node.def,
                    next: decls,
                });
            }
            scc::Scc::Cyclic(group) => {
                // Phase 2: SCC on DIRECT deps within the cyclic group,
                // preserving the group's own node order like Elm's
                // `Graph.stronglyConnComp subNodes`.
                let group_names: BTreeSet<&str> = group.iter().map(|n| n.name).collect();

                let phase2_nodes: Vec<scc::Node<'_, &NodeOne<'a>>> = group
                    .iter()
                    .map(|node| {
                        let deps = if node.has_args {
                            vec![] // functions: body is delayed
                        } else {
                            node.free_locals
                                .iter()
                                .filter(|(k, uses)| group_names.contains(*k) && uses.direct > 0)
                                .map(|(k, _)| *k)
                                .collect()
                        };
                        scc::Node {
                            key: node.name,
                            value: node,
                            deps,
                        }
                    })
                    .collect();

                let phase2_sccs = scc::strongly_connected_components(phase2_nodes);

                // Elm's `traverse detectBadCycles` accumulates every bad
                // cycle in this group before giving up.
                let mut rec_defs: Vec<&'a nash_ast::Def<'a>> = Vec::new();
                let mut cycle_errors: Vec<Error<'a>> = Vec::new();
                for sub_scc in phase2_sccs {
                    match sub_scc {
                        scc::Scc::Acyclic(node) => {
                            rec_defs.push(node.def);
                        }
                        scc::Scc::Cyclic(bad_nodes) => {
                            let def_name = match bad_nodes[0].def {
                                nash_ast::Def::Def { name, .. }
                                | nash_ast::Def::TypedDef { name, .. } => name,
                            };
                            cycle_errors.push(Error::RecursiveDecl {
                                name: def_name,
                                others: bump
                                    .alloc_slice_fill_iter(bad_nodes[1..].iter().map(|n| n.name)),
                            });
                        }
                    }
                }
                if !cycle_errors.is_empty() {
                    return Err(cycle_errors);
                }

                if let Some((first, rest)) = rec_defs.split_first() {
                    decls = bump.alloc(Decls::DeclareRec {
                        definition: first,
                        following: bump.alloc_slice_fill_iter(rest.iter().copied()),
                        next: decls,
                    });
                }
            }
        }
    }
    Ok(decls)
}

struct NodeOne<'a> {
    def: &'a nash_ast::Def<'a>,
    name: &'a str,
    has_args: bool,
    free_locals: expression::FreeLocals<'a>,
}

enum TopLevelDefBuilder<'a> {
    Typed {
        context: &'a [nash_ast::Pred<'a>],
        annotation: &'a Located<nash_ast::Type<'a>>,
        free_vars: nash_ast::FreeVars<'a>,
        args: &'a [nash_ast::TypedPattern<'a>],
        typ: &'a Located<nash_ast::Type<'a>>,
    },
    Untyped {
        args: &'a [&'a Located<nash_ast::Pattern<'a>>],
    },
}

fn to_node_one<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    value: &'a Located<SourceValue<'a>>,
    known_annotation: Option<&'a nash_ast::Annotation<'a>>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<NodeOne<'a>, Vec<Error<'a>>> {
    let src = &value.value;
    if let Some(attribute) = src.attributes.first() {
        return Err(vec![Error::Unsupported {
            feature: "attributes",
            region: attribute.name.region,
        }]);
    }

    // Mirrors Elm's `toNodeOne`: typed definitions resolve the annotation
    // and match it against the arguments before the body is touched, and
    // one duplicate scope spans all arguments either way.
    let annotation = match known_annotation {
        Some(annotation) => Some(annotation),
        None => src
            .annotation
            .map(|ann| types::to_annotation(bump, env, ann))
            .transpose()?,
    };
    let annotation = match (known_annotation, annotation) {
        (None, Some(annotation)) => Some(kinds::check_annotation(
            bump,
            &env.kinds,
            src.name.value,
            annotation,
        )?),
        (_, annotation) => annotation,
    };
    let (builder, arg_bindings) = if let Some(annotation) = annotation {
        let mut bound: Vec<(&'a str, Region)> = Vec::new();
        let (typed_args, result_type) = expression::gather_typed_args(
            bump,
            env,
            src.name.value,
            src.arguments,
            annotation.typ,
            &mut bound,
        )?;
        let arg_bindings =
            pattern::detect_duplicates(DuplicatePatternContext::FuncArgs(src.name.value), bound)?;
        (
            TopLevelDefBuilder::Typed {
                context: annotation.context,
                annotation: annotation.typ,
                free_vars: annotation.free_vars,
                args: bump.alloc_slice_fill_iter(typed_args),
                typ: result_type,
            },
            arg_bindings,
        )
    } else {
        let (can_args, arg_bindings) = pattern::verify_all(
            bump,
            env,
            DuplicatePatternContext::FuncArgs(src.name.value),
            src.arguments,
        )?;
        (
            TopLevelDefBuilder::Untyped {
                args: bump.alloc_slice_fill_iter(can_args),
            },
            arg_bindings,
        )
    };

    let body_env = crate::environment::Scope::new(env, None, &arg_bindings)?;
    let mut free_locals = expression::FreeLocals::new();
    let can_body =
        expression::canonicalize_expr(bump, &body_env, src.body, &mut free_locals, warnings)?;

    let outer_free = expression::verify_bindings(
        WarningContext::Pattern,
        &arg_bindings,
        free_locals,
        warnings,
    );

    let def = match builder {
        TopLevelDefBuilder::Typed {
            context,
            annotation,
            free_vars,
            args,
            typ,
        } => bump.alloc(nash_ast::Def::TypedDef {
            context,
            annotation,
            name: src.name,
            free_vars,
            args,
            body: can_body,
            typ,
        }),
        TopLevelDefBuilder::Untyped { args } => bump.alloc(nash_ast::Def::Def {
            name: src.name,
            args,
            body: can_body,
        }),
    };

    Ok(NodeOne {
        def,
        name: src.name.value,
        has_args: !src.arguments.is_empty(),
        free_locals: outer_free,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct PreUnion<'a> {
    pub source: &'a Located<SourceUnion<'a>>,
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub ctors: &'a [&'a CanCtor<'a>],
    pub context: &'a [nash_ast::Pred<'a>],
    pub alternatives: u16,
    pub options: CtorOpts,
}

#[derive(Clone, Copy)]
pub(crate) struct PreAlias<'a> {
    pub source: &'a Located<SourceAlias<'a>>,
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub typ: &'a Located<nash_ast::Type<'a>>,
    pub context: &'a [nash_ast::Pred<'a>],
}

fn canonicalize_unions<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    unions: &'a [&'a Located<SourceUnion<'a>>],
) -> Result<Vec<PreUnion<'a>>, Vec<Error<'a>>> {
    accumulate::try_all(
        unions
            .iter()
            .map(|union| canonicalize_union(bump, env, union)),
    )
}

fn ctor_arg_types<'a>(
    bump: &'a Bump,
    ctor: &'a SourceCtor<'a>,
) -> &'a [&'a Located<SourceType<'a>>] {
    match &ctor.arguments {
        SourceCtorArgs::Positional(args) => args,
        SourceCtorArgs::Labeled(fields) => {
            bump.alloc_slice_fill_iter(fields.iter().map(|(_, typ)| *typ))
        }
    }
}

fn canonicalize_union<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    source_union: &'a Located<SourceUnion<'a>>,
) -> Result<PreUnion<'a>, Vec<Error<'a>>> {
    let union = &source_union.value;
    if let Some(attribute) = union.attributes.first() {
        return Err(vec![Error::Unsupported {
            feature: "attributes",
            region: attribute.name.region,
        }]);
    }
    let parameters =
        bump.alloc_slice_fill_iter(union.arguments.iter().copied().map(|arg| arg.name.value));
    let ctors = canonicalize_ctors(bump, env, union.ctors)?;
    let mut context = parameter_repr_predicates(bump, union.arguments);
    for ctor in union.ctors {
        for typ in ctor_arg_types(bump, ctor) {
            context.extend(types::repr_predicates(bump, env, typ)?);
        }
    }
    let context = bump.alloc_slice_fill_iter(context);
    let alternatives = union
        .ctors
        .len()
        .try_into()
        .expect("union alternatives exceed u16");
    let options = if union.ctors.len() == 1 && ctor_arg_types(bump, union.ctors[0]).len() == 1 {
        CtorOpts::Unbox
    } else if union
        .ctors
        .iter()
        .all(|ctor| ctor_arg_types(bump, ctor).is_empty())
    {
        CtorOpts::Enum
    } else {
        CtorOpts::Normal
    };

    Ok(PreUnion {
        source: source_union,
        context,
        name: union.name,
        parameters,
        ctors,
        alternatives,
        options,
    })
}

fn canonicalize_ctors<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    ctors: &'a [&'a SourceCtor<'a>],
) -> Result<&'a [&'a CanCtor<'a>], Vec<Error<'a>>> {
    accumulate::try_all_alloc_ref(
        bump,
        ctors.iter().copied().enumerate().map(|(index, ctor)| {
            let source_arguments = ctor_arg_types(bump, ctor);
            if matches!(&ctor.arguments, SourceCtorArgs::Positional(_))
                && let Some(record) = source_arguments
                    .iter()
                    .find(|arg| matches!(arg.value.unannotated(), SourceType::Record(_)))
            {
                return Err(vec![Error::Unsupported {
                    feature: "anonymous record constructor arguments",
                    region: record.region,
                }]);
            }
            let labels = match &ctor.arguments {
                SourceCtorArgs::Positional(_) => None,
                SourceCtorArgs::Labeled(fields) => {
                    dups::detect(
                        fields.iter().map(|(name, _)| (name.value, name.region)),
                        |name, first, second| Error::DuplicateField {
                            name,
                            first,
                            second,
                        },
                    )?;
                    Some(&*bump.alloc_slice_fill_iter(fields.iter().map(|(name, _)| name.value)))
                }
            };
            let arguments = types::canonicalize_type_arguments(bump, env, source_arguments)?;
            Ok(&*bump.alloc(CanCtor {
                labels,
                name: ctor.name.value,
                index: index.try_into().expect("constructor index exceeds u16"),
                arity: source_arguments
                    .len()
                    .try_into()
                    .expect("constructor arity exceeds u16"),
                arguments,
            }))
        }),
    )
}

fn canonicalize_aliases<'a>(
    bump: &'a Bump,
    env: &mut Env<'a>,
    source_aliases: &'a [&'a Located<SourceAlias<'a>>],
) -> Result<Vec<PreAlias<'a>>, Vec<Error<'a>>> {
    let alias_names: BTreeSet<&str> = source_aliases.iter().map(|a| a.value.name.value).collect();

    let scc_nodes: Vec<scc::Node<'_, &'a Located<SourceAlias<'a>>>> = source_aliases
        .iter()
        .map(|&alias| {
            let mut deps = Vec::new();
            collect_type_edges(&alias.value.typ.value, &alias_names, &mut deps);
            deps.reverse();
            scc::Node {
                key: alias.value.name.value,
                value: alias,
                deps,
            }
        })
        .collect();
    let sccs = scc::strongly_connected_components(scc_nodes);

    let mut results: BTreeMap<&str, PreAlias> = BTreeMap::new();
    for component in sccs {
        match component {
            scc::Scc::Acyclic(source) => {
                check_alias_free_vars(bump, source)?;
                let alias = canonicalize_single_alias(bump, env, source)?;
                environment::local::add_alias_type(
                    env,
                    alias.name.value,
                    alias.parameters,
                    alias.typ,
                );
                results.insert(source.value.name.value, alias);
            }
            scc::Scc::Cyclic(cycle) => {
                // Elm checks the head alias's type variables before
                // reporting the cycle, so a messed-up cyclic alias gets
                // the variable error first.
                let first = &cycle[0];
                check_alias_free_vars(bump, first)?;
                return Err(vec![Error::RecursiveAlias {
                    region: first.value.name.region,
                    name: first.value.name.value,
                    args: bump
                        .alloc_slice_fill_iter(first.value.arguments.iter().map(|a| a.name.value)),
                    typ: first.value.typ,
                    others: bump
                        .alloc_slice_fill_iter(cycle[1..].iter().map(|a| a.value.name.value)),
                }]);
            }
        }
    }

    Ok(source_aliases
        .iter()
        .map(|a| {
            *results
                .get(a.value.name.value)
                .expect("all acyclic aliases inserted into results")
        })
        .collect())
}

fn canonicalize_single_alias<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    source_alias: &'a Located<SourceAlias<'a>>,
) -> Result<PreAlias<'a>, Vec<Error<'a>>> {
    let alias = &source_alias.value;
    if let Some(attribute) = alias.attributes.first() {
        return Err(vec![Error::Unsupported {
            feature: "attributes",
            region: attribute.name.region,
        }]);
    }
    let parameters =
        bump.alloc_slice_fill_iter(alias.arguments.iter().copied().map(|arg| arg.name.value));
    let typ = types::canonicalize_alias_body(bump, env, alias.typ)?;

    let mut context = parameter_repr_predicates(bump, alias.arguments);
    context.extend(types::alias_repr_predicates(bump, env, alias.typ)?);
    let can_alias = PreAlias {
        context: bump.alloc_slice_fill_iter(context),
        source: source_alias,
        name: alias.name,
        parameters,
        typ,
    };

    Ok(can_alias)
}

fn check_union_free_vars<'a>(
    bump: &'a Bump,
    union: &'a Located<SourceUnion<'a>>,
) -> Result<(), Vec<Error<'a>>> {
    let u = &union.value;

    // Elm builds the argument dups dict with foldr, so occurrences are
    // inserted in reverse source order; replicated for identical regions.
    dups::detect(
        u.arguments
            .iter()
            .rev()
            .map(|a| (a.name.value, a.name.region)),
        |arg_name, first, second| Error::DuplicateUnionArg {
            type_name: u.name.value,
            arg_name,
            first,
            second,
        },
    )?;

    let bound: BTreeSet<&str> = u.arguments.iter().map(|a| a.name.value).collect();

    // Elm folds ctors with foldr and overwriting inserts: later ctors are
    // processed first, so earlier ctors win region conflicts.
    let mut free_vars: BTreeMap<&str, Region> = BTreeMap::new();
    for ctor in u.ctors.iter().rev() {
        for arg in ctor_arg_types(bump, ctor) {
            collect_free_type_vars(arg, &mut free_vars);
        }
    }

    let unbound: Vec<(&str, Region)> = free_vars
        .into_iter()
        .filter(|(name, _)| !bound.contains(name))
        .collect();

    if unbound.is_empty() {
        Ok(())
    } else {
        let args = bump.alloc_slice_fill_iter(u.arguments.iter().map(|a| a.name.value));
        let (first_unbound, rest_unbound) = unbound
            .split_first()
            .expect("unbound is non-empty: guarded by is_empty check");
        Err(vec![Error::TypeVarsUnboundInUnion {
            region: union.region,
            name: u.name.value,
            args,
            unbound: *first_unbound,
            more_unbound: bump.alloc_slice_fill_iter(rest_unbound.iter().copied()),
        }])
    }
}

fn check_alias_free_vars<'a>(
    bump: &'a Bump,
    alias: &'a Located<SourceAlias<'a>>,
) -> Result<(), Vec<Error<'a>>> {
    let a = &alias.value;

    // Reverse source order, matching Elm's foldr-built dups dict.
    dups::detect(
        a.arguments
            .iter()
            .rev()
            .map(|arg| (arg.name.value, arg.name.region)),
        |arg_name, first, second| Error::DuplicateAliasArg {
            type_name: a.name.value,
            arg_name,
            first,
            second,
        },
    )?;

    let bound: BTreeSet<&str> = a.arguments.iter().map(|arg| arg.name.value).collect();

    let mut free_vars: BTreeMap<&str, Region> = BTreeMap::new();
    collect_free_type_vars(a.typ, &mut free_vars);

    // Name-sorted, like Elm's `Map.toList (Map.difference bound free)`.
    let unused: BTreeMap<&str, Region> = a
        .arguments
        .iter()
        .filter(|arg| !free_vars.contains_key(arg.name.value))
        .map(|arg| (arg.name.value, arg.name.region))
        .collect();

    let unbound: Vec<(&str, Region)> = free_vars
        .into_iter()
        .filter(|(name, _)| !bound.contains(name))
        .collect();

    if unused.is_empty() && unbound.is_empty() {
        Ok(())
    } else {
        let args = bump.alloc_slice_fill_iter(a.arguments.iter().map(|arg| arg.name.value));
        Err(vec![Error::TypeVarsMessedUpInAlias {
            region: alias.region,
            name: a.name.value,
            args,
            unused: bump.alloc_slice_fill_iter(unused),
            unbound: bump.alloc_slice_fill_iter(unbound),
        }])
    }
}

fn collect_type_edges<'a>(
    typ: &SourceType<'a>,
    alias_names: &BTreeSet<&'a str>,
    edges: &mut Vec<&'a str>,
) {
    match typ {
        SourceType::Repr { typ, .. } => collect_type_edges(&typ.value, alias_names, edges),
        SourceType::Lambda { from, to } => {
            collect_type_edges(&from.value, alias_names, edges);
            collect_type_edges(&to.value, alias_names, edges);
        }
        SourceType::Var(_) => {}
        SourceType::VarApp { args, .. } => {
            for arg in *args {
                collect_type_edges(&arg.value, alias_names, edges);
            }
        }
        SourceType::Type { name, args, .. } => {
            // Elm's `getEdges` keeps duplicates; the caller reverses the
            // final list to match its prepend accumulation.
            if alias_names.contains(name) {
                edges.push(name);
            }
            for arg in *args {
                collect_type_edges(&arg.value, alias_names, edges);
            }
        }
        SourceType::TypeQual { args, .. } => {
            // Qualified refs are external, not local alias deps
            for arg in *args {
                collect_type_edges(&arg.value, alias_names, edges);
            }
        }
        SourceType::Record(fields) => {
            for field in *fields {
                collect_type_edges(&field.typ.value, alias_names, edges);
            }
        }
        SourceType::Unit => {}
        SourceType::Tuple {
            first,
            second,
            rest,
        } => {
            collect_type_edges(&first.value, alias_names, edges);
            collect_type_edges(&second.value, alias_names, edges);
            for r in *rest {
                collect_type_edges(&r.value, alias_names, edges);
            }
        }
    }
}

/// Mirrors Elm's `addFreeVars`: overwriting inserts (the last occurrence
/// wins the region), and the record extension variable counts as free.
fn collect_free_type_vars<'a>(typ: &Located<SourceType<'a>>, vars: &mut BTreeMap<&'a str, Region>) {
    match &typ.value {
        SourceType::Repr { typ, .. } => collect_free_type_vars(typ, vars),
        SourceType::Var(name) => {
            vars.insert(name, typ.region);
        }
        SourceType::VarApp { name, region, args } => {
            vars.insert(name, *region);
            for arg in *args {
                collect_free_type_vars(arg, vars);
            }
        }
        SourceType::Lambda { from, to } => {
            collect_free_type_vars(from, vars);
            collect_free_type_vars(to, vars);
        }
        SourceType::Type { args, .. } | SourceType::TypeQual { args, .. } => {
            for arg in *args {
                collect_free_type_vars(arg, vars);
            }
        }
        SourceType::Record(fields) => {
            for field in *fields {
                collect_free_type_vars(field.typ, vars);
            }
        }
        SourceType::Unit => {}
        SourceType::Tuple {
            first,
            second,
            rest,
        } => {
            collect_free_type_vars(first, vars);
            collect_free_type_vars(second, vars);
            for r in *rest {
                collect_free_type_vars(r, vars);
            }
        }
    }
}

/// Mirrors Elm's `canonicalizeExports`: each exposed item is resolved
/// against the module's own values/types/binops first (accumulating all
/// resolution errors), and only then are duplicates detected. The result
/// is name-keyed, hence name-sorted, like Elm's `Map Name Export`.
fn canonicalize_exports<'a>(
    bump: &'a Bump,
    module: &SourceModule<'a>,
) -> Result<Exports<'a>, Vec<Error<'a>>> {
    match module.exports.value {
        Exposing::Open => Ok(Exports::Everything(module.exports.region)),
        Exposing::Explicit(exposed) => {
            let value_names: BTreeSet<&str> =
                module.values.iter().map(|v| v.value.name.value).collect();
            let union_names: BTreeSet<&str> =
                module.unions.iter().map(|u| u.value.name.value).collect();
            let alias_names: BTreeSet<&str> =
                module.aliases.iter().map(|a| a.value.name.value).collect();
            let trait_names: BTreeSet<&str> =
                module.traits.iter().map(|t| t.value.name.value).collect();
            let binop_names: BTreeSet<&str> = module.binops.iter().map(|b| b.value.op).collect();

            let mut resolved: Vec<(&'a str, Region, Export<'a>)> = Vec::new();
            let mut errors: Vec<Error<'a>> = Vec::new();

            for item in exposed {
                match item {
                    Exposed::Lower(name) => {
                        if value_names.contains(name.value) {
                            resolved.push((name.value, name.region, Export::Value(name.value)));
                        } else {
                            errors.push(Error::ExportNotFound {
                                region: name.region,
                                kind: VarKind::BadVar,
                                name: name.value,
                                suggestions: bump
                                    .alloc_slice_fill_iter(value_names.iter().copied()),
                            });
                        }
                    }
                    Exposed::Operator { region, op } => {
                        if binop_names.contains(*op) {
                            resolved.push((op, *region, Export::Binop(op)));
                        } else {
                            errors.push(Error::ExportNotFound {
                                region: *region,
                                kind: VarKind::BadOp,
                                name: op,
                                suggestions: bump
                                    .alloc_slice_fill_iter(binop_names.iter().copied()),
                            });
                        }
                    }
                    Exposed::Upper { name, privacy } | Exposed::LowerType { name, privacy } => {
                        if trait_names.contains(name.value) {
                            match privacy {
                                Privacy::Private => resolved.push((
                                    name.value,
                                    name.region,
                                    Export::Trait(name.value),
                                )),
                                Privacy::Public(region) => errors.push(Error::ExportOpenTrait {
                                    region: *region,
                                    name: name.value,
                                }),
                            }
                            continue;
                        }
                        match privacy {
                            Privacy::Public(dot_dot_region) => {
                                if union_names.contains(name.value) {
                                    resolved.push((
                                        name.value,
                                        name.region,
                                        Export::UnionOpen(name.value),
                                    ));
                                } else if alias_names.contains(name.value) {
                                    errors.push(Error::ExportOpenAlias {
                                        region: *dot_dot_region,
                                        name: name.value,
                                    });
                                } else {
                                    errors.push(Error::ExportNotFound {
                                        region: name.region,
                                        kind: VarKind::BadType,
                                        name: name.value,
                                        suggestions: type_suggestions(
                                            bump,
                                            &union_names,
                                            &alias_names,
                                        ),
                                    });
                                }
                            }
                            Privacy::Private => {
                                if union_names.contains(name.value) {
                                    resolved.push((
                                        name.value,
                                        name.region,
                                        Export::UnionClosed(name.value),
                                    ));
                                } else if alias_names.contains(name.value) {
                                    resolved.push((
                                        name.value,
                                        name.region,
                                        Export::Alias(name.value),
                                    ));
                                } else {
                                    errors.push(Error::ExportNotFound {
                                        region: name.region,
                                        kind: VarKind::BadType,
                                        name: name.value,
                                        suggestions: type_suggestions(
                                            bump,
                                            &union_names,
                                            &alias_names,
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            if !errors.is_empty() {
                return Err(errors);
            }

            let mut occurrences: BTreeMap<&'a str, Vec<(Region, Export<'a>)>> = BTreeMap::new();
            for (name, region, export) in resolved {
                occurrences.entry(name).or_default().push((region, export));
            }

            let mut exports: Vec<&'a Located<Export<'a>>> = Vec::new();
            let mut dup_errors: Vec<Error<'a>> = Vec::new();
            for (name, entries) in occurrences {
                if entries.len() > 1 {
                    dup_errors.push(Error::ExportDuplicate {
                        name,
                        first: entries[0].0,
                        second: entries[1].0,
                    });
                } else {
                    let (region, export) = entries.into_iter().next().expect("one entry");
                    exports.push(bump.alloc(Located::at(region, export)));
                }
            }

            if !dup_errors.is_empty() {
                return Err(dup_errors);
            }

            Ok(Exports::Explicit(bump.alloc_slice_fill_iter(exports)))
        }
    }
}

/// Elm suggests `Map.keys unions ++ Map.keys aliases` for a bad type export.
fn type_suggestions<'a>(
    bump: &'a Bump,
    union_names: &BTreeSet<&'a str>,
    alias_names: &BTreeSet<&'a str>,
) -> &'a [&'a str] {
    let names: Vec<&'a str> = union_names
        .iter()
        .chain(alias_names.iter())
        .copied()
        .collect();
    bump.alloc_slice_fill_iter(names)
}

fn canonicalize_binops<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    binops: &'a [&'a Located<Infix<'a>>],
) -> &'a [&'a Located<CanBinop<'a>>] {
    bump.alloc_slice_fill_iter(binops.iter().copied().map(|binop| {
        let (home, annotation) = match env.vars.get(binop.value.name) {
            Some(environment::Var::TopLevel(_)) => (env.home, None),
            Some(environment::Var::Foreign(home, annotation)) => (*home, Some(*annotation)),
            Some(environment::Var::Method {
                trait_, annotation, ..
            }) => (trait_.home, Some(*annotation)),
            _ => unreachable!("check_binops validates every backing function"),
        };
        &*bump.alloc(Located::at(
            binop.region,
            CanBinop {
                symbol: binop.value.op,
                associativity: binop.value.associativity,
                precedence: binop.value.precedence,
                function: nash_ast::QualifiedName {
                    home,
                    name: binop.value.name,
                },
                annotation,
            },
        ))
    }))
}

// --- Unused import detection ---

fn collect_used_modules<'a>(module: &CanModule<'a>) -> BTreeSet<&'a str> {
    let mut used = BTreeSet::new();
    let home = module.name;
    for binop in module.binops {
        add_if_foreign(home, binop.value.function.home, &mut used);
    }
    collect_from_decls(module.decls, home, &mut used);
    for impl_ in module.impls {
        add_if_foreign(home, impl_.value.trait_.home, &mut used);
        for head in impl_.value.heads {
            let mut pending = vec![&head.value];
            while let Some(head) = pending.pop() {
                if let nash_ast::Head::Named { reference, .. } = head {
                    add_if_foreign(home, reference.home, &mut used);
                }
                pending.extend(nash_ast::head::children(head));
            }
        }
        for predicate in impl_.value.context {
            collect_from_predicate(predicate, home, &mut used);
        }
        for method in impl_.value.methods {
            collect_from_def(method, home, &mut used);
        }
    }
    for trait_ in module.traits {
        for predicate in trait_.value.supers {
            collect_from_predicate(predicate, home, &mut used);
        }
        for method in trait_.value.methods {
            collect_from_type(&method.annotation.typ.value, home, &mut used);
            for predicate in method.annotation.context {
                collect_from_predicate(predicate, home, &mut used);
            }
            if let Some(default) = method.default {
                collect_from_def(default, home, &mut used);
            }
        }
    }
    for union in module.unions {
        for ctor in union.value.ctors {
            for arg in ctor.arguments {
                collect_from_type(&arg.value, home, &mut used);
            }
        }
    }
    for alias in module.aliases {
        collect_from_type(&alias.value.typ.value, home, &mut used);
    }
    used
}

fn add_if_foreign<'a>(
    home: ModuleName<'a>,
    reference_home: ModuleName<'a>,
    used: &mut BTreeSet<&'a str>,
) {
    if reference_home != home {
        used.insert(reference_home.name);
    }
}

fn collect_from_decls<'a>(decls: &Decls<'a>, home: ModuleName<'a>, used: &mut BTreeSet<&'a str>) {
    match decls {
        Decls::Declare { definition, next } => {
            collect_from_def(definition, home, used);
            collect_from_decls(next, home, used);
        }
        Decls::DeclareRec {
            definition,
            following,
            next,
        } => {
            collect_from_def(definition, home, used);
            for def in *following {
                collect_from_def(def, home, used);
            }
            collect_from_decls(next, home, used);
        }
        Decls::Empty => {}
    }
}

fn collect_from_def<'a>(
    def: &nash_ast::Def<'a>,
    home: ModuleName<'a>,
    used: &mut BTreeSet<&'a str>,
) {
    match def {
        nash_ast::Def::Def { body, args, .. } => {
            for arg in *args {
                collect_from_pattern(&arg.value, home, used);
            }
            collect_from_expr(&body.value, home, used);
        }
        nash_ast::Def::TypedDef {
            context,
            annotation,
            args,
            body,
            typ,
            ..
        } => {
            for predicate in *context {
                collect_from_predicate(predicate, home, used);
            }
            for arg in *args {
                collect_from_pattern(&arg.pattern.value, home, used);
                collect_from_type(&arg.typ.value, home, used);
            }
            collect_from_expr(&body.value, home, used);
            collect_from_type(&annotation.value, home, used);
            collect_from_type(&typ.value, home, used);
        }
    }
}

fn collect_from_expr<'a>(
    expr: &nash_ast::Expr<'a>,
    home: ModuleName<'a>,
    used: &mut BTreeSet<&'a str>,
) {
    use nash_ast::Expr::*;
    match expr {
        Assert(inner) | Comptime(inner) => collect_from_expr(&inner.value, home, used),
        Fail(message) | Todo(message) => {
            if let Some(message) = message {
                collect_from_expr(&message.value, home, used);
            }
        }
        Trace { message, body } => {
            collect_from_expr(&message.value, home, used);
            collect_from_expr(&body.value, home, used);
        }
        VarLocal(_) | Accessor(_) | Unit => {}
        Str(_) | Bytes(_) | Int(_) => {
            add_if_foreign(home, nash_ast::primitives::literal_home(), used);
        }
        VarTopLevel(q) => add_if_foreign(home, q.home, used),
        // Only the reference counts as a use: the annotation is data from
        // the origin module's solver, not something written here.
        VarForeign { reference, .. } => add_if_foreign(home, reference.home, used),
        VarMethod { trait_, .. } => add_if_foreign(home, trait_.home, used),
        VarConstructor {
            reference,
            annotation,
            ..
        } => {
            add_if_foreign(home, reference.home, used);
            collect_from_type(&annotation.typ.value, home, used);
        }
        VarOperator { operator_home, .. } => {
            add_if_foreign(home, *operator_home, used);
        }
        Binop {
            operator_home,
            left,
            right,
            ..
        } => {
            add_if_foreign(home, *operator_home, used);
            collect_from_expr(&left.value, home, used);
            collect_from_expr(&right.value, home, used);
        }
        List(items) => {
            for item in *items {
                collect_from_expr(&item.value, home, used);
            }
        }
        Lambda { parameters, body } => {
            for p in *parameters {
                collect_from_pattern(&p.value, home, used);
            }
            collect_from_expr(&body.value, home, used);
        }
        Call {
            function,
            arguments,
        } => {
            collect_from_expr(&function.value, home, used);
            for a in *arguments {
                collect_from_expr(&a.value, home, used);
            }
        }
        If {
            branches,
            final_else,
        } => {
            for b in *branches {
                collect_from_expr(&b.condition.value, home, used);
                collect_from_expr(&b.then_branch.value, home, used);
            }
            collect_from_expr(&final_else.value, home, used);
        }
        Let { definition, body } => {
            collect_from_def(definition, home, used);
            collect_from_expr(&body.value, home, used);
        }
        LetRec { definitions, body } => {
            for d in *definitions {
                collect_from_def(d, home, used);
            }
            collect_from_expr(&body.value, home, used);
        }
        LetDestruct {
            pattern,
            value,
            body,
        } => {
            collect_from_pattern(&pattern.value, home, used);
            collect_from_expr(&value.value, home, used);
            collect_from_expr(&body.value, home, used);
        }
        Case {
            scrutinee,
            branches,
        } => {
            collect_from_expr(&scrutinee.value, home, used);
            for b in *branches {
                collect_from_pattern(&b.pattern.value, home, used);
                collect_from_expr(&b.body.value, home, used);
            }
        }
        Access { record, .. } => collect_from_expr(&record.value, home, used),
        Update { base, fields, .. } => {
            collect_from_expr(&base.value, home, used);
            for f in *fields {
                collect_from_expr(&f.value.value, home, used);
            }
        }
        Record {
            alias,
            annotation,
            fields,
        } => {
            add_if_foreign(home, alias.home, used);
            collect_from_type(&annotation.typ.value, home, used);
            for f in *fields {
                collect_from_expr(&f.value.value, home, used);
            }
        }
        Tuple {
            first,
            second,
            rest,
        } => {
            collect_from_expr(&first.value, home, used);
            collect_from_expr(&second.value, home, used);
            for r in *rest {
                collect_from_expr(&r.value, home, used);
            }
        }
    }
}

fn collect_from_pattern<'a>(
    pat: &nash_ast::Pattern<'a>,
    home: ModuleName<'a>,
    used: &mut BTreeSet<&'a str>,
) {
    use nash_ast::Pattern::*;
    match pat {
        Anything | Var(_) | Unit | Record(_) => {}
        Str(_) | Bytes(_) | Int(_) => {
            add_if_foreign(home, nash_ast::primitives::literal_home(), used);
            add_if_foreign(
                home,
                nash_ast::ModuleName {
                    package: Some(nash_ast::primitives::CORE),
                    name: "Eq",
                },
                used,
            );
        }
        // Only the exact builtin bool type produces this pattern form.
        Bool { .. } => {
            add_if_foreign(home, nash_ast::primitives::builtin_home(), used);
        }
        Constructor(ctor) => {
            add_if_foreign(home, ctor.reference.home, used);
            for arg in ctor.arguments {
                collect_from_type(&arg.typ.value, home, used);
                collect_from_pattern(&arg.pattern.value, home, used);
            }
        }
        Alias { pattern, .. } => collect_from_pattern(&pattern.value, home, used),
        Tuple {
            first,
            second,
            rest,
        } => {
            collect_from_pattern(&first.value, home, used);
            collect_from_pattern(&second.value, home, used);
            for r in *rest {
                collect_from_pattern(&r.value, home, used);
            }
        }
        List(items) => {
            for item in *items {
                collect_from_pattern(&item.value, home, used);
            }
        }
        Cons { head, tail } => {
            collect_from_pattern(&head.value, home, used);
            collect_from_pattern(&tail.value, home, used);
        }
    }
}

fn collect_from_type<'a>(
    typ: &nash_ast::Type<'a>,
    home: ModuleName<'a>,
    used: &mut BTreeSet<&'a str>,
) {
    use nash_ast::Type::*;
    match typ {
        App { head, args } => {
            collect_from_type(&head.value, home, used);
            for arg in *args {
                collect_from_type(&arg.value, home, used);
            }
        }
        Var(_) => {}
        Lambda { from, to } => {
            collect_from_type(&from.value, home, used);
            collect_from_type(&to.value, home, used);
        }
        Named { reference, args } => {
            add_if_foreign(home, reference.home, used);
            for a in *args {
                collect_from_type(&a.value, home, used);
            }
        }
        Record { fields, .. } => {
            for f in *fields {
                collect_from_type(&f.typ.value, home, used);
            }
        }
        Tuple {
            first,
            second,
            rest,
        } => {
            collect_from_type(&first.value, home, used);
            collect_from_type(&second.value, home, used);
            for r in *rest {
                collect_from_type(&r.value, home, used);
            }
        }
        Alias {
            reference,
            arguments,
            target,
            ..
        } => {
            add_if_foreign(home, reference.home, used);
            for a in *arguments {
                collect_from_type(&a.typ.value, home, used);
            }
            match target {
                nash_ast::AliasType::Open(t) | nash_ast::AliasType::Filled { typ: t, .. } => {
                    collect_from_type(&t.value, home, used);
                }
            }
        }
    }
}

pub(crate) fn canonicalize_typed_value<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    definition: &'a Located<nash_source::Def<'a>>,
    annotation: &'a nash_ast::Annotation<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a nash_ast::Def<'a>, Vec<Error<'a>>> {
    let nash_source::Def::Define {
        name, args, body, ..
    } = &definition.value
    else {
        unreachable!("trait defaults are named definitions")
    };
    let value = bump.alloc(Located::at(
        definition.region,
        SourceValue {
            name,
            arguments: args,
            body,
            annotation: None,
            attributes: &[],
        },
    ));
    Ok(to_node_one(bump, env, value, Some(annotation), warnings)?.def)
}

fn collect_from_predicate<'a>(
    predicate: &nash_ast::Pred<'a>,
    home: ModuleName<'a>,
    used: &mut BTreeSet<&'a str>,
) {
    if let Some(trait_) = predicate.trait_ref() {
        add_if_foreign(home, trait_.home, used);
    }
    for argument in predicate.types() {
        collect_from_type(&argument.value, home, used);
    }
}

fn parameter_repr_predicates<'a>(
    bump: &'a Bump,
    parameters: &'a [&'a nash_source::TypeParam<'a>],
) -> Vec<nash_ast::Pred<'a>> {
    parameters
        .iter()
        .filter_map(|parameter| {
            parameter.repr.map(|repr| nash_ast::Pred::Implied {
                trait_: types::repr_trait(repr.value).qualified(),
                args: bump.alloc_slice_copy(&[&*bump.alloc(Located::at(
                    parameter.name.region,
                    nash_ast::Type::Var(parameter.name.value),
                ))]),
            })
        })
        .collect()
}
