use super::*;

impl<'a> Engine<'a, '_, '_> {
    pub(super) fn constants_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::Unit => self.ir.lit(Constant::unit(self.ir.arena)),
            Expr::Constant(constant) => match *constant {
                can::Constant::Int(n) => self.ir.int(n),
                can::Constant::BigInt(digits) => {
                    let value = digits.parse().map_err(|_| Error::InvalidInstance(node))?;
                    let value = self.ir.arena.alloc_integer(value);
                    self.ir.lit(Constant::integer(self.ir.arena, value))
                }
                can::Constant::Bytes(bytes) => {
                    self.ir.lit(Constant::byte_string(self.ir.arena, bytes))
                }
                can::Constant::Str(s) => self.ir.lit(Constant::string(self.ir.arena, s)),
                can::Constant::BlsG1(bytes) => {
                    let point = nash_plutus::bls::Compressable::uncompress(self.ir.arena, bytes)
                        .map_err(|_| Error::InvalidInstance(node))?;
                    self.ir.lit(Constant::g1(self.ir.arena, point))
                }
                can::Constant::BlsG2(bytes) => {
                    let point = nash_plutus::bls::Compressable::uncompress(self.ir.arena, bytes)
                        .map_err(|_| Error::InvalidInstance(node))?;
                    self.ir.lit(Constant::g2(self.ir.arena, point))
                }
            },
            Expr::ModuleConstantCheck { value } => {
                let ty = self.ty(node, ctx)?;
                let body = self.expr(value, ctx)?;
                let body = self.closed_dependencies(body)?;
                let body = crate::casts::expand_with_traces(
                    &self.ir,
                    &mut self.types,
                    body,
                    self.trace.compiler,
                )?;
                let term = crate::comptime::eval_closed_term(
                    self.ir.arena,
                    &[],
                    body,
                    nash_plutus::machine::ExBudget::max(),
                )
                .map_err(|error| match error {
                    crate::comptime::ComptimeError::Evaluation(reason) => {
                        Error::ComptimeEvaluation(reason)
                    }
                    other => Error::ComptimeAssembly(other.to_string()),
                })?;
                self.ir.arena.alloc(Core::Evaluated { term, ty })
            }
            _ => unreachable!("constants codegen dispatch"),
        })
    }

    pub(super) fn operations_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::TupleIndex { tuple, index } => {
                let target = self.ty(node, ctx)?;
                let container = self.ty(NodeId::expr(tuple), ctx)?;
                let value = self.expr(tuple, ctx)?;
                let data = match container {
                    Ty::Const(ConstTy::DataPair(..)) => self
                        .ir
                        .builtin(if *index == 0 { F::FstPair } else { F::SndPair }, &[value]),
                    Ty::Const(ConstTy::DataTuple(_)) => {
                        let mut tail = value;
                        for _ in 0..*index {
                            tail = self.ir.builtin(F::TailList, &[tail]);
                        }
                        self.ir.builtin(F::HeadList, &[tail])
                    }
                    _ => return Err(Error::RuntimeLayout(container)),
                };
                self.ir.cast(CastKind::FromDataShallow, DATA, target, data)
            }
            Expr::Match {
                value,
                pattern,
                body,
                fallback,
                conversion,
                ..
            } => self.refutable_match(node, value, pattern, body, fallback, *conversion, ctx)?,
            Expr::RecordUpdate { base, fields, .. } => {
                self.encoded_record_update(base, fields, ctx)?
            }
            Expr::Equal {
                left,
                right,
                negate,
            } => self.equality(left, right, *negate, ctx)?,
            Expr::Format { value } => self.format_value(value, ctx)?,
            Expr::TraceLabel {
                label,
                arguments,
                body,
                verbose_only,
            } => {
                let body = self.expr(body, ctx)?;
                if self.trace.user == TraceLevel::Silent
                    || (*verbose_only && self.trace.user != TraceLevel::Verbose)
                {
                    body
                } else {
                    let mut message = self.expr(label, ctx)?;
                    if self.trace.user == TraceLevel::Verbose {
                        for (index, argument) in arguments.iter().enumerate() {
                            let value = self.expr(argument, ctx)?;
                            let delimiter = self.ir.lit(Constant::string(
                                self.ir.arena,
                                if index == 0 { ": " } else { ", " },
                            ));
                            message = self.ir.builtin(F::AppendString, &[message, delimiter]);
                            message = self.ir.builtin(F::AppendString, &[message, value]);
                        }
                    }
                    self.ir.trace(message, body)
                }
            }
            _ => unreachable!("operations codegen dispatch"),
        })
    }

    pub(super) fn conversion_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::Convert { kind, value, .. } => {
                let raw_boundary = matches!(
                    kind,
                    ConversionKind::Ascription(
                        ConversionSite::ValidatorDatum
                            | ConversionSite::ValidatorPurpose
                            | ConversionSite::ValidatorContext
                    )
                );
                let kind = match *kind {
                    ConversionKind::Ascription(_) => self
                        .solved(ctx)
                        .conversions
                        .get(&node)
                        .copied()
                        .ok_or(Error::MissingType(node))?,
                    kind => kind,
                };
                if kind == ConversionKind::Identity {
                    return self.expr(value, ctx);
                }
                let source = self.ty(NodeId::expr(value), ctx)?;
                let target = self.ty(node, ctx)?;
                let value = self.expr(value, ctx)?;
                let kind = match kind {
                    ConversionKind::ToData => CastKind::ToData,
                    ConversionKind::Ascription(_) | ConversionKind::Identity => {
                        return Err(Error::InvalidInstance(node));
                    }
                    ConversionKind::FromDataShallow => CastKind::FromDataShallow,
                    ConversionKind::FromDataBytesView => {
                        return Ok(self.ir.builtin(F::UnBData, &[value]));
                    }
                    ConversionKind::ValidateData => CastKind::ValidateData,
                    ConversionKind::ViewData => {
                        if source != DATA || (!raw_boundary && !matches!(target, Ty::Big(_))) {
                            return Err(Error::RuntimeLayout(target));
                        }
                        return Ok(value);
                    }
                };
                self.ir.cast(kind, source, target, value)
            }
            _ => unreachable!("conversion codegen dispatch"),
        })
    }

    pub(super) fn collections_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::Pair { first, second } => {
                let first_ty = self.ty(NodeId::expr(first), ctx)?;
                let second_ty = self.ty(NodeId::expr(second), ctx)?;
                let first = self.expr(first, ctx)?;
                let second = self.expr(second, ctx)?;
                self.ir.builtin(
                    F::MkPairData,
                    &[
                        self.ir.cast(CastKind::ToData, first_ty, DATA, first),
                        self.ir.cast(CastKind::ToData, second_ty, DATA, second),
                    ],
                )
            }
            Expr::DataList { elements, tail } => {
                let ty = self.ty(node, ctx)?;
                let Ty::Const(ConstTy::DataList(element)) = ty else {
                    return Err(Error::RuntimeLayout(ty));
                };
                let map = matches!(element, Ty::Const(ConstTy::DataPair(_, _)));
                let values = elements
                    .iter()
                    .map(|expr| {
                        let value = self.expr(expr, ctx)?;
                        Ok(if map {
                            value
                        } else {
                            self.ir.cast(CastKind::ToData, *element, DATA, value)
                        })
                    })
                    .collect::<Result<Vec<_>, Error<'a>>>()?;
                let mut result = if let Some(tail) = tail {
                    self.expr(tail, ctx)?
                } else {
                    self.list(if map { *element } else { DATA }, &[])?
                };
                for value in values.iter().rev() {
                    result = self.ir.builtin(F::MkCons, &[value, result]);
                }
                result
            }
            Expr::DataTuple {
                first,
                second,
                rest,
            } => {
                let items = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|expr| {
                        let ty = self.ty(NodeId::expr(expr), ctx)?;
                        let value = self.expr(expr, ctx)?;
                        Ok(self.ir.cast(CastKind::ToData, ty, DATA, value))
                    })
                    .collect::<Result<Vec<_>, Error<'a>>>()?;
                self.list(DATA, &items)?
            }
            Expr::Tuple {
                first,
                second,
                rest,
            } => {
                let items = [*first, *second]
                    .into_iter()
                    .chain(rest.iter().copied())
                    .map(|e| self.expr(e, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.ir.constr(0, &items)
            }
            Expr::List(items) => {
                let ty = self.ty(node, ctx)?;
                let Ty::Const(ConstTy::List(element)) = ty else {
                    return Err(Error::RuntimeLayout(ty));
                };
                let values = items
                    .iter()
                    .map(|e| self.expr(e, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.list(*element, &values)?
            }
            _ => unreachable!("collections codegen dispatch"),
        })
    }

    pub(super) fn values_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::Int(n) => {
                let value = self.ir.int(*n);
                self.literal("FromInt", "fromInt", node, value, ctx, 0)?
            }
            Expr::Str(s) => {
                let value = self.ir.lit(Constant::string(self.ir.arena, s));
                self.literal("FromString", "fromString", node, value, ctx, 0)?
            }
            Expr::Bytes(bytes) => {
                let value = self.ir.lit(Constant::byte_string(self.ir.arena, bytes));
                self.literal("FromBytes", "fromBytes", node, value, ctx, 0)?
            }
            Expr::VarLocal(name) => match ctx
                .env
                .get(name)
                .copied()
                .ok_or(Error::UnknownLocal(name))?
            {
                Binding::Value(binder) => self.ir.var(binder.name),
                Binding::Template { id, projection } => {
                    let binder = self.use_template(id, node, ctx)?;
                    if let Some(name) = projection {
                        self.destruct_projection(id, name, binder, node, ctx)?
                    } else {
                        self.ir.var(binder.name)
                    }
                }
            },
            Expr::VarTopLevel(reference)
            | Expr::VarForeign { reference, .. }
            | Expr::VarOperator { reference, .. } => self.reference(*reference, node, ctx)?,
            Expr::VarMethod {
                trait_,
                method,
                annotation,
            } => self.method(*trait_, method, annotation, node, ctx)?,
            Expr::VarConstructor {
                reference, index, ..
            } => self.constructor(*reference, *index, node, ctx)?,
            Expr::Binop {
                reference,
                left,
                right,
                ..
            } => {
                let func = self.reference(*reference, node, ctx)?;
                let left = self.expr(left, ctx)?;
                let right = self.expr(right, ctx)?;
                self.ir.app(func, &[left, right])
            }
            _ => unreachable!("values codegen dispatch"),
        })
    }

    pub(super) fn calls_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        Ok(match &expr.value {
            Expr::Callable { value, .. } => self.expr(value, ctx)?,
            Expr::Function { parameters, body } => {
                let function = if parameters.is_empty() {
                    let body = self.expr(body, ctx)?;
                    self.ir.lam(
                        &[Binder {
                            name: self.ir.fresh("unit"),
                            ty: Ty::Const(&ConstTy::Unit),
                        }],
                        body,
                    )
                } else {
                    self.lambda(parameters, body, ctx)?
                };
                self.no_inline_function(function)
            }
            Expr::SurfaceCall {
                function,
                arguments,
                direct_builtin,
            } => self.surface_call(function, arguments, *direct_builtin, node, ctx)?,
            Expr::Pipe {
                input,
                function,
                arguments,
                direct_builtin,
            } => self.pipe_call(input, function, *arguments, *direct_builtin, node, ctx)?,
            Expr::Call {
                function,
                arguments,
            } => {
                let func = self.expr(function, ctx)?;
                let args = arguments
                    .iter()
                    .map(|e| self.expr(e, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                self.ir.app(func, &args)
            }
            Expr::Lambda { parameters, body } => self.lambda(parameters, body, ctx)?,
            _ => unreachable!("calls codegen dispatch"),
        })
    }

    pub(super) fn control_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        Ok(match &expr.value {
            Expr::If {
                branches,
                final_else,
            } => {
                let mut body = self.expr(final_else, ctx)?;
                for branch in branches.iter().rev() {
                    let condition = self.expr(branch.condition, ctx)?;
                    let yes = self.expr(branch.then_branch, ctx)?;
                    body = self.ir.if_(condition, yes, body);
                }
                body
            }
            Expr::LetValue {
                pattern,
                value,
                body,
                uses,
                ..
            } => {
                if *uses != 0 {
                    self.source_let(pattern, value, body, ctx)?
                } else {
                    self.expr(body, ctx)?
                }
            }
            Expr::Let { definition, body } => self.let_definitions(&[*definition], body, ctx)?,
            Expr::LetRec { definitions, body } => self.let_definitions(definitions, body, ctx)?,
            Expr::LetDestruct {
                pattern,
                value,
                body,
            } => self.let_destruct(pattern, value, body, ctx)?,
            Expr::Case {
                scrutinee,
                branches,
            } => self.case(scrutinee, branches, ctx)?,
            _ => unreachable!("control codegen dispatch"),
        })
    }

    pub(super) fn records_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::expr(expr);
        if let Expr::FieldOrModule { module, .. } = &expr.value
            && !self
                .solved(ctx)
                .field_selections
                .get(&node)
                .copied()
                .ok_or(Error::MissingType(node))?
        {
            return self.expr(module.ok_or(Error::InvalidInstance(node))?, ctx);
        }
        Ok(match &expr.value {
            Expr::Record { fields, .. } => {
                let values = fields
                    .iter()
                    .map(|f| self.expr(f.value, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                let ty = self.ty(node, ctx)?;
                self.product(ty, &values)?
            }
            Expr::Access { record, field } | Expr::FieldOrModule { record, field, .. } => {
                let typ = self.can_type(NodeId::expr(record), ctx)?;
                let fields = self.types.fields(typ, &ctx.runtime_subst)?;
                let index = fields
                    .iter()
                    .find(|(name, ..)| *name == field.value)
                    .ok_or(Error::InvalidConstructor)?
                    .1;
                let ty = self.ty(NodeId::expr(record), ctx)?;
                let value = self.expr(record, ctx)?;
                self.field(ty, value, index, fields.len())?
            }
            Expr::Accessor(field) => {
                let typ = self.substitute(self.can_type(node, ctx)?, &ctx.runtime_subst)?;
                let Type::Lambda { from, .. } = &typ.value else {
                    return Err(Error::InvalidConstructor);
                };
                let fields = self.types.fields(from, &BTreeMap::new())?;
                let index = fields
                    .iter()
                    .find(|(name, ..)| *name == *field)
                    .ok_or(Error::InvalidConstructor)?
                    .1;
                let ty = self.types.ty(from, &BTreeMap::new())?;
                let binder = Binder {
                    name: self.ir.fresh("record"),
                    ty,
                };
                let body = self.field(ty, self.ir.var(binder.name), index, fields.len())?;
                self.ir.lam(&[binder], body)
            }
            Expr::Update { base, fields, .. } => {
                let typ = self.can_type(NodeId::expr(base), ctx)?;
                let labels = self.types.fields(typ, &ctx.runtime_subst)?;
                let ty = self.ty(NodeId::expr(base), ctx)?;
                let value = self.expr(base, ctx)?;
                let binder = Binder {
                    name: self.ir.fresh("base"),
                    ty,
                };
                let mut values = Vec::new();
                for (name, index, _) in &labels {
                    values.push(
                        if let Some(update) = fields.iter().find(|f| f.field.value == *name) {
                            self.expr(update.value, ctx)?
                        } else {
                            self.field(ty, self.ir.var(binder.name), *index, labels.len())?
                        },
                    );
                }
                let result = match ty {
                    Ty::Big(BigTy::Adt(_)) => self.big_constructor(ty, 0, &values)?,
                    _ => self.product(ty, &values)?,
                };
                self.ir.let_(binder, value, result)
            }
            _ => unreachable!("records codegen dispatch"),
        })
    }

    pub(super) fn effects_expr(
        &mut self,
        expr: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        Ok(match &expr.value {
            Expr::Trace { message, body } => {
                let body = self.expr(body, ctx)?;
                self.user_trace(Some(message), None, expr.region, body, ctx)?
            }
            Expr::Fail(message) | Expr::Todo(message) => {
                let todo = matches!(expr.value, Expr::Todo(_));
                self.user_trace(
                    *message,
                    todo.then_some("TODO: "),
                    expr.region,
                    self.ir.error(),
                    ctx,
                )?
            }
            Expr::Assert(condition) => {
                let value = self.expr(condition, ctx)?;
                let failed = self.user_trace(
                    None,
                    Some("assertion failed"),
                    expr.region,
                    self.ir.error(),
                    ctx,
                )?;
                self.ir
                    .if_(value, self.ir.lit(Constant::unit(self.ir.arena)), failed)
            }
            Expr::Comptime(value) => {
                let body = self.expr(value, ctx)?;
                let body = self.closed_dependencies(body)?;
                let body = crate::casts::expand_with_traces(
                    &self.ir,
                    &mut self.types,
                    body,
                    self.trace.compiler,
                )?;
                let constant = crate::comptime::eval_closed(self.ir.arena, &[], body)
                    .map_err(|error| Error::ComptimeAssembly(error.to_string()))?;
                self.ir.lit(constant)
            }
            _ => unreachable!("effects codegen dispatch"),
        })
    }
}
