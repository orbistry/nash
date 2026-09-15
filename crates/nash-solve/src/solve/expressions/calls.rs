use super::*;

impl<'a> Solver<'a, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_surface_call(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        function: &Located<CanExpr<'a>>,
        arguments: &[nash_ast::CallArgument<'a>],
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let function_var = self.fresh(uf, rank);
        state = self.infer_expr(
            uf,
            env,
            rank,
            state,
            rtv,
            function,
            Expected::NoExpectation(function_var),
        );
        self.infer_call_group(
            uf,
            env,
            rank,
            state,
            rtv,
            expr.region,
            Some(NodeId::expr(expr)),
            function,
            function_var,
            arguments,
            None,
            expected,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_pipe(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        input: &Located<CanExpr<'a>>,
        function: &Located<CanExpr<'a>>,
        arguments: Option<&[nash_ast::CallArgument<'a>]>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let input_var = self.fresh(uf, rank);
        state = self.infer_expr(
            uf,
            env,
            rank,
            state,
            rtv,
            input,
            Expected::NoExpectation(input_var),
        );
        let function_var = self.fresh(uf, rank);
        state = self.infer_expr(
            uf,
            env,
            rank,
            state,
            rtv,
            function,
            Expected::NoExpectation(function_var),
        );
        let mut allocated = Vec::new();
        let resolved =
            crate::representation::subject(uf, &self.tables.kinds, function_var, &mut allocated);
        self.introduce(uf, rank, &allocated);
        let insert = arguments.is_none()
            || matches!(&uf.get(resolved).content,
            Content::Structure(FlatType::Function1(parameters, _)) if arguments.is_some_and(|args| args.len() < parameters.len()));
        self.pipe_insertions.insert(NodeId::expr(expr), insert);
        if insert {
            self.infer_call_group(
                uf,
                env,
                rank,
                state,
                rtv,
                expr.region,
                Some(NodeId::expr(expr)),
                function,
                function_var,
                arguments.unwrap_or(&[]),
                Some(input_var),
                expected,
            )
        } else {
            let result = self.fresh(uf, rank);
            state = self.infer_call_group(
                uf,
                env,
                rank,
                state,
                rtv,
                expr.region,
                Some(NodeId::expr(expr)),
                function,
                function_var,
                arguments.unwrap_or(&[]),
                None,
                Expected::NoExpectation(result),
            );
            self.infer_call_group(
                uf,
                env,
                rank,
                state,
                rtv,
                expr.region,
                None,
                function,
                result,
                &[],
                Some(input_var),
                expected,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_call_group(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        region: nash_region::Region,
        node: Option<NodeId>,
        function: &Located<CanExpr<'a>>,
        function_var: Variable,
        arguments: &[nash_ast::CallArgument<'a>],
        input: Option<Variable>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let name = direct_get_name(function);
        let order = match node
            .map(|node| self.call_order(node, function, arguments, input.is_some()))
            .transpose()
        {
            Ok(order) => order.flatten(),
            Err(reason) => return add_error(state, Error::InvalidCall { region, reason }),
        };
        let result = self.fresh(uf, rank);
        let offset = usize::from(input.is_some());
        let mut args = Vec::with_capacity(arguments.len() + offset);
        args.extend(input);
        args.extend(arguments.iter().map(|_| self.fresh(uf, rank)));
        if let Some(order) = order {
            args = order.iter().map(|index| args[*index]).collect();
        }
        let arity = args.len();
        let signature = self.structure(uf, rank, FlatType::Function1(args.clone(), result));
        state = self.equal(
            uf,
            rank,
            state,
            function.region,
            Category::CallResult(name),
            function_var,
            Expected::FromContext(region, Context::CallArity(name, arity), signature),
        );
        for (index, variable) in args.into_iter().enumerate() {
            let source_index = order.map_or(index, |order| order[index]);
            if source_index < offset {
                continue;
            }
            let argument = &arguments[source_index - offset];
            state = self.infer_expr(
                uf,
                env,
                rank,
                state,
                rtv,
                argument.value,
                Expected::FromContext(region, Context::CallArg(name, index), variable),
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

    fn call_order(
        &mut self,
        node: NodeId,
        function: &Located<CanExpr<'a>>,
        arguments: &[nash_ast::CallArgument<'a>],
        input: bool,
    ) -> Result<Option<&'a [usize]>, &'a str> {
        let CanExpr::FieldOrModule { module_labels, .. } = &function.value else {
            return Ok(None);
        };
        if arguments.iter().all(|argument| argument.label.is_none()) {
            return Ok(None);
        }
        let use_field = self
            .field_selections
            .get(&NodeId::expr(function))
            .copied()
            .expect("field/module selection is inferred before its call");
        let labels = if use_field { None } else { *module_labels };
        let declaration = labels.map(|labels| (labels.len(), Some(labels)));
        let mut arguments = arguments.to_vec();
        if input {
            // Only labels and positions are inspected by the shared ordering
            // policy; this placeholder denotes the already-inferred pipe input.
            let value = arguments[0].value;
            arguments.insert(0, nash_ast::CallArgument { label: None, value });
        }
        let mut order: Vec<_> = (0..arguments.len()).collect();
        nash_can::expression::reorder_arguments(
            self.bump,
            declaration,
            &mut arguments,
            |left, right| order.swap(left, right),
        )?;
        let order = &*self.bump.alloc_slice_copy(&order);
        self.call_orders.insert(node, order);
        Ok(Some(order))
    }
}
