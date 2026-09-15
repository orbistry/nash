use super::*;

pub(super) fn callable<'a>(
    env: &Scope<'_, 'a>,
    function: &CanExpr<'a>,
) -> Option<(usize, Option<&'a [&'a str]>)> {
    match function {
        CanExpr::VarTopLevel(reference) | CanExpr::VarForeign { reference, .. } => env
            .module
            .callables
            .get(reference)
            .map(|labels| (labels.len(), Some(*labels))),
        CanExpr::VarConstructor {
            reference, index, ..
        } => env
            .module
            .ctors
            .values()
            .chain(env.module.q_ctors.values().flat_map(|ctors| ctors.values()))
            .find_map(|info| match info {
                Info::Specific(
                    _,
                    EnvCtor::Union {
                        home,
                        type_name,
                        union,
                        ..
                    },
                ) if *home == reference.home && *type_name == reference.union => union
                    .ctors
                    .iter()
                    .find(|ctor| ctor.index == *index)
                    .map(|ctor| (ctor.arguments.len(), ctor.labels)),
                Info::Specific(
                    _,
                    EnvCtor::RecordCtor {
                        home,
                        alias_name,
                        fields,
                        ..
                    },
                ) if *home == reference.home && *alias_name == reference.union => {
                    Some((fields.len(), None))
                }
                Info::Specific(_, EnvCtor::Bool { home, union, .. })
                    if *home == reference.home && union.name.value == reference.union =>
                {
                    Some((0, None))
                }
                _ => None,
            }),
        _ => None,
    }
}

pub fn reorder_arguments<'a>(
    bump: &'a Bump,
    declaration: Option<(usize, Option<&'a [&'a str]>)>,
    args: &mut [nash_ast::CallArgument<'a>],
    mut swap: impl FnMut(usize, usize),
) -> Result<(), &'a str> {
    let invalid = |reason: &str| -> &'a str { bump.alloc_str(reason) };
    if let Some((arity, _)) = declaration
        && arity != args.len()
    {
        return Err(invalid(&format!(
            "Expected {arity} arguments, but received {}.",
            args.len()
        )));
    }
    let labels = declaration.and_then(|(_, labels)| labels);
    if args.iter().all(|arg| arg.label.is_none()) {
        return Ok(());
    }
    let Some(labels) = labels else {
        return Err(invalid("This callable has no declared argument labels."));
    };
    let mut after_label = 0;
    let mut labeled = false;
    let mut seen = std::collections::BTreeSet::new();
    for argument in args.iter() {
        match argument.label {
            Some(label) => {
                labeled = true;
                if !labels.contains(&label.value) {
                    return Err(invalid(&format!(
                        "Unknown argument label `{}`.",
                        label.value
                    )));
                }
                if !seen.insert(label.value) {
                    return Err(invalid(&format!(
                        "Duplicate argument label `{}`.",
                        label.value
                    )));
                }
            }
            None if labeled => after_label += 1,
            None => {}
        }
    }
    // The pinned field-map algorithm permits one trailing positional argument.
    if after_label > 1 {
        return Err(invalid(
            "More than one positional argument follows a labeled argument.",
        ));
    }
    let mut index = 0;
    while index < args.len() {
        let Some(label) = args[index].label else {
            index += 1;
            continue;
        };
        let position = labels
            .iter()
            .position(|name| *name == label.value)
            .expect("validated label");
        if position == index {
            index += 1;
        } else {
            args.swap(index, position);
            swap(index, position);
        }
    }
    Ok(())
}

fn reorder<'a>(
    bump: &'a Bump,
    region: Region,
    declaration: Option<(usize, Option<&'a [&'a str]>)>,
    args: &mut [nash_ast::CallArgument<'a>],
) -> Result<(), Vec<Error<'a>>> {
    reorder_arguments(bump, declaration, args, |_, _| {})
        .map_err(|reason| vec![Error::InvalidCall { region, reason }])
}

fn arguments<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    args: &'a [nash_source::CallArgument<'a>],
    free: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<Vec<nash_ast::CallArgument<'a>>, Vec<Error<'a>>> {
    let mut arguments = Vec::with_capacity(args.len());
    for argument in args {
        let value = canonicalize_expr(bump, env, argument.value, free, warnings)?;
        arguments.push(nash_ast::CallArgument {
            label: argument.label,
            value,
        });
    }
    Ok(arguments)
}

fn convert_arguments<'a>(bump: &'a Bump, args: &mut [nash_ast::CallArgument<'a>]) {
    for argument in args {
        argument.label = None;
        argument.value = convert_argument(bump, argument.value);
    }
}

fn convert_argument<'a>(
    bump: &'a Bump,
    value: &'a Located<CanExpr<'a>>,
) -> &'a Located<CanExpr<'a>> {
    bump.alloc(Located::at(
        value.region,
        CanExpr::Convert {
            kind: nash_ast::ConversionKind::Ascription(nash_ast::ConversionSite::CallArgument),
            typ: bump.alloc(Located::at(value.region, CanType::Hole)),
            value,
        },
    ))
}

fn resolved_call<'a>(
    bump: &'a Bump,
    region: Region,
    function: &'a Located<CanExpr<'a>>,
    args: Vec<nash_ast::CallArgument<'a>>,
    direct_builtin: Option<nash_ast::DirectBuiltin>,
) -> CanExpr<'a> {
    if direct_builtin.is_some() {
        CanExpr::SurfaceCall {
            function,
            arguments: bump.alloc_slice_fill_iter(args),
            direct_builtin,
        }
    } else {
        let mut values: Vec<_> = args.into_iter().map(|arg| arg.value).collect();
        if values.is_empty() {
            values.push(bump.alloc(Located::at(region, CanExpr::Unit)));
        }
        CanExpr::Call {
            function,
            arguments: bump.alloc_slice_fill_iter(values),
        }
    }
}

pub(super) fn call<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<CanExpr<'a>, Vec<Error<'a>>> {
    let SourceExpr::SurfaceCall {
        function,
        arguments: args,
        direct_builtin,
    } = &expr.value
    else {
        unreachable!("surface call dispatch")
    };
    let region = expr.region;
    let direct_builtin = *direct_builtin;
    let function = canonicalize_expr(bump, env, function, free, warnings)?;
    let mut args = arguments(bump, env, args, free, warnings)?;
    if matches!(function.value, CanExpr::FieldOrModule { .. }) {
        for argument in &mut args {
            argument.value = convert_argument(bump, argument.value);
        }
        return Ok(CanExpr::SurfaceCall {
            function,
            arguments: bump.alloc_slice_fill_iter(args),
            direct_builtin,
        });
    }
    let declaration = callable(env, &function.value);
    reorder(bump, region, declaration, &mut args)?;
    convert_arguments(bump, &mut args);
    Ok(if declaration.is_some() {
        resolved_call(bump, region, function, args, direct_builtin)
    } else {
        CanExpr::SurfaceCall {
            function,
            arguments: bump.alloc_slice_fill_iter(args),
            direct_builtin,
        }
    })
}

pub(super) fn pipe<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<CanExpr<'a>, Vec<Error<'a>>> {
    let SourceExpr::Pipe {
        input,
        function,
        arguments: args,
        direct_builtin,
    } = &expr.value
    else {
        unreachable!("pipeline dispatch")
    };
    let region = expr.region;
    let args = *args;
    let direct_builtin = *direct_builtin;
    let input = canonicalize_expr(bump, env, input, free, warnings)?;
    let function = canonicalize_expr(bump, env, function, free, warnings)?;
    let Some(args) = args else {
        return Ok(CanExpr::Pipe {
            input: convert_argument(bump, input),
            function,
            arguments: None,
            direct_builtin,
        });
    };
    let mut args = arguments(bump, env, args, free, warnings)?;
    if matches!(function.value, CanExpr::FieldOrModule { .. }) {
        for argument in &mut args {
            argument.value = convert_argument(bump, argument.value);
        }
        return Ok(CanExpr::Pipe {
            input: convert_argument(bump, input),
            function,
            arguments: Some(bump.alloc_slice_fill_iter(args)),
            direct_builtin,
        });
    }
    if let Some(declaration) = callable(env, &function.value) {
        let insert = args.len() < declaration.0;
        let pipe_name = env.module.fresh_local(bump);
        let pipe_var = bump.alloc(Located::at(input.region, CanExpr::VarLocal(pipe_name)));
        if insert {
            args.insert(
                0,
                nash_ast::CallArgument {
                    label: None,
                    value: pipe_var,
                },
            );
        }
        reorder(bump, region, Some(declaration), &mut args)?;
        convert_arguments(bump, &mut args);
        let call = bump.alloc(Located::at(
            region,
            resolved_call(bump, region, function, args, direct_builtin),
        ));
        let body = if insert {
            &*call
        } else {
            let mut args = [nash_ast::CallArgument {
                label: None,
                value: pipe_var,
            }];
            convert_arguments(bump, &mut args);
            bump.alloc(Located::at(
                region,
                CanExpr::SurfaceCall {
                    function: call,
                    arguments: bump.alloc_slice_copy(&args),
                    direct_builtin: None,
                },
            ))
        };
        Ok(CanExpr::LetDestruct {
            pattern: bump.alloc(Located::at(input.region, nash_ast::Pattern::Var(pipe_name))),
            value: input,
            body,
        })
    } else {
        reorder(bump, region, None, &mut args)?;
        convert_arguments(bump, &mut args);
        Ok(CanExpr::Pipe {
            input: convert_argument(bump, input),
            function,
            arguments: Some(bump.alloc_slice_fill_iter(args)),
            direct_builtin,
        })
    }
}
