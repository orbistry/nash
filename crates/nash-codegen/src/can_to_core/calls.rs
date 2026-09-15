use super::*;

impl<'a> Engine<'a, '_, '_> {
    pub(super) fn surface_call(
        &mut self,
        function: &'a Located<Expr<'a>>,
        arguments: &'a [CallArgument<'a>],
        direct_builtin: Option<DirectBuiltin>,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let direct_builtin = self.selected_builtin(function, direct_builtin, ctx)?;
        let order = self.solved(ctx).call_orders.get(&node).copied();
        let func = if direct_builtin.is_none() {
            Some(self.expr(function, ctx)?)
        } else {
            None
        };
        let mut args = (0..arguments.len())
            .map(|index| {
                self.expr(
                    arguments[order.map_or(index, |order| order[index])].value,
                    ctx,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(builtin) = direct_builtin {
            let selector = arguments
                .get(order.map_or(0, |order| order[0]))
                .ok_or(Error::InvalidInstance(node))?;
            let selector_ty = self.ty(NodeId::expr(selector.value), ctx)?;
            self.direct_call(builtin, &args, selector_ty, node)
        } else {
            if args.is_empty() {
                args.push(self.ir.lit(Constant::unit(self.ir.arena)));
            }
            Ok(self.ir.app(func.expect("ordinary call function"), &args))
        }
    }

    pub(super) fn pipe_call(
        &mut self,
        input: &'a Located<Expr<'a>>,
        function: &'a Located<Expr<'a>>,
        arguments: Option<&'a [CallArgument<'a>]>,
        direct_builtin: Option<DirectBuiltin>,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let direct_builtin = self.selected_builtin(function, direct_builtin, ctx)?;
        let order = self.solved(ctx).call_orders.get(&node).copied();
        let insert = *self
            .solved(ctx)
            .pipe_insertions
            .get(&node)
            .ok_or(Error::MissingType(node))?;
        let input_ty = self.ty(NodeId::expr(input), ctx)?;
        let input_value = self.expr(input, ctx)?;
        let input_binder = Binder {
            name: self.ir.fresh("pipe"),
            ty: input_ty,
        };
        let piped = self.ir.var(input_binder.name);
        let function_value = if direct_builtin.is_none() {
            Some(self.expr(function, ctx)?)
        } else {
            None
        };
        let arguments = arguments.unwrap_or(&[]);
        let mut args = Vec::with_capacity(arguments.len() + usize::from(insert));
        for index in 0..arguments.len() + usize::from(insert) {
            let index = order.map_or(index, |order| order[index]);
            args.push(if insert && index == 0 {
                piped
            } else {
                self.expr(arguments[index - usize::from(insert)].value, ctx)?
            });
        }
        let called = if let Some(builtin) = direct_builtin {
            let first = order.map_or(0, |order| order[0]);
            let selector_ty = if insert && first == 0 {
                input_ty
            } else {
                self.ty(
                    NodeId::expr(
                        arguments
                            .get(first - usize::from(insert))
                            .ok_or(Error::InvalidInstance(node))?
                            .value,
                    ),
                    ctx,
                )?
            };
            self.direct_call(builtin, &args, selector_ty, node)?
        } else {
            if args.is_empty() {
                args.push(self.ir.lit(Constant::unit(self.ir.arena)));
            }
            self.ir
                .app(function_value.expect("ordinary pipe function"), &args)
        };
        let body = if insert {
            called
        } else {
            self.ir.app(called, &[piped])
        };
        Ok(self.ir.let_(input_binder, input_value, body))
    }

    fn selected_builtin(
        &self,
        function: &Located<Expr<'a>>,
        builtin: Option<DirectBuiltin>,
        ctx: &Context<'a>,
    ) -> Result<Option<DirectBuiltin>, Error<'a>> {
        if matches!(function.value, Expr::FieldOrModule { .. }) {
            let node = NodeId::expr(function);
            if self
                .solved(ctx)
                .field_selections
                .get(&node)
                .copied()
                .ok_or(Error::MissingType(node))?
            {
                return Ok(None);
            }
        }
        Ok(builtin)
    }

    fn direct_call(
        &self,
        builtin: DirectBuiltin,
        args: &[&'a Core<'a>],
        selector_ty: Ty<'a>,
        node: NodeId,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        Ok(match (builtin, args) {
            (DirectBuiltin::IfThenElse, [condition, yes, no]) => self.ir.if_(condition, yes, no),
            (DirectBuiltin::ChooseList, [list, empty, nonempty]) => self.ir.force(self.ir.builtin(
                F::ChooseList,
                &[list, self.ir.delay(empty), self.ir.delay(nonempty)],
            )),
            (DirectBuiltin::ChooseData, [data, constr, map, list, int, bytes]) => {
                self.ir.force(self.ir.builtin(
                    F::ChooseData,
                    &[
                        data,
                        self.ir.delay(constr),
                        self.ir.delay(map),
                        self.ir.delay(list),
                        self.ir.delay(int),
                        self.ir.delay(bytes),
                    ],
                ))
            }
            (DirectBuiltin::ChooseUnit, [selector, body]) => self.ir.let_(
                Binder {
                    name: self.ir.fresh("ignored"),
                    ty: selector_ty,
                },
                selector,
                body,
            ),
            (DirectBuiltin::Trace, [message, body]) => self.ir.trace(message, body),
            _ => return Err(Error::InvalidInstance(node)),
        })
    }
}
