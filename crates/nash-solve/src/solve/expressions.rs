use super::*;
use nash_ast::{Expr as CanExpr, NodeId};
use nash_constrain::error::{Context, MaybeName, PContext, SubContext};

impl<'a> Solver<'a, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_expr(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let variable = match expected {
            Expected::NoExpectation(var)
            | Expected::FromContext(_, _, var)
            | Expected::FromAnnotation(_, _, _, _, var) => var,
        };
        self.expr_types.push(NodeTypeRecord {
            node: NodeId::expr(expr),
            variable,
            owner: self.owners.last().copied(),
        });
        // These are exactly the old `exists` boundaries. Their wanted flush
        // must remain even though variables are now introduced eagerly.
        let existential = matches!(
            &expr.value,
            CanExpr::List(_)
                | CanExpr::Binop { .. }
                | CanExpr::Lambda { .. }
                | CanExpr::Call { .. }
                | CanExpr::Case { .. }
                | CanExpr::Accessor(_)
                | CanExpr::Access { .. }
                | CanExpr::Update { .. }
                | CanExpr::Record { .. }
                | CanExpr::Tuple { .. }
                | CanExpr::If { .. }
        );
        let wanted_start = self.wanted.len();
        let mut state = self.infer_expr_inner(uf, env, rank, state, rtv, expr, expected);
        if existential {
            self.retry_fields(uf, rank, &mut state.errors);
            state = self.resolve_wanted(uf, rank, state, wanted_start, None, false);
        }
        state
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_expr_inner(
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
                let annotation =
                    type_::literal_annotation(self.bump, &[type_::literal_trait(trait_)]);
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
            CanExpr::Unit => {
                let unit = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "unit", vec![]),
                );
                self.equal(uf, rank, state, region, Category::Unit, unit, expected)
            }
            CanExpr::Tuple {
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
                self.equal(uf, rank, state, region, Category::Tuple, tuple, expected)
            }
        }
    }
}

fn direct_get_name<'a>(expr: &Located<CanExpr<'a>>) -> MaybeName<'a> {
    match &expr.value {
        CanExpr::VarMethod { method, .. } => MaybeName::FuncName(method),
        CanExpr::VarLocal(name) => MaybeName::FuncName(name),
        CanExpr::VarTopLevel(reference) | CanExpr::VarForeign { reference, .. } => {
            MaybeName::FuncName(reference.name)
        }
        CanExpr::VarConstructor { reference, .. } => MaybeName::CtorName(reference.name),
        CanExpr::VarOperator { symbol, .. } => MaybeName::OpName(symbol),
        _ => MaybeName::NoName,
    }
}

fn direct_get_access_name<'a>(expr: &Located<CanExpr<'a>>) -> Option<&'a str> {
    match &expr.value {
        CanExpr::VarLocal(name) => Some(name),
        CanExpr::VarTopLevel(reference) | CanExpr::VarForeign { reference, .. } => {
            Some(reference.name)
        }
        _ => None,
    }
}

impl<'a> Solver<'a, '_> {
    /// An annotation is canonical syntax, not a shared unification structure.
    /// Reinstantiate it at each branch equation so one failed branch cannot
    /// poison another branch's structural expectation. Its rigid variables
    /// still come from the same lexical substitution.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_annotated_expr(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, &'a Located<CanType<'a>>>,
    ) -> State<'a> {
        let Expected::FromAnnotation(name, annotation_region, arity, _, typ) = expected else {
            unreachable!("annotated expression has its canonical expectation")
        };
        // These forms recurse directly through canonical expectations instead
        // of entering infer_expr. Retain their original arena node identities.
        if matches!(
            expr.value,
            CanExpr::If { .. }
                | CanExpr::Case { .. }
                | CanExpr::Let { .. }
                | CanExpr::LetRec { .. }
                | CanExpr::LetDestruct { .. }
        ) {
            let variable = self.src_type_to_var(uf, rank, rtv, typ);
            self.expr_types.push(NodeTypeRecord {
                node: NodeId::expr(expr),
                variable,
                owner: self.owners.last().copied(),
            });
        }
        let region = expr.region;
        match &expr.value {
            CanExpr::If {
                branches,
                final_else,
            } => {
                for branch in *branches {
                    let boolean = self.structure(
                        uf,
                        rank,
                        FlatType::App1(nash_ast::primitives::builtin_home(), "bool", Vec::new()),
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
                for (index, branch) in branches
                    .iter()
                    .map(|branch| branch.then_branch)
                    .chain(std::iter::once(*final_else))
                    .enumerate()
                {
                    state = self.infer_annotated_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        branch,
                        Expected::FromAnnotation(
                            name,
                            annotation_region,
                            arity,
                            SubContext::TypedIfBranch(index),
                            typ,
                        ),
                    );
                }
                state
            }
            CanExpr::Case {
                scrutinee,
                branches,
            } => {
                let start = self.wanted.len();
                let matched = self.fresh(uf, rank);
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
                    state = self.infer_annotated_expr(
                        uf,
                        &scope.env,
                        rank,
                        scope.state,
                        rtv,
                        branch.body,
                        Expected::FromAnnotation(
                            name,
                            annotation_region,
                            arity,
                            SubContext::TypedCaseBranch(index),
                            typ,
                        ),
                    );
                    state = self.close_locals(uf, state, scope.locals);
                }
                self.retry_fields(uf, rank, &mut state.errors);
                self.resolve_wanted(uf, rank, state, start, None, false)
            }
            CanExpr::Let { definition, body } => {
                let scope = self.infer_definition(uf, env, rank, state, rtv, definition, true);
                state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::LetRec { definitions, body } => {
                let scope = self.infer_group(uf, env, rank, state, rtv, definitions);
                state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::LetDestruct {
                pattern,
                value,
                body,
            } => {
                let scope = self.infer_destruct(uf, env, rank, state, rtv, region, pattern, value);
                state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            _ => {
                let variable = self.src_type_to_var(uf, rank, rtv, typ);
                self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    expr,
                    expected.type_replace(variable),
                )
            }
        }
    }
}
