use super::{Expr, Lower, Result, n, profile};
use aiken_lang::{
    ast::{AssignmentKind, BinOp, LogicalOpChainKind, Span, UnOp},
    expr::UntypedExpr as A,
};

impl<'a> Lower<'a, '_> {
    pub(super) fn expr(&mut self, expr: &A) -> Result<Expr<'a>> {
        let span = expr.location();
        Ok(match expr {
            A::UInt { value, .. } => self.at(
                span,
                n::Expr::Constant(n::Constant::Int(self.integer(span, value)?)),
            ),
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
            A::Var { name, .. } => {
                if name == "main"
                    && self.validator_name.is_some()
                    && self.local(name).is_none()
                    && !self.imported.contains_key(name.as_str())
                {
                    return self.unsupported(span, "references to the generated validator main");
                }
                self.value_name(span, name)
            }
            A::Sequence { expressions, .. } => {
                let scope = self.locals.len();
                let result = self.sequence(span, expressions);
                self.locals.truncate(scope);
                result?
            }
            A::Fn {
                arguments,
                body,
                return_annotation,
                ..
            } => {
                let (parameters, body, annotation) =
                    self.function(span, arguments, return_annotation.as_ref(), body)?;
                let lambda = self.at(span, n::Expr::Lambda { parameters, body });
                if let Some(annotation) = annotation {
                    self.ascribe(span, lambda, annotation.typ)
                } else {
                    lambda
                }
            }
            A::List { elements, tail, .. } => {
                let elements = elements
                    .iter()
                    .map(|item| self.expr(item))
                    .collect::<Result<Vec<_>>>()?;
                if let Some(tail) = tail {
                    let mut result = self.expr(tail)?;
                    for head in elements.iter().rev() {
                        result = self.call(span, self.builtin(span, "mkCons"), &[*head, result]);
                    }
                    result
                } else {
                    self.at(span, n::Expr::List(self.slice(&elements)))
                }
            }
            A::Call { fun, arguments, .. } => {
                let function = self.expr(fun)?;
                if arguments.iter().any(|arg| arg.label.is_some()) {
                    let constructor = match fun.as_ref() {
                        A::Var { name, .. } => name.starts_with(|c: char| c.is_ascii_uppercase()),
                        A::FieldAccess { label, .. } => {
                            label.starts_with(|c: char| c.is_ascii_uppercase())
                        }
                        _ => false,
                    };
                    if !constructor || arguments.iter().any(|arg| arg.label.is_none()) {
                        return self
                            .unsupported(span, "labeled function arguments or mixed call labels");
                    }
                    let mut fields = Vec::new();
                    for arg in arguments {
                        let Some(label) = arg.label.as_deref() else {
                            return self.unsupported(arg.location, "unlabeled record arguments");
                        };
                        let value = self.expr(&arg.value)?;
                        fields.push(self.alloc(n::FieldAssign {
                            field: self.at(arg.location, self.text(label)),
                            value,
                        }));
                    }
                    self.call(
                        span,
                        function,
                        &[self.at(
                            span,
                            n::Expr::Record {
                                fields: self.slice(&fields),
                                grouped: false,
                            },
                        )],
                    )
                } else {
                    let mut arguments = arguments
                        .iter()
                        .map(|arg| self.expr(&arg.value))
                        .collect::<Result<Vec<_>>>()?;
                    if arguments.is_empty() {
                        arguments.push(self.at(span, n::Expr::Unit));
                    }
                    self.call(span, function, &arguments)
                }
            }
            A::BinOp {
                name, left, right, ..
            } => {
                if matches!(name, BinOp::Eq | BinOp::NotEq) {
                    return self.unsupported(
                        span,
                        "polymorphic equality (use an explicitly typed equality function)",
                    );
                }
                let left = self.expr(left)?;
                let right = self.expr(right)?;
                match name {
                    BinOp::And => self.if_(span, left, right, self.bool(span, false)),
                    BinOp::Or => self.if_(span, left, self.bool(span, true), right),
                    _ => {
                        let Some(builtin) = profile::integer_operator(*name) else {
                            return self.unsupported(span, "this binary operator");
                        };
                        let call = self.call(span, self.builtin(span, builtin), &[left, right]);
                        if matches!(name, BinOp::GtInt | BinOp::GtEqInt) {
                            self.not(span, call)
                        } else {
                            call
                        }
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
                        n::Expr::Constant(n::Constant::Int(
                            self.integer(span, &format!("-{value}"))?,
                        )),
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
                for clause in clauses {
                    for pattern in &clause.patterns {
                        let scope = self.locals.len();
                        let pattern = self.pattern(pattern)?;
                        let body = self.expr(&clause.then)?;
                        self.locals.truncate(scope);
                        arms.push(self.alloc(n::CaseArm { pattern, body }));
                    }
                }
                self.at(
                    span,
                    n::Expr::Case {
                        scrutinee,
                        arms: self.slice(&arms),
                    },
                )
            }
            A::If {
                branches,
                final_else,
                ..
            } => {
                let mut lowered = Vec::new();
                for branch in branches {
                    if branch.is.is_some() {
                        return self.unsupported(branch.location, "if/is Data casts");
                    }
                    let condition = self.expr(&branch.condition)?;
                    let then_branch = self.expr(&branch.body)?;
                    lowered.push(self.alloc(n::IfBranch {
                        condition,
                        then_branch,
                    }));
                }
                let final_else = self.expr(final_else)?;
                self.at(
                    span,
                    n::Expr::If {
                        branches: self.slice(&lowered),
                        final_else,
                    },
                )
            }
            A::FieldAccess {
                label, container, ..
            } => {
                if let A::Var { name, .. } = container.as_ref() {
                    if self.validator_name == Some(name.as_str())
                        && self.local(name).is_none()
                        && !self.globals.contains(name.as_str())
                        && !self.qualifiers.contains(name.as_str())
                    {
                        return self
                            .unsupported(span, "calling validator handlers as ordinary functions");
                    }
                    if self.local(name).is_none()
                        && !self.globals.contains(name.as_str())
                        && self.qualifiers.contains(name.as_str())
                    {
                        if self.builtin_qualifiers.contains(name.as_str()) {
                            let Some(builtin) = profile::builtin_value(label) else {
                                return self.unsupported(span, &format!("builtin `{label}`"));
                            };
                            return Ok(self.at(
                                span,
                                n::Expr::VarQual {
                                    kind: n::VarType::LowVar,
                                    module: self.text(name),
                                    name: builtin,
                                },
                            ));
                        }
                        return Ok(self.at(
                            span,
                            n::Expr::VarQual {
                                kind: Self::kind(label),
                                module: self.text(name),
                                name: self.text(label),
                            },
                        ));
                    }
                    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                        return self.unsupported(span, "type-qualified constructor access");
                    }
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
                        n::Expr::Tuple {
                            first,
                            second,
                            rest: self.slice(rest),
                        },
                    ),
                    _ => return self.unsupported(span, "tuples with fewer than two elements"),
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
                if !arguments.is_empty() {
                    return self.unsupported(*location, "trace value formatting");
                }
                let label = self.expr(label)?;
                let body = self.expr(then)?;
                let thunk = self.at(
                    *location,
                    n::Expr::Lambda {
                        parameters: self.slice(&[self.at(*location, n::Pattern::Unit)]),
                        body,
                    },
                );
                let traced =
                    self.call(*location, self.builtin(*location, "trace"), &[label, thunk]);
                self.call(*location, traced, &[self.at(*location, n::Expr::Unit)])
            }
            A::PipeLine { .. } => {
                return self.unsupported(
                    span,
                    "pipelines (argument insertion depends on inferred Aiken arity)",
                );
            }
            A::Pair { .. } => {
                return self.unsupported(
                    span,
                    "Pair construction (a builtin pair is not a Nash tuple)",
                );
            }
            A::TupleIndex { .. } => {
                return self.unsupported(span, "tuple indexing without a known tuple arity");
            }
            A::CurvePoint { .. } => return self.unsupported(span, "curve point literals"),
            A::RecordUpdate { .. } => return self.unsupported(span, "record updates"),
            A::TraceIfFalse { .. } => {
                return self.unsupported(span, "trace-if-false source rendering");
            }
            A::Assignment { .. } => {
                return self.unsupported(span, "assignments without a following expression");
            }
        })
    }

    fn sequence(&mut self, span: Span, expressions: &[A]) -> Result<Expr<'a>> {
        let Some((first, rest)) = expressions.split_first() else {
            return self.unsupported(span, "empty expression sequences");
        };
        if rest.is_empty() {
            return self.expr(first);
        }
        let location = first.location();
        if let A::Assignment {
            value,
            patterns,
            kind,
            ..
        } = first
        {
            if !matches!(kind, AssignmentKind::Let { backpassing: false }) || patterns.len() != 1 {
                return self.unsupported(
                    location,
                    "expect, backpassing, or multi-pattern assignments",
                );
            }
            let assignment = patterns.first();
            let mut value = self.expr(value)?;
            if let Some(annotation) = &assignment.annotation {
                if Self::has_type_variable(annotation) {
                    return self.unsupported(location, "polymorphic local type ascriptions");
                }
                let typ = self.typ(annotation)?;
                value = self.ascribe(location, value, typ);
            }
            let pattern = self.pattern(&assignment.pattern)?;
            let body = self.sequence(span, rest)?;
            Ok(self.apply_pattern(location, pattern, value, body))
        } else {
            let value = self.expr(first)?;
            let value = self.ascribe(location, value, self.at(location, n::Type::Unit));
            let body = self.sequence(span, rest)?;
            Ok(self.apply_pattern(location, self.at(location, n::Pattern::Unit), value, body))
        }
    }
}
