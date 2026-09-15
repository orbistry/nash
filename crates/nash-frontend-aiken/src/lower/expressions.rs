use super::{Expr, Lower, Result, n, profile};
use aiken_lang::{
    ast::{AssignmentKind, BinOp, LogicalOpChainKind, Span, UnOp},
    expr::UntypedExpr as A,
};

impl<'a> Lower<'a, '_> {
    pub(super) fn expr(&mut self, expr: &A) -> Result<Expr<'a>> {
        let span = expr.location();
        Ok(match expr {
            A::UInt { value, .. } => self.at(span, n::Expr::Constant(self.integer(span, value)?)),
            A::String { value, .. } => {
                self.at(span, n::Expr::Constant(n::Constant::Str(self.text(value))))
            }
            A::ByteArray { bytes, .. } => {
                let bytes = bytes.iter().map(|(byte, _)| *byte).collect::<Vec<_>>();
                self.at(
                    span,
                    n::Expr::Constant(n::Constant::Bytes(self.slice(&bytes))),
                )
            }
            A::Var { name, .. } => self.value_name(span, name),
            A::Sequence { expressions, .. } => {
                let scope = self.locals.len();
                let result = self.sequence(span, expressions);
                self.locals.truncate(scope);
                self.at(span, n::Expr::TypeScope { value: result? })
            }
            A::Fn {
                arguments,
                body,
                return_annotation,
                ..
            } => {
                let (parameters, body, annotation) =
                    self.function(span, arguments, return_annotation.as_ref(), body)?;
                let parameters = if arguments.is_empty() {
                    &[][..]
                } else {
                    parameters
                };
                let lambda = self.at(span, n::Expr::Function { parameters, body });
                self.conversion_ascribe(
                    span,
                    lambda,
                    annotation.typ,
                    n::ConversionSite::LambdaSignature(parameters.len()),
                )
            }
            A::List { elements, tail, .. } => {
                let elements = elements
                    .iter()
                    .map(|item| self.expr(item))
                    .collect::<Result<Vec<_>>>()?;
                let tail = tail.as_deref().map(|tail| self.expr(tail)).transpose()?;
                self.at(
                    span,
                    n::Expr::DataList {
                        elements: self.slice(&elements),
                        tail,
                    },
                )
            }
            A::Call { fun, arguments, .. } => {
                let direct_builtin = self.direct_builtin(fun);
                let function = self.expr(fun)?;
                let arguments = self.call_arguments(arguments)?;
                self.at(
                    span,
                    n::Expr::SurfaceCall {
                        function,
                        arguments,
                        direct_builtin,
                    },
                )
            }
            A::BinOp {
                name, left, right, ..
            } => {
                let left = self.expr(left)?;
                let right = self.expr(right)?;
                match name {
                    BinOp::Eq | BinOp::NotEq => self.at(
                        span,
                        n::Expr::Equal {
                            left,
                            right,
                            negate: *name == BinOp::NotEq,
                        },
                    ),
                    BinOp::And => self.if_(span, left, right, self.bool(span, false)),
                    BinOp::Or => self.if_(span, left, self.bool(span, true), right),
                    BinOp::LtInt
                    | BinOp::LtEqInt
                    | BinOp::GtInt
                    | BinOp::GtEqInt
                    | BinOp::AddInt
                    | BinOp::SubInt
                    | BinOp::MultInt
                    | BinOp::DivInt
                    | BinOp::ModInt => {
                        let builtin = profile::integer_operator(*name)
                            .expect("every fixed integer operator has a compiler builtin");
                        let arguments = if matches!(name, BinOp::GtInt | BinOp::GtEqInt) {
                            [right, left]
                        } else {
                            [left, right]
                        };
                        self.call(span, self.builtin(span, builtin), &arguments)
                    }
                }
            }
            A::UnOp {
                op: UnOp::Negate,
                value,
                ..
            } => {
                if let A::UInt { value, .. } = value.as_ref() {
                    self.at(
                        span,
                        n::Expr::Constant(self.integer(span, &format!("-{value}"))?),
                    )
                } else {
                    let value = self.expr(value)?;
                    self.call(
                        span,
                        self.builtin(span, "subtractInteger"),
                        &[self.at(span, n::Expr::Constant(n::Constant::Int(0))), value],
                    )
                }
            }
            A::UnOp {
                op: UnOp::Not,
                value,
                ..
            } => {
                let value = self.expr(value)?;
                self.not(span, value)
            }
            A::LogicalOpChain {
                kind, expressions, ..
            } => {
                let expressions = expressions
                    .iter()
                    .map(|item| self.expr(item))
                    .collect::<Result<Vec<_>>>()?;
                let mut result = self.bool(span, *kind == LogicalOpChainKind::And);
                for item in expressions.iter().rev() {
                    result = match kind {
                        LogicalOpChainKind::And => {
                            self.if_(span, item, result, self.bool(span, false))
                        }
                        LogicalOpChainKind::Or => {
                            self.if_(span, item, self.bool(span, true), result)
                        }
                    };
                }
                result
            }
            A::When {
                subject, clauses, ..
            } => {
                let scrutinee = self.expr(subject)?;
                let mut arms = Vec::new();
                let mut shared_bodies = Vec::new();
                for clause in clauses {
                    let shared = (clause.patterns.len() > 1).then(|| self.fresh());
                    let mut names = None;
                    for source_pattern in &clause.patterns {
                        let scope = self.locals.len();
                        let pattern = self.pattern(source_pattern)?;
                        let bindings = self.locals[scope..]
                            .iter()
                            .cloned()
                            .collect::<std::collections::BTreeMap<_, _>>();
                        let current_names = bindings.keys().cloned().collect::<Vec<_>>();
                        if let Some(previous) = &names {
                            if previous != &current_names {
                                return self.invalid_declaration(
                                    source_pattern.location(),
                                    "Alternative patterns must bind exactly the same names.",
                                );
                            }
                        } else {
                            names = Some(current_names);
                            if let Some(name) = shared {
                                let body = self.expr(&clause.then)?;
                                let body = self
                                    .at(clause.then.location(), n::Expr::TypeScope { value: body });
                                let mut parameters = bindings
                                    .values()
                                    .map(|name| {
                                        self.at(source_pattern.location(), n::Pattern::Var(name))
                                    })
                                    .collect::<Vec<_>>();
                                if parameters.is_empty() {
                                    parameters.push(self.at(span, n::Pattern::Unit));
                                }
                                let function = self.at(
                                    span,
                                    n::Expr::Lambda {
                                        parameters: self.slice(&parameters),
                                        body,
                                    },
                                );
                                shared_bodies.push((name, function));
                            }
                        }
                        let body = if let Some(name) = shared {
                            let mut arguments = bindings
                                .values()
                                .map(|name| self.var(span, name))
                                .collect::<Vec<_>>();
                            if arguments.is_empty() {
                                arguments.push(self.at(span, n::Expr::Unit));
                            }
                            self.call(span, self.var(span, name), &arguments)
                        } else {
                            let body = self.expr(&clause.then)?;
                            self.at(clause.then.location(), n::Expr::TypeScope { value: body })
                        };
                        self.locals.truncate(scope);
                        arms.push(self.alloc(n::CaseArm { pattern, body }));
                    }
                }
                let mut result = self.at(
                    span,
                    n::Expr::Case {
                        scrutinee,
                        arms: self.slice(&arms),
                    },
                );
                for (name, function) in shared_bodies.into_iter().rev() {
                    result = self.apply_pattern(
                        span,
                        self.at(span, n::Pattern::Var(name)),
                        function,
                        result,
                    );
                }
                result
            }
            A::If {
                branches,
                final_else,
                ..
            } => {
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    let scope = self.locals.len();
                    let value = self.expr(&branch.condition)?;
                    let matched = branch
                        .is
                        .as_ref()
                        .map(|is| -> Result<_> {
                            let annotation = is
                                .annotation
                                .as_ref()
                                .map(|typ| self.typ(typ))
                                .transpose()?;
                            let pattern = self.pattern(&is.pattern)?;
                            Result::Ok((pattern, annotation))
                        })
                        .transpose()?;
                    let body = self.expr(&branch.body)?;
                    self.locals.truncate(scope);
                    lowered.push((branch.location, value, matched, body));
                }
                let mut result = self.expr(final_else)?;
                for (location, value, matched, body) in lowered.into_iter().rev() {
                    result = if let Some((pattern, annotation)) = matched {
                        self.at(
                            location,
                            n::Expr::Match {
                                value,
                                pattern,
                                body,
                                fallback: result,
                                annotation,
                                conversion: Some(if annotation.is_some() {
                                    n::ConversionSite::CastTest
                                } else {
                                    n::ConversionSite::CastPattern
                                }),
                            },
                        )
                    } else {
                        self.if_(location, value, body, result)
                    };
                }
                result
            }
            A::FieldAccess {
                label, container, ..
            } => {
                if let A::Var { name, .. } = container.as_ref() {
                    if let Some(handler) = self.validator_handler(span, name, label) {
                        return Ok(handler);
                    }
                    if self.qualifiers.contains(name.as_str()) {
                        let module = self.module_field(span, name, label);
                        if self.local(name).is_some() || self.globals.contains(name.as_str()) {
                            return Ok(self.at(
                                span,
                                n::Expr::FieldOrModule {
                                    record: self.value_name(container.location(), name),
                                    field: self.at(span, self.text(label)),
                                    module,
                                },
                            ));
                        }
                        return module.ok_or_else(|| {
                            crate::validate::unsupported(
                                self.spans,
                                span,
                                &format!("builtin `{label}`"),
                            )
                        });
                    }
                    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                        let (module, type_name) = if self.local_types.contains(name.as_str()) {
                            return self.invalid_declaration(
                                span,
                                "Type-qualified constructors require an imported declaring module.",
                            );
                        } else if let Some((module, original)) = self.imported.get(name.as_str()) {
                            if self.prelude_qualifiers.contains(*module) {
                                return self.invalid_declaration(span, "The compiler prelude has no importable type-qualified constructor namespace.");
                            }
                            (Some(*module), *original)
                        } else if profile::primitive(name).is_some() {
                            return self.invalid_declaration(span, "The compiler prelude has no importable type-qualified constructor namespace.");
                        } else {
                            (None, self.text(name))
                        };
                        return Ok(self.at(
                            span,
                            n::Expr::ConstructorRef {
                                module,
                                type_name: Some(type_name),
                                name: self.text(label),
                            },
                        ));
                    }
                }
                if let A::FieldAccess {
                    container: module,
                    label: type_name,
                    ..
                } = container.as_ref()
                    && let A::Var { name, .. } = module.as_ref()
                    && self.qualifiers.contains(name.as_str())
                {
                    if self.prelude_qualifiers.contains(name.as_str()) {
                        return self.invalid_declaration(span, "The compiler prelude has no importable type-qualified constructor namespace.");
                    }
                    return Ok(self.at(
                        span,
                        n::Expr::ConstructorRef {
                            module: Some(self.text(name)),
                            type_name: Some(self.text(type_name)),
                            name: self.text(label),
                        },
                    ));
                }
                let record = self.expr(container)?;
                self.at(
                    span,
                    n::Expr::Access {
                        record,
                        field: self.at(span, self.text(label)),
                    },
                )
            }
            A::Tuple { elems, .. } => {
                let elems = elems
                    .iter()
                    .map(|item| self.expr(item))
                    .collect::<Result<Vec<_>>>()?;
                match elems.as_slice() {
                    [first, second, rest @ ..] => self.at(
                        span,
                        n::Expr::DataTuple {
                            first,
                            second,
                            rest: self.slice(rest),
                        },
                    ),
                    _ => {
                        return self
                            .invalid_declaration(span, "Tuples require at least two elements.");
                    }
                }
            }
            A::ErrorTerm { .. } => self.call(
                span,
                self.builtin(span, "error"),
                &[self.at(span, n::Expr::Unit)],
            ),
            A::Trace {
                location,
                then,
                label,
                arguments,
                ..
            } => {
                let label_span = label.location();
                let label = self.expr(label)?;
                let label = self.at(label_span, n::Expr::Format { value: label });
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        let value = self.expr(argument)?;
                        Ok(self.at(argument.location(), n::Expr::Format { value }))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let body = self.expr(then)?;
                self.at(
                    *location,
                    n::Expr::TraceLabel {
                        label,
                        arguments: self.slice(&arguments),
                        body,
                        verbose_only: false,
                    },
                )
            }
            A::PipeLine { expressions, .. } => {
                let mut stages = expressions.iter();
                let mut input = self.expr(stages.next().expect("parser pipeline is nonempty"))?;
                for stage in stages {
                    let (fun, arguments) = match stage {
                        A::Call { fun, arguments, .. } => {
                            (fun.as_ref(), Some(arguments.as_slice()))
                        }
                        stage => (stage, None),
                    };
                    let direct_builtin = self.direct_builtin(fun);
                    let function = self.expr(fun)?;
                    let arguments = arguments
                        .map(|args| self.call_arguments(args))
                        .transpose()?;
                    input = self.at(
                        stage.location(),
                        n::Expr::Pipe {
                            input,
                            function,
                            arguments,
                            direct_builtin,
                        },
                    );
                }
                input
            }
            A::Pair { fst, snd, .. } => {
                let first = self.expr(fst)?;
                let second = self.expr(snd)?;
                self.at(span, n::Expr::Pair { first, second })
            }
            A::TupleIndex { tuple, index, .. } => {
                let tuple = self.expr(tuple)?;
                self.at(
                    span,
                    n::Expr::TupleIndex {
                        tuple,
                        index: *index,
                    },
                )
            }
            A::CurvePoint { point, .. } => {
                use aiken_lang::ast::{Bls12_381Point, Curve};
                let bytes = self.slice(&point.compress());
                let constant = match point.as_ref() {
                    Curve::Bls12_381(Bls12_381Point::G1(_)) => n::Constant::BlsG1(bytes),
                    Curve::Bls12_381(Bls12_381Point::G2(_)) => n::Constant::BlsG2(bytes),
                };
                self.at(span, n::Expr::Constant(constant))
            }
            A::RecordUpdate {
                constructor,
                spread,
                arguments,
                ..
            } => {
                let constructor = self.expr(constructor)?;
                let base = self.expr(&spread.base)?;
                let fields = arguments
                    .iter()
                    .map(|argument| {
                        let value = self.expr(&argument.value)?;
                        Ok(self.alloc(n::FieldAssign {
                            field: self.at(argument.location, self.text(&argument.label)),
                            value,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.at(
                    span,
                    n::Expr::RecordUpdate {
                        constructor,
                        base,
                        fields: self.slice(&fields),
                    },
                )
            }
            A::TraceIfFalse { value, .. } => {
                let message = format!(
                    "{} ? False",
                    aiken_lang::format::Formatter::new()
                        .expr(value, false)
                        .to_pretty_string(999)
                );
                let message = self.at(
                    span,
                    n::Expr::Constant(n::Constant::Str(self.text(&message))),
                );
                let condition = self.expr(value)?;
                let on_false = self.at(
                    span,
                    n::Expr::TraceLabel {
                        label: message,
                        arguments: &[],
                        body: self.bool(span, false),
                        verbose_only: true,
                    },
                );
                self.if_(span, condition, self.bool(span, true), on_false)
            }
            A::Assignment { .. } => return self.sequence(span, std::slice::from_ref(expr)),
        })
    }

    fn call_arguments(
        &mut self,
        arguments: &[aiken_lang::ast::CallArg<A>],
    ) -> Result<&'a [n::CallArgument<'a>]> {
        let arguments = arguments
            .iter()
            .map(|argument| {
                Ok(n::CallArgument {
                    label: argument
                        .label
                        .as_deref()
                        .map(|label| self.at(argument.location, self.text(label))),
                    value: self.expr(&argument.value)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(self.arena.alloc_slice_fill_iter(arguments))
    }

    pub(super) fn grouped_builtin(&self, span: Span, value: Expr<'a>, arity: usize) -> Expr<'a> {
        let arguments = (0..arity)
            .map(|_| self.at(span, n::Type::Hole))
            .collect::<Vec<_>>();
        self.at(
            span,
            n::Expr::Convert {
                kind: n::ConversionKind::Identity,
                typ: self.at(
                    span,
                    n::Type::Function {
                        arguments: self.slice(&arguments),
                        result: self.at(span, n::Type::Hole),
                    },
                ),
                value,
            },
        )
    }

    fn module_field(&self, span: Span, module: &str, label: &str) -> Option<Expr<'a>> {
        if self.builtin_qualifiers.contains(module) {
            let builtin = profile::builtin_value(label)?;
            let value = self.at(
                span,
                n::Expr::VarQual {
                    kind: n::VarType::LowVar,
                    module: self.text(module),
                    name: builtin,
                },
            );
            return Some(match profile::builtin_arity(label) {
                Some(arity) => self.grouped_builtin(span, value, arity),
                None => value,
            });
        }
        if self.prelude_qualifiers.contains(module)
            && let Some(value) = self.prelude_reference(span, self.text(module), label)
        {
            return Some(value);
        }
        Some(self.at(
            span,
            n::Expr::VarQual {
                kind: Self::kind(label),
                module: self.text(module),
                name: self.text(label),
            },
        ))
    }

    fn direct_builtin(&self, fun: &A) -> Option<n::DirectBuiltin> {
        let A::FieldAccess {
            container, label, ..
        } = fun
        else {
            return None;
        };
        let A::Var { name, .. } = container.as_ref() else {
            return None;
        };
        if !self.builtin_qualifiers.contains(name.as_str()) {
            return None;
        }
        match profile::builtin_value(label)? {
            "ifThenElse" => Some(n::DirectBuiltin::IfThenElse),
            "dataListChoose" => Some(n::DirectBuiltin::ChooseList),
            "chooseData" => Some(n::DirectBuiltin::ChooseData),
            "chooseValue" => Some(n::DirectBuiltin::ChooseUnit),
            "trace" => Some(n::DirectBuiltin::Trace),
            _ => None,
        }
    }

    fn sequence(&mut self, span: Span, expressions: &[A]) -> Result<Expr<'a>> {
        let scope = self.locals.len();
        let result = self.sequence_inner(span, expressions);
        self.locals.truncate(scope);
        result
    }

    fn sequence_inner(&mut self, span: Span, expressions: &[A]) -> Result<Expr<'a>> {
        let Some((first, rest)) = expressions.split_first() else {
            return Err(self.assignment_error(span, "An expression sequence cannot be empty."));
        };
        let location = first.location();
        if let A::Assignment {
            value,
            patterns,
            kind,
            ..
        } = first
        {
            if kind.is_backpassing() {
                return self.backpassing(first, rest);
            }
            if patterns.len() != 1 {
                return Err(self.assignment_error(
                    location,
                    "Multiple assignment patterns require the backpassing arrow `<-`, not `=`.",
                ));
            }
            if rest.is_empty() && !kind.is_expect() {
                return Err(self.assignment_error(
                    location,
                    "A let assignment must be followed by an expression.",
                ));
            }
            let assignment = patterns.first();
            // The initializer is outside the new binding's lexical scope.
            let mut value = self.expr(value)?;
            let annotation = assignment
                .annotation
                .as_ref()
                .map(|typ| self.typ(typ))
                .transpose()?;
            if kind.is_let()
                && let Some(typ) = annotation
            {
                value = self.conversion_ascribe(
                    location,
                    value,
                    typ,
                    n::ConversionSite::AnnotatedBinding,
                );
            }
            let pattern = self.pattern(&assignment.pattern)?;
            let body = if rest.is_empty() {
                self.at(location, n::Expr::Unit)
            } else {
                self.sequence_inner(span, rest)?
            };
            if kind.is_expect() {
                let fallback = self.call(
                    location,
                    self.builtin(location, "error"),
                    &[self.at(location, n::Expr::Unit)],
                );
                Ok(self.at(
                    location,
                    n::Expr::Match {
                        value,
                        pattern,
                        body,
                        fallback,
                        annotation,
                        conversion: Some(if annotation.is_some() {
                            n::ConversionSite::ExpectBinding
                        } else {
                            n::ConversionSite::ExpectPattern
                        }),
                    },
                ))
            } else {
                Ok(self.at(
                    location,
                    n::Expr::LetValue {
                        pattern,
                        value,
                        body,
                    },
                ))
            }
        } else if rest.is_empty() {
            self.expr(first)
        } else {
            let value = self.expr(first)?;
            let value = self.ascribe(location, value, self.at(location, n::Type::Unit));
            let body = self.sequence_inner(span, rest)?;
            Ok(self.apply_pattern(location, self.at(location, n::Pattern::Unit), value, body))
        }
    }

    fn assignment_error(&self, span: Span, message: &str) -> nash_frontend::FrontendFailure {
        nash_frontend::FrontendDiagnostic::error(
            "NAF2103",
            "Invalid assignment",
            message,
            Some(self.spans.region(span)),
        )
        .into()
    }

    /// Rewrite callback syntax before lowering expressions. In particular a
    /// call receives the callback as its final argument, rather than applying
    /// the call's result; captured functions retain any remaining parameters.
    fn backpassing(&mut self, assignment: &A, rest: &[A]) -> Result<Expr<'a>> {
        use aiken_lang::ast::{ArgName, AssignmentPattern, CallArg, Pattern};
        let A::Assignment {
            location,
            value,
            kind,
            patterns,
            comment,
        } = assignment
        else {
            unreachable!("backpassing is only dispatched for an assignment")
        };
        let Some(last) = rest.last() else {
            return Err(self.assignment_error(
                *location,
                "A backpassing assignment must be followed by its continuation.",
            ));
        };
        let call_location = Span {
            start: location.start,
            end: last.location().end,
        };
        let lambda_location = Span {
            start: location.end,
            end: last.location().end,
        };
        let mut names = Vec::with_capacity(patterns.len());
        let mut checks = Vec::with_capacity(patterns.len());
        for assignment in patterns {
            let pattern_location = assignment.pattern.location();
            let name = match &assignment.pattern {
                Pattern::Var { name, location } if kind.is_let() => ArgName::Named {
                    label: name.clone(),
                    name: name.clone(),
                    location: *location,
                },
                Pattern::Discard { name, location } if kind.is_let() => ArgName::Discarded {
                    label: name.clone(),
                    name: name.clone(),
                    location: *location,
                },
                pattern => {
                    let name = self.fresh().to_owned();
                    checks.push(A::Assignment {
                        location: assignment.location,
                        value: Box::new(A::Var {
                            location: pattern_location,
                            name: name.clone(),
                        }),
                        patterns: AssignmentPattern::new(
                            pattern.clone(),
                            assignment.annotation.clone(),
                            assignment.location,
                        )
                        .into(),
                        kind: if kind.is_let()
                            || (pattern.is_var() && assignment.annotation.is_none())
                        {
                            AssignmentKind::let_()
                        } else {
                            AssignmentKind::expect()
                        },
                        comment: comment.clone(),
                    });
                    ArgName::Named {
                        label: name.clone(),
                        name,
                        location: pattern_location,
                    }
                }
            };
            names.push((name, pattern_location, assignment.annotation.clone()));
        }
        // The pinned rewrite prepends each check, so the last parameter is
        // checked first. Keep this order observable for refutations and traces.
        checks.reverse();
        checks.extend_from_slice(rest);
        let callback = A::lambda(names, checks, lambda_location);
        let callback_arg = CallArg {
            location: lambda_location,
            label: None,
            value: callback,
        };
        let rewritten = match value.as_ref() {
            A::Call { fun, arguments, .. } => {
                let mut arguments = arguments.clone();
                arguments.push(callback_arg);
                A::Call {
                    location: call_location,
                    fun: fun.clone(),
                    arguments,
                }
            }
            A::Fn {
                fn_style,
                arguments,
                return_annotation,
                ..
            } => {
                let call = A::Call {
                    location: call_location,
                    fun: value.clone(),
                    arguments: vec![callback_arg],
                };
                if arguments.len() <= 1 {
                    call
                } else {
                    A::Fn {
                        location: call_location,
                        fn_style: *fn_style,
                        arguments: arguments.iter().skip(1).cloned().collect(),
                        body: Box::new(call),
                        return_annotation: return_annotation.clone(),
                    }
                }
            }
            _ => A::Call {
                location: call_location,
                fun: value.clone(),
                arguments: vec![callback_arg],
            },
        };
        self.expr(&rewritten)
    }
}
