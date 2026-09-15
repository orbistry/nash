use super::*;

// Keep recursive dispatch frames independent of unrelated forms' temporaries.

pub(super) fn runnable<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::RunnableCheck {
            generator,
            argument_type,
            return_type,
            function,
            benchmark,
        } => {
            let generator = generator
                .map(|generator| canonicalize_expr(bump, env, generator, free_locals, warnings))
                .transpose()?;
            let argument_type = argument_type
                .map(|typ| {
                    let annotation = types::to_annotation(
                        bump,
                        env.module,
                        bump.alloc(nash_source::Annotation {
                            constraints: &[],
                            typ,
                        }),
                    )?;
                    crate::kinds::check_annotation(
                        bump,
                        &env.module.kinds,
                        "runnable argument",
                        annotation,
                    )
                    .map(|annotation| annotation.typ)
                })
                .transpose()?;
            let return_type = return_type
                .map(|typ| {
                    let annotation = types::to_annotation(
                        bump,
                        env.module,
                        bump.alloc(nash_source::Annotation {
                            constraints: &[],
                            typ,
                        }),
                    )?;
                    crate::kinds::check_annotation(
                        bump,
                        &env.module.kinds,
                        "runnable result",
                        annotation,
                    )
                    .map(|annotation| annotation.typ)
                })
                .transpose()?;
            CanExpr::RunnableCheck {
                generator,
                argument_type,
                return_type,
                function: canonicalize_expr(bump, env, function, free_locals, warnings)?,
                benchmark: *benchmark,
            }
        }
        _ => unreachable!("RunnableCheck dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn trace_label<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::TraceLabel {
            label,
            arguments,
            body,
            verbose_only,
        } => CanExpr::TraceLabel {
            label: canonicalize_expr(bump, env, label, free_locals, warnings)?,
            arguments: crate::accumulate::try_all_alloc(
                bump,
                arguments
                    .iter()
                    .map(|arg| canonicalize_expr(bump, env, arg, free_locals, warnings)),
            )?,
            body: canonicalize_expr(bump, env, body, free_locals, warnings)?,
            verbose_only: *verbose_only,
        },
        _ => unreachable!("TraceLabel dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn convert<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Convert { kind, typ, value } => {
            let annotation = types::to_annotation(
                bump,
                env.module,
                bump.alloc(nash_source::Annotation {
                    constraints: &[],
                    typ,
                }),
            )?;
            let annotation = crate::kinds::check_annotation(
                bump,
                &env.module.kinds,
                "Data conversion",
                annotation,
            )?;
            if !matches!(
                kind,
                nash_source::ConversionKind::Ascription(_) | nash_source::ConversionKind::Identity
            ) {
                crate::kinds::check_data_type(bump, &env.module.kinds, annotation.typ)?;
            }
            if *kind == nash_source::ConversionKind::ViewData
                && crate::kinds::repr_of(bump, &env.module.kinds, annotation.typ)
                    != Some(nash_ast::primitives::Repr::Big)
            {
                return Err(vec![Error::Unsupported {
                    feature: "raw Data boundary views require a Data-backed type",
                    region: typ.region,
                }]);
            }
            CanExpr::Convert {
                kind: *kind,
                typ: annotation.typ,
                value: canonicalize_expr(bump, env, value, free_locals, warnings)?,
            }
        }
        _ => unreachable!("Convert dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn constructor_reference<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    _free_locals: &mut FreeLocals<'a>,
    _warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::ConstructorRef {
            module,
            type_name,
            name,
        } => {
            match pattern::primitive_constructor(
                bump, env.module, region, *module, *type_name, name,
            )? {
                Some(pattern::PrimitiveConstructor::Unit) => CanExpr::Unit,
                Some(pattern::PrimitiveConstructor::Pair) => {
                    let first = "$pair.first";
                    let second = "$pair.second";
                    let parameters = bump.alloc_slice_copy(&[
                        &*bump.alloc(Located::at(region, nash_ast::Pattern::Var(first))),
                        &*bump.alloc(Located::at(region, nash_ast::Pattern::Var(second))),
                    ]);
                    let body = bump.alloc(Located::at(
                        region,
                        CanExpr::Pair {
                            first: bump.alloc(Located::at(region, CanExpr::VarLocal(first))),
                            second: bump.alloc(Located::at(region, CanExpr::VarLocal(second))),
                        },
                    ));
                    CanExpr::Function { parameters, body }
                }
                None => {
                    let ctor = pattern::resolve_constructor(
                        bump, env.module, region, *module, *type_name, name,
                    )?;
                    to_var_ctor(bump, env, name, &ctor)?
                }
            }
        }
        _ => unreachable!("ConstructorRef dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn let_value<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::LetValue {
            pattern,
            value,
            body,
        } => {
            let value = canonicalize_expr(bump, env, value, free_locals, warnings)?;
            let (pattern, bindings) = pattern::verify(
                bump,
                env.module,
                DuplicatePatternContext::LetBinding,
                pattern,
            )?;
            let inner_env = env.add_locals(&bindings)?;
            let mut body_free_locals = FreeLocals::new();
            let body = canonicalize_expr(bump, &inner_env, body, &mut body_free_locals, warnings)?;
            let uses = bindings
                .keys()
                .filter_map(|name| body_free_locals.get(name))
                .map(|usage| usage.direct + usage.delayed)
                .sum();
            let outer_free = verify_bindings(
                WarningContext::Pattern,
                &bindings,
                body_free_locals,
                warnings,
            );
            merge_free_locals(free_locals, outer_free, false);
            CanExpr::LetValue {
                pattern,
                value,
                body,
                uses,
            }
        }
        _ => unreachable!("LetValue dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn refutable_match<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Match {
            value,
            pattern,
            body,
            fallback,
            annotation,
            conversion,
        } => {
            let value = canonicalize_expr(bump, env, value, free_locals, warnings)?;
            let arm = bump.alloc(CaseArm { pattern, body });
            let branch = canonicalize_case_branch(bump, env, arm, free_locals, warnings)?;
            let fallback = canonicalize_expr(bump, env, fallback, free_locals, warnings)?;
            let annotation = annotation
                .map(|typ| {
                    let annotation = types::to_annotation(
                        bump,
                        env.module,
                        bump.alloc(nash_source::Annotation {
                            constraints: &[],
                            typ,
                        }),
                    )?;
                    crate::kinds::check_annotation(
                        bump,
                        &env.module.kinds,
                        "pattern annotation",
                        annotation,
                    )
                    .map(|annotation| annotation.typ)
                })
                .transpose()?;
            CanExpr::Match {
                value,
                pattern: branch.pattern,
                body: branch.body,
                fallback,
                annotation,
                conversion: *conversion,
            }
        }
        _ => unreachable!("Match dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn record_update<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::RecordUpdate {
            constructor,
            base,
            fields,
        } => {
            let constructor = canonicalize_expr(bump, env, constructor, free_locals, warnings)?;
            let CanExpr::VarConstructor {
                reference,
                index,
                annotation,
                ..
            } = constructor.value
            else {
                return Err(vec![Error::Unsupported {
                    region,
                    feature: "record updates require a labeled constructor",
                }]);
            };
            let declared = env
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
                    ) if *home == reference.home && *type_name == reference.union => Some(*union),
                    _ => None,
                });
            let Some(union) = declared else {
                return Err(vec![Error::Unsupported {
                    region,
                    feature: "record update constructor is not a visible data constructor",
                }]);
            };
            let [ctor] = union.ctors else {
                return Err(vec![Error::Unsupported {
                    region,
                    feature: "record updates require a single-constructor type",
                }]);
            };
            let Some(labels) = ctor.labels.filter(|_| ctor.index == index) else {
                return Err(vec![Error::Unsupported {
                    region,
                    feature: "record updates require labeled fields",
                }]);
            };
            let given = check_field_assigns(fields)?;
            for (field, value) in &given {
                if !labels.contains(field) {
                    return Err(vec![Error::LabeledCtorExtraField {
                        region: value.field.region,
                        ctor: reference.name,
                        field,
                    }]);
                }
            }
            let mut result = annotation.typ;
            match &result.value {
                CanType::Function { result: ret, .. } => result = ret,
                _ => {
                    for _ in 0..ctor.arity {
                        let CanType::Lambda { to, .. } = &result.value else {
                            return Err(vec![Error::Unsupported {
                                region,
                                feature: "invalid record constructor type",
                            }]);
                        };
                        result = to;
                    }
                }
            }
            let base = canonicalize_expr(bump, env, base, free_locals, warnings)?;
            let base = bump.alloc(Located::at(
                base.region,
                CanExpr::Convert {
                    kind: nash_ast::ConversionKind::Identity,
                    typ: result,
                    value: base,
                },
            ));
            let mut updates = Vec::with_capacity(given.len());
            for label in labels {
                if let Some(field) = given.get(label) {
                    let value = canonicalize_expr(bump, env, field.value, free_locals, warnings)?;
                    let value = bump.alloc(Located::at(
                        value.region,
                        CanExpr::Convert {
                            kind: nash_ast::ConversionKind::Ascription(
                                nash_ast::ConversionSite::RecordUpdateField,
                            ),
                            typ: bump.alloc(Located::at(value.region, CanType::Hole)),
                            value,
                        },
                    ));
                    updates.push(CanFieldUpdate {
                        field: field.field,
                        value,
                    });
                }
            }
            CanExpr::RecordUpdate {
                record: reference.name,
                base,
                fields: bump.alloc_slice_fill_iter(updates),
            }
        }
        _ => unreachable!("RecordUpdate dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn data_list<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::DataList { elements, tail } => CanExpr::DataList {
            elements: crate::accumulate::try_all_alloc(
                bump,
                elements
                    .iter()
                    .map(|element| canonicalize_expr(bump, env, element, free_locals, warnings)),
            )?,
            tail: tail
                .map(|tail| canonicalize_expr(bump, env, tail, free_locals, warnings))
                .transpose()?,
        },
        _ => unreachable!("DataList dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn negate<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Negate(inner) => {
            let trait_ = env.module.core_trait(nash_ast::primitives::num_trait());
            let annotation = env
                .module
                .method_annotation(trait_, "negate")
                .ok_or_else(|| vec![Error::NegateWithoutNum { region }])?;
            let function = bump.alloc(Located::at(
                region,
                CanExpr::VarMethod {
                    trait_,
                    method: "negate",
                    annotation,
                },
            ));
            let argument = canonicalize_expr(bump, env, inner, free_locals, warnings)?;
            CanExpr::Call {
                function,
                arguments: bump.alloc_slice_copy(&[argument]),
            }
        }
        _ => unreachable!("Negate dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn callable<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Callable {
            arity,
            labels,
            value,
        } => {
            let mut seen = std::collections::BTreeSet::new();
            for label in *labels {
                if !seen.insert(*label) {
                    return Err(vec![Error::InvalidCall {
                        region,
                        reason: bump
                            .alloc_str(&format!("Duplicate declared argument label `{label}`.")),
                    }]);
                }
            }
            CanExpr::Callable {
                arity: *arity,
                labels,
                value: canonicalize_expr(bump, env, value, free_locals, warnings)?,
            }
        }
        _ => unreachable!("Callable dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn function<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Function { parameters, body } => {
            let lambda =
                canonicalize_lambda(bump, env, parameters, body, region, free_locals, warnings)?;
            let CanExpr::Lambda { parameters, body } = &lambda.value else {
                unreachable!()
            };
            CanExpr::Function { parameters, body }
        }
        _ => unreachable!("Function dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn call<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Call {
            function,
            arguments,
        } => {
            let can_func = canonicalize_expr(bump, env, function, free_locals, warnings)?;
            let labeled = if let (
                [argument],
                CanExpr::VarConstructor {
                    reference, index, ..
                },
            ) = (*arguments, &can_func.value)
                && let SourceExpr::Record {
                    fields,
                    grouped: false,
                } = &argument.value
            {
                env.module
                    .ctors
                    .values()
                    .chain(env.module.q_ctors.values().flat_map(|ctors| ctors.values()))
                    .find_map(|info| {
                        let Info::Specific(
                            _,
                            EnvCtor::Union {
                                home,
                                type_name,
                                union,
                                ..
                            },
                        ) = info
                        else {
                            return None;
                        };
                        if *home != reference.home || *type_name != reference.union {
                            return None;
                        }
                        union
                            .ctors
                            .iter()
                            .find(|ctor| ctor.index == *index)
                            .and_then(|ctor| ctor.labels)
                            .map(|labels| (labels, *fields))
                    })
            } else {
                None
            };
            let can_args = if let Some((labels, fields)) = labeled {
                let CanExpr::VarConstructor { reference, .. } = &can_func.value else {
                    unreachable!()
                };
                let given = check_field_assigns(fields)?;
                let mut errors = Vec::new();
                for label in labels {
                    if !given.contains_key(label) {
                        errors.push(Error::LabeledCtorMissingField {
                            region,
                            ctor: reference.name,
                            field: label,
                        });
                    }
                }
                for (name, assign) in &given {
                    if !labels.contains(name) {
                        errors.push(Error::LabeledCtorExtraField {
                            region: assign.field.region,
                            ctor: reference.name,
                            field: name,
                        });
                    }
                }
                if !errors.is_empty() {
                    return Err(errors);
                }
                crate::accumulate::try_all_alloc_ref(
                    bump,
                    labels.iter().map(|label| {
                        canonicalize_expr(bump, env, given[label].value, free_locals, warnings)
                    }),
                )?
            } else {
                canonicalize_exprs(bump, env, arguments, free_locals, warnings)?
            };
            CanExpr::Call {
                function: can_func,
                arguments: can_args,
            }
        }
        _ => unreachable!("Call dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}

pub(super) fn case<'a>(
    bump: &'a Bump,
    env: &Scope<'_, 'a>,
    expr: &'a Located<SourceExpr<'a>>,
    free_locals: &mut FreeLocals<'a>,
    warnings: &mut Vec<Warning<'a>>,
) -> Result<&'a Located<CanExpr<'a>>, Vec<Error<'a>>> {
    let region = expr.region;
    let can_expr = match &expr.value {
        SourceExpr::Case { scrutinee, arms } => {
            let can_scrutinee = canonicalize_expr(bump, env, scrutinee, free_locals, warnings)?;
            let can_branches = canonicalize_case_branches(bump, env, arms, free_locals, warnings)?;
            CanExpr::Case {
                scrutinee: can_scrutinee,
                branches: can_branches,
            }
        }
        _ => unreachable!("Case dispatch invariant"),
    };
    Ok(bump.alloc(Located::at(region, can_expr)))
}
