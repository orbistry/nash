use super::*;

impl<'a> Solver<'a, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_scoped_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        match &expr.value {
            CanExpr::TypeScope { value } => {
                self.infer_annotation_scope(uf, env, rank, state, rtv, value, expected)
            }
            CanExpr::ModuleConstantCheck { value } => {
                self.infer_module_constant(uf, env, rank, state, rtv, value, region, expected)
            }
            CanExpr::RunnableCheck {
                generator,
                argument_type,
                return_type,
                function,
                benchmark,
            } => self.infer_runnable(
                uf,
                env,
                rank,
                state,
                rtv,
                *generator,
                *argument_type,
                *return_type,
                function,
                *benchmark,
                region,
                expected,
            ),
            CanExpr::Constant(value) => {
                let name = value.builtin_name();
                let actual = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), name, Vec::new()),
                );
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Foreign(name),
                    actual,
                    expected,
                )
            }
            _ => unreachable!("scoped inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_operations_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        let node = NodeId::expr(expr);
        match &expr.value {
            CanExpr::TupleIndex { tuple, index } => {
                let input = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    tuple,
                    Expected::NoExpectation(input),
                );
                let mut allocated = Vec::new();
                let subject =
                    crate::representation::subject(uf, &self.tables.kinds, input, &mut allocated);
                let selected = match uf.get(subject).content.clone() {
                    Content::Structure(FlatType::App1(home, "data_pair", args))
                        if home == nash_ast::primitives::builtin_home() =>
                    {
                        args.get(*index).copied()
                    }
                    Content::Structure(FlatType::App1(home, "data_tuple", args))
                        if home == nash_ast::primitives::builtin_home() && args.len() == 1 =>
                    {
                        let tuple = crate::representation::subject(
                            uf,
                            &self.tables.kinds,
                            args[0],
                            &mut allocated,
                        );
                        match uf.get(tuple).content.clone() {
                            Content::Structure(FlatType::Tuple1(first, second, rest)) => {
                                [first, second].into_iter().chain(rest).nth(*index)
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                self.introduce(uf, rank, &allocated);
                let output = selected.unwrap_or_else(|| {
                    state.errors.push(Error::InvalidTupleIndex {
                        region,
                        index: *index,
                        typ: to_error_type(self.bump, uf, input),
                    });
                    self.register(uf, rank, Content::Error)
                });
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Foreign("tuple index"),
                    output,
                    expected,
                )
            }
            CanExpr::LetValue {
                pattern,
                value,
                body,
                ..
            } => {
                let input = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    value,
                    Expected::NoExpectation(input),
                );
                let scope = self.infer_patterns(
                    uf,
                    env,
                    rank,
                    state,
                    &[(*pattern, PExpected::NoExpectation(input))],
                );
                state = self.infer_expr(uf, &scope.env, rank, scope.state, rtv, body, expected);
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::Match {
                value,
                pattern,
                body,
                fallback,
                annotation,
                conversion,
            } => {
                let scoped = matches!(
                    conversion,
                    Some(
                        nash_ast::ConversionSite::CastTest | nash_ast::ConversionSite::CastPattern
                    )
                );
                if scoped {
                    self.push_annotation_scope();
                }
                let input = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    value,
                    Expected::NoExpectation(input),
                );
                let mut allocated = Vec::new();
                let subject =
                    crate::representation::subject(uf, &self.tables.kinds, input, &mut allocated);
                let source_is_data = matches!(&uf.get(subject).content,
                    Content::Structure(FlatType::App1(home, "Data", args))
                        if *home == nash_ast::primitives::builtin_home() && args.is_empty());
                self.introduce(uf, rank, &allocated);
                let target = if let Some(typ) = annotation {
                    self.conversion_annotation(uf, rank, rtv, typ, true)
                } else if source_is_data && conversion.is_some() {
                    self.fresh(uf, rank)
                } else {
                    input
                };
                let scope = self.infer_patterns(
                    uf,
                    env,
                    rank,
                    state,
                    &[(
                        *pattern,
                        PExpected::FromContext(region, PContext::CaseMatch(0), target),
                    )],
                );
                state = scope.state;
                if source_is_data
                    && matches!(
                        conversion,
                        Some(
                            nash_ast::ConversionSite::ExpectPattern
                                | nash_ast::ConversionSite::CastPattern
                        )
                    )
                    && (matches!(
                        pattern.value,
                        nash_ast::Pattern::Var(_) | nash_ast::Pattern::Anything
                    ) || !self.conversion_type_known(uf, rank, target, true))
                {
                    state.errors.push(Error::InvalidDataCast {
                        region,
                        reason: "A Data pattern without an annotation must determine a complete type before its body.",
                        typ: to_error_type(self.bump, uf, target),
                    });
                }
                if let Some(site) = conversion {
                    self.pending_conversions.push(PendingConversion {
                        node,
                        rank,
                        request: ConversionRequest {
                            site: *site,
                            source: input,
                            target,
                            region: value.region,
                        },
                    });
                }
                let result = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    &scope.env,
                    rank,
                    state,
                    rtv,
                    body,
                    Expected::FromContext(region, Context::CaseBranch(0, None), result),
                );
                state = self.close_locals(uf, state, scope.locals);
                if scoped {
                    self.pop_annotation_scope();
                }
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    fallback,
                    Expected::FromContext(
                        region,
                        Context::CaseBranch(1, Some(body.region)),
                        result,
                    ),
                );
                self.equal(uf, rank, state, region, Category::Case, result, expected)
            }
            CanExpr::Equal { left, right, .. } => {
                let operand = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    left,
                    Expected::NoExpectation(operand),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    right,
                    Expected::FromContext(region, Context::OpRight("=="), operand),
                );
                self.pending_serialisable.push(PendingSerialisable {
                    rank,
                    variable: operand,
                    region,
                    top_level: false,
                    purpose: SerialisablePurpose::Comparison,
                });
                let boolean = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "bool", Vec::new()),
                );
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Foreign("equality"),
                    boolean,
                    expected,
                )
            }
            CanExpr::Format { value } => {
                let input = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    value,
                    Expected::NoExpectation(input),
                );
                let string = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "string", Vec::new()),
                );
                // The pinned trace operation first unifies with String. An
                // unconstrained argument therefore becomes String, not Data.
                let mut allocated = Vec::new();
                let subject =
                    crate::representation::subject(uf, &self.tables.kinds, input, &mut allocated);
                self.introduce(uf, rank, &allocated);
                if matches!(uf.get(subject).content, Content::FlexVar(_)) {
                    state = self.equal(
                        uf,
                        rank,
                        state,
                        value.region,
                        Category::String,
                        input,
                        Expected::NoExpectation(string),
                    );
                }
                if matches!(uf.get(subject).content, Content::RigidVar(_)) {
                    state.errors.push(Error::IllegalTraceArgument {
                        region: value.region,
                        typ: to_error_type(self.bump, uf, input),
                    });
                }
                self.pending_serialisable.push(PendingSerialisable {
                    rank,
                    variable: input,
                    region: value.region,
                    top_level: false,
                    purpose: SerialisablePurpose::Format,
                });
                self.equal(uf, rank, state, region, Category::String, string, expected)
            }
            CanExpr::TraceLabel {
                label,
                arguments,
                body,
                ..
            } => {
                let string = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "string", Vec::new()),
                );
                for argument in *arguments {
                    state = self.infer_annotation_scope(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        argument,
                        Expected::NoExpectation(string),
                    );
                }
                state = self.infer_expr(uf, env, rank, state, rtv, body, expected);
                self.infer_annotation_scope(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    label,
                    Expected::NoExpectation(string),
                )
            }
            _ => unreachable!("operations inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_conversion_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        let node = NodeId::expr(expr);
        match &expr.value {
            CanExpr::Convert { kind, typ, value } => {
                let target = self.conversion_annotation(
                    uf,
                    rank,
                    rtv,
                    typ,
                    matches!(kind, nash_ast::ConversionKind::Ascription(_)),
                );
                if let nash_ast::ConversionKind::Ascription(site) = kind {
                    let contextual = matches!(
                        site,
                        nash_ast::ConversionSite::CallArgument
                            | nash_ast::ConversionSite::RecordUpdateField
                    );
                    if contextual {
                        state = self.equal(
                            uf,
                            rank,
                            state,
                            region,
                            Category::Foreign("argument annotation"),
                            target,
                            expected,
                        );
                    }
                    let mut allocated = Vec::new();
                    let subject = crate::representation::subject(
                        uf,
                        &self.tables.kinds,
                        target,
                        &mut allocated,
                    );
                    let known_non_data = match &uf.get(subject).content {
                        Content::Structure(FlatType::App1(home, name, _)) => {
                            *home != nash_ast::primitives::builtin_home() || *name != "Data"
                        }
                        Content::Structure(
                            FlatType::Fun1(..) | FlatType::Function1(..) | FlatType::Tuple1(..),
                        )
                        | Content::Alias { .. } => true,
                        _ => false,
                    };
                    self.introduce(uf, rank, &allocated);
                    let input = if matches!(site, nash_ast::ConversionSite::LambdaSignature(_)) {
                        // Parameter annotations and the surrounding call constrain
                        // the body before trace rendering or tuple selection.
                        // The written result annotation remains a separate policy:
                        // it must not turn an Int-returning lambda into Data.
                        let input = match uf.get(subject).content.clone() {
                            Content::Structure(FlatType::Function1(arguments, _)) => {
                                let result = self.fresh(uf, rank);
                                self.structure(uf, rank, FlatType::Function1(arguments, result))
                            }
                            _ => self.fresh(uf, rank),
                        };
                        state =
                            self.equal(uf, rank, state, region, Category::Lambda, input, expected);
                        input
                    } else if contextual && known_non_data {
                        target
                    } else {
                        self.fresh(uf, rank)
                    };
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        value,
                        Expected::NoExpectation(input),
                    );
                    let output = if matches!(
                        site,
                        nash_ast::ConversionSite::LambdaResult
                            | nash_ast::ConversionSite::LambdaSignature(_)
                    ) {
                        input
                    } else {
                        target
                    };
                    self.pending_conversions.push(PendingConversion {
                        node,
                        rank,
                        request: ConversionRequest {
                            site: *site,
                            source: input,
                            target,
                            region: value.region,
                        },
                    });
                    return self.equal(
                        uf,
                        rank,
                        state,
                        region,
                        Category::Foreign("type ascription"),
                        output,
                        expected,
                    );
                }
                let data = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "Data", Vec::new()),
                );
                let input = match kind {
                    nash_ast::ConversionKind::ToData => {
                        state = self.equal(
                            uf,
                            rank,
                            state,
                            typ.region,
                            Category::Foreign("Data conversion"),
                            data,
                            Expected::NoExpectation(target),
                        );
                        self.fresh(uf, rank)
                    }
                    nash_ast::ConversionKind::Identity => target,
                    _ => data,
                };
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    value,
                    Expected::NoExpectation(input),
                );
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Foreign("Data conversion"),
                    target,
                    expected,
                )
            }
            _ => unreachable!("conversion inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_collections_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        match &expr.value {
            CanExpr::Pair { first, second } => {
                let first_var = self.fresh(uf, rank);
                let second_var = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    first,
                    Expected::NoExpectation(first_var),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    second,
                    Expected::NoExpectation(second_var),
                );
                self.queue_serialisable(rank, first_var, region, false);
                self.queue_serialisable(rank, second_var, region, false);
                let pair = self.structure(
                    uf,
                    rank,
                    FlatType::App1(
                        nash_ast::primitives::builtin_home(),
                        "data_pair",
                        vec![first_var, second_var],
                    ),
                );
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Foreign("data_pair"),
                    pair,
                    expected,
                )
            }
            CanExpr::DataList { elements, tail } => {
                let element = self.fresh(uf, rank);
                for (index, entry) in elements.iter().enumerate() {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        entry,
                        Expected::FromContext(
                            region,
                            Context::ListEntry(
                                index,
                                index.checked_sub(1).map(|i| elements[i].region),
                            ),
                            element,
                        ),
                    );
                }
                let list = self.structure(
                    uf,
                    rank,
                    FlatType::App1(
                        nash_ast::primitives::builtin_home(),
                        "data_list",
                        vec![element],
                    ),
                );
                if let Some(tail) = tail {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        tail,
                        Expected::NoExpectation(list),
                    );
                }
                self.queue_serialisable(rank, element, region, false);
                self.equal(uf, rank, state, region, Category::List, list, expected)
            }
            _ => unreachable!("collections inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_values_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        let node = NodeId::expr(expr);
        match &expr.value {
            CanExpr::VarLocal(name) => {
                self.local(uf, env, rank, state, region, node, name, expected)
            }
            CanExpr::VarTopLevel(reference) => {
                self.local(uf, env, rank, state, region, node, reference.name, expected)
            }
            CanExpr::VarForeign {
                reference,
                annotation,
            } => self.foreign(
                uf,
                rank,
                state,
                region,
                node,
                reference.name,
                annotation,
                expected,
            ),
            CanExpr::VarConstructor {
                reference,
                annotation,
                ..
            } => self.foreign(
                uf,
                rank,
                state,
                region,
                node,
                reference.name,
                annotation,
                expected,
            ),
            CanExpr::VarMethod {
                method, annotation, ..
            } => self.foreign(uf, rank, state, region, node, method, annotation, expected),
            CanExpr::VarOperator {
                symbol, annotation, ..
            } => self.foreign(uf, rank, state, region, node, symbol, annotation, expected),
            CanExpr::Str(_) | CanExpr::Bytes(_) | CanExpr::Int(_) => {
                let (name, trait_) = match &expr.value {
                    CanExpr::Str(_) => ("fromString", "FromString"),
                    CanExpr::Bytes(_) => ("fromBytes", "FromBytes"),
                    _ => ("fromInt", "FromInt"),
                };
                let annotation = type_::literal_annotation(
                    self.bump,
                    &[self.tables.core_trait(type_::literal_trait(trait_))],
                );
                self.foreign(uf, rank, state, region, node, name, annotation, expected)
            }
            CanExpr::List(entries) => {
                let element = self.fresh(uf, rank);
                for (index, entry) in entries.iter().enumerate() {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        entry,
                        Expected::FromContext(
                            region,
                            Context::ListEntry(
                                index,
                                index.checked_sub(1).map(|i| entries[i].region),
                            ),
                            element,
                        ),
                    );
                }
                let list = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "list", vec![element]),
                );
                self.equal(uf, rank, state, region, Category::List, list, expected)
            }
            CanExpr::Binop {
                symbol,
                annotation,
                left,
                right,
                ..
            } => {
                let left_var = self.fresh(uf, rank);
                let right_var = self.fresh(uf, rank);
                let result = self.fresh(uf, rank);
                let tail = self.structure(uf, rank, FlatType::Fun1(right_var, result));
                let function = self.structure(uf, rank, FlatType::Fun1(left_var, tail));
                state = self.foreign(
                    uf,
                    rank,
                    state,
                    region,
                    node,
                    symbol,
                    annotation,
                    Expected::NoExpectation(function),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    left,
                    Expected::FromContext(region, Context::OpLeft(symbol), left_var),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    right,
                    Expected::FromContext(region, Context::OpRight(symbol), right_var),
                );
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::CallResult(MaybeName::OpName(symbol)),
                    result,
                    expected,
                )
            }
            _ => unreachable!("values inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_functions_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        match &expr.value {
            CanExpr::Callable { value, .. } => {
                state = self.infer_expr(uf, env, rank, state, rtv, value, expected);
                let result = match expected {
                    Expected::NoExpectation(variable)
                    | Expected::FromContext(_, _, variable)
                    | Expected::FromAnnotation(_, _, _, _, variable) => variable,
                };
                self.queue_serialisable(rank, result, value.region, true);
                state
            }
            CanExpr::Function { parameters, body } => {
                let args: Vec<_> = parameters.iter().map(|_| self.fresh(uf, rank)).collect();
                let result = self.fresh(uf, rank);
                let function = self.structure(uf, rank, FlatType::Function1(args.clone(), result));
                state = self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Lambda,
                    function,
                    expected,
                );
                let patterns: Vec<_> = parameters
                    .iter()
                    .zip(&args)
                    .map(|(pattern, var)| (*pattern, PExpected::NoExpectation(*var)))
                    .collect();
                let scope = self.infer_patterns(uf, env, rank, state, &patterns);
                state = self.infer_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    Expected::NoExpectation(result),
                );
                self.queue_serialisable(rank, result, body.region, true);
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::SurfaceCall {
                function,
                arguments,
                ..
            } => self.infer_surface_call(
                uf, env, rank, state, rtv, expr, function, arguments, expected,
            ),
            CanExpr::Pipe {
                input,
                function,
                arguments,
                ..
            } => self.infer_pipe(
                uf, env, rank, state, rtv, expr, input, function, *arguments, expected,
            ),
            CanExpr::Lambda { parameters, body } => {
                let args: Vec<_> = parameters.iter().map(|_| self.fresh(uf, rank)).collect();
                let result = self.fresh(uf, rank);
                let patterns: Vec<_> = parameters
                    .iter()
                    .zip(&args)
                    .map(|(pattern, var)| (*pattern, PExpected::NoExpectation(*var)))
                    .collect();
                let scope = self.infer_patterns(uf, env, rank, state, &patterns);
                state = self.infer_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    Expected::NoExpectation(result),
                );
                state = self.close_locals(uf, state, scope.locals);
                let function = args.into_iter().rev().fold(result, |tail, arg| {
                    self.structure(uf, rank, FlatType::Fun1(arg, tail))
                });
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Lambda,
                    function,
                    expected,
                )
            }
            CanExpr::Call {
                function,
                arguments,
            } => {
                let name = direct_get_name(function);
                let function_var = self.fresh(uf, rank);
                let result = self.fresh(uf, rank);
                let args: Vec<_> = arguments.iter().map(|_| self.fresh(uf, rank)).collect();
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    function,
                    Expected::NoExpectation(function_var),
                );
                let arity_type = args.iter().rev().fold(result, |tail, arg| {
                    self.structure(uf, rank, FlatType::Fun1(*arg, tail))
                });
                state = self.equal(
                    uf,
                    rank,
                    state,
                    function.region,
                    Category::CallResult(name),
                    function_var,
                    Expected::FromContext(
                        region,
                        Context::CallArity(name, arguments.len()),
                        arity_type,
                    ),
                );
                for (index, (arg, var)) in arguments.iter().zip(args).enumerate() {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        arg,
                        Expected::FromContext(region, Context::CallArg(name, index), var),
                    );
                }
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::CallResult(name),
                    result,
                    expected,
                )
            }
            _ => unreachable!("functions inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_branches_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        match &expr.value {
            CanExpr::If {
                branches,
                final_else,
            } => {
                // All conditions precede all branch bodies, including else-if.
                let branch_var = self.fresh(uf, rank);
                for branch in *branches {
                    let boolean = self.structure(
                        uf,
                        rank,
                        FlatType::App1(nash_ast::primitives::builtin_home(), "bool", vec![]),
                    );
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        branch.condition,
                        Expected::FromContext(region, Context::IfCondition, boolean),
                    );
                }
                let mut previous_branch = None;
                for (index, branch) in branches
                    .iter()
                    .map(|branch| branch.then_branch)
                    .chain(std::iter::once(*final_else))
                    .enumerate()
                {
                    let branch_expected = Expected::FromContext(
                        region,
                        Context::IfBranch(index, previous_branch),
                        branch_var,
                    );
                    state = self.infer_expr(uf, env, rank, state, rtv, branch, branch_expected);
                    previous_branch = Some(branch.region);
                }
                self.equal(uf, rank, state, region, Category::If, branch_var, expected)
            }
            CanExpr::Case {
                scrutinee,
                branches,
            } => {
                let matched = self.fresh(uf, rank);
                let branch_var = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    scrutinee,
                    Expected::NoExpectation(matched),
                );
                for (index, branch) in branches.iter().enumerate() {
                    let scope = self.infer_patterns(
                        uf,
                        env,
                        rank,
                        state,
                        &[(
                            branch.pattern,
                            PExpected::FromContext(region, PContext::CaseMatch(index), matched),
                        )],
                    );
                    let branch_expected = Expected::FromContext(
                        region,
                        Context::CaseBranch(
                            index,
                            index.checked_sub(1).map(|i| branches[i].body.region),
                        ),
                        branch_var,
                    );
                    state = self.infer_expr(
                        uf,
                        &scope.env,
                        rank,
                        scope.state,
                        rtv,
                        branch.body,
                        branch_expected,
                    );
                    state = self.close_locals(uf, state, scope.locals);
                }
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Case,
                    branch_var,
                    expected,
                )
            }
            CanExpr::Let { definition, body } => {
                let scope = self.infer_definition(uf, env, rank, state, rtv, definition, true);
                state = self.infer_expr(uf, &scope.env, rank, scope.state, rtv, body, expected);
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::LetRec { definitions, body } => {
                let scope = self.infer_group(uf, env, rank, state, rtv, definitions);
                state = self.infer_expr(uf, &scope.env, rank, scope.state, rtv, body, expected);
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::LetDestruct {
                pattern,
                value,
                body,
            } => {
                let scope = self.infer_destruct(uf, env, rank, state, rtv, region, pattern, value);
                state = self.infer_expr(uf, &scope.env, rank, scope.state, rtv, body, expected);
                self.close_locals(uf, state, scope.locals)
            }
            _ => unreachable!("branches inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_records_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        let node = NodeId::expr(expr);
        match &expr.value {
            CanExpr::Accessor(field) => {
                let record = self.fresh(uf, rank);
                let value = self.fresh(uf, rank);
                state = self.field(
                    uf,
                    rank,
                    state,
                    DeferredField {
                        region,
                        context: type_::FieldContext::Accessor,
                        record,
                        field: Some((field, value)),
                        allow_union_update: false,
                    },
                );
                let function = self.structure(uf, rank, FlatType::Fun1(record, value));
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Accessor(field),
                    function,
                    expected,
                )
            }
            CanExpr::FieldOrModule {
                record,
                field,
                module,
                ..
            } => {
                let record_var = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    record,
                    Expected::NoExpectation(record_var),
                );
                let value = self.fresh(uf, rank);
                let query = DeferredField {
                    region,
                    context: type_::FieldContext::Access {
                        record_region: record.region,
                        maybe_name: direct_get_access_name(record),
                    },
                    record: record_var,
                    field: Some((field.value, value)),
                    allow_union_update: false,
                };
                let mut field_errors = Vec::new();
                let resolved = self.resolve_field(uf, rank, query, &mut field_errors);
                let use_field = module.is_none() || (resolved && field_errors.is_empty());
                self.field_selections.insert(node, use_field);
                if use_field {
                    state.errors.extend(field_errors);
                    if !resolved {
                        self.fields.push(query);
                    }
                    self.equal(
                        uf,
                        rank,
                        state,
                        region,
                        Category::Access(field.value),
                        value,
                        expected,
                    )
                } else {
                    self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        module.expect("a failed field access has a module candidate"),
                        expected,
                    )
                }
            }
            CanExpr::Access { record, field } => {
                let record_var = self.fresh(uf, rank);
                let value = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    record,
                    Expected::NoExpectation(record_var),
                );
                state = self.field(
                    uf,
                    rank,
                    state,
                    DeferredField {
                        region,
                        context: type_::FieldContext::Access {
                            record_region: record.region,
                            maybe_name: direct_get_access_name(record),
                        },
                        record: record_var,
                        field: Some((field.value, value)),
                        allow_union_update: false,
                    },
                );
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Access(field.value),
                    value,
                    expected,
                )
            }
            CanExpr::Update {
                record,
                base,
                fields,
            }
            | CanExpr::RecordUpdate {
                record,
                base,
                fields,
            } => {
                let record_var = self.fresh(uf, rank);
                let values: Vec<_> = fields.iter().map(|_| self.fresh(uf, rank)).collect();
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    base,
                    Expected::FromContext(
                        region,
                        Context::RecordUpdateKeys(record, fields),
                        record_var,
                    ),
                );
                if fields.is_empty() {
                    state = self.field(
                        uf,
                        rank,
                        state,
                        DeferredField {
                            region,
                            context: type_::FieldContext::Update { record },
                            record: record_var,
                            field: None,
                            allow_union_update: matches!(expr.value, CanExpr::RecordUpdate { .. }),
                        },
                    );
                }
                for (field, var) in fields.iter().zip(values) {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        field.value,
                        Expected::FromContext(
                            region,
                            Context::RecordUpdateValue(field.field.value),
                            var,
                        ),
                    );
                    state = self.field(
                        uf,
                        rank,
                        state,
                        DeferredField {
                            region: field.field.region,
                            context: type_::FieldContext::Update { record },
                            record: record_var,
                            field: Some((field.field.value, var)),
                            allow_union_update: matches!(expr.value, CanExpr::RecordUpdate { .. }),
                        },
                    );
                }
                self.equal(
                    uf,
                    rank,
                    state,
                    region,
                    Category::Record,
                    record_var,
                    expected,
                )
            }
            CanExpr::Record {
                alias,
                annotation,
                fields,
            } => {
                let args: Vec<_> = fields.iter().map(|_| self.fresh(uf, rank)).collect();
                let result = self.fresh(uf, rank);
                for (field, var) in fields.iter().zip(&args) {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        field.value,
                        Expected::FromContext(
                            region,
                            Context::RecordField(alias.name, field.field.value),
                            *var,
                        ),
                    );
                }
                let constructor = args.into_iter().rev().fold(result, |tail, arg| {
                    self.structure(uf, rank, FlatType::Fun1(arg, tail))
                });
                state = self.foreign(
                    uf,
                    rank,
                    state,
                    region,
                    node,
                    alias.name,
                    annotation,
                    Expected::NoExpectation(constructor),
                );
                self.equal(uf, rank, state, region, Category::Record, result, expected)
            }
            _ => unreachable!("records inference dispatch"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_effects_form(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let region = expr.region;
        match &expr.value {
            CanExpr::Assert(condition) => {
                let boolean = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "bool", vec![]),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    condition,
                    Expected::NoExpectation(boolean),
                );
                let unit = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "unit", vec![]),
                );
                self.equal(uf, rank, state, region, Category::Unit, unit, expected)
            }
            CanExpr::Fail(message) | CanExpr::Todo(message) => {
                if let Some(message) = message {
                    let string = self.structure(
                        uf,
                        rank,
                        FlatType::App1(nash_ast::primitives::builtin_home(), "string", vec![]),
                    );
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        message,
                        Expected::NoExpectation(string),
                    );
                }
                state
            }
            CanExpr::Trace { message, body } => {
                let string = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "string", vec![]),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    message,
                    Expected::NoExpectation(string),
                );
                self.infer_expr(uf, env, rank, state, rtv, body, expected)
            }
            CanExpr::Comptime(inner) => self.infer_expr(uf, env, rank, state, rtv, inner, expected),
            CanExpr::Unit => {
                let unit = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "unit", vec![]),
                );
                self.equal(uf, rank, state, region, Category::Unit, unit, expected)
            }
            CanExpr::DataTuple {
                first,
                second,
                rest,
            }
            | CanExpr::Tuple {
                first,
                second,
                rest,
            } => {
                let first_var = self.fresh(uf, rank);
                let second_var = self.fresh(uf, rank);
                let tail: Vec<_> = rest.iter().map(|_| self.fresh(uf, rank)).collect();
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    first,
                    Expected::NoExpectation(first_var),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    second,
                    Expected::NoExpectation(second_var),
                );
                for (item, var) in rest.iter().zip(&tail) {
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        item,
                        Expected::NoExpectation(*var),
                    );
                }
                let tuple = self.structure(uf, rank, FlatType::Tuple1(first_var, second_var, tail));
                let tuple = if matches!(&expr.value, CanExpr::DataTuple { .. }) {
                    self.queue_serialisable(rank, tuple, region, false);
                    self.structure(
                        uf,
                        rank,
                        FlatType::App1(
                            nash_ast::primitives::builtin_home(),
                            "data_tuple",
                            vec![tuple],
                        ),
                    )
                } else {
                    tuple
                };
                self.equal(uf, rank, state, region, Category::Tuple, tuple, expected)
            }
            _ => unreachable!("effects inference dispatch"),
        }
    }
}
