use super::*;
use crate::{
    decision_tree::{self, MatchBranch, MatchInputs, RecordFields},
    evidence,
    ty_of::Substitution,
};
use std::collections::HashMap;

impl<'a> Engine<'a, '_, '_> {
    pub(super) fn lambda(
        &mut self,
        parameters: &[&'a Located<Pattern<'a>>],
        body: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        self.lambda_with_policy(parameters, body, ctx, false)
    }

    fn lambda_with_policy(
        &mut self,
        parameters: &[&'a Located<Pattern<'a>>],
        body: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
        source_let: bool,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        if parameters.is_empty() {
            return self.expr(body, ctx);
        }
        let mut child = ctx.clone();
        let mut params = Vec::new();
        let mut matches = Vec::new();
        for &pattern in parameters {
            let ty = self.ty(NodeId::pattern(pattern), ctx)?;
            if let Pattern::Var(name) = pattern.value {
                let binder = Binder {
                    name: self.ir.fresh(name),
                    ty,
                };
                child.env.insert(name, Binding::Value(binder));
                params.push(binder);
                continue;
            }
            let binder = Binder {
                name: self.ir.fresh("arg"),
                ty,
            };
            let (records, literals) = self.pattern_inputs(pattern, ctx)?;
            let bindings =
                decision_tree::bindings(&self.ir, &mut self.types, ty, pattern, &records)?;
            for (name, value) in &bindings {
                child.env.insert(name, Binding::Value(*value));
            }
            params.push(binder);
            matches.push((pattern, binder, bindings, records, literals));
        }
        let mut body = self.expr(body, &child)?;
        for (pattern, binder, bindings, records, literals) in matches.into_iter().rev() {
            let fallback = self.match_failure();
            let continuation = body;
            body = decision_tree::compile(
                &self.ir,
                &mut self.types,
                binder.ty,
                self.ir.var(binder.name),
                &[MatchBranch {
                    pattern,
                    bindings,
                    body,
                }],
                MatchInputs {
                    record_fields: &records,
                    literal_tests: &literals,
                },
                fallback,
            )?;
            if source_let {
                crate::build::source_lets::mark_pattern_bindings(
                    body,
                    continuation,
                    &mut self.source_lets,
                );
            }
        }
        if source_let {
            self.source_lets
                .extend(params.iter().map(|param| param.name.unique));
        }
        Ok(self.ir.lam(&params, body))
    }

    pub(super) fn source_let(
        &mut self,
        pattern: &'a Located<Pattern<'a>>,
        value: &'a Located<Expr<'a>>,
        body: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let function = self.lambda_with_policy(&[pattern], body, ctx, true)?;
        let Core::Lam {
            params: [binder],
            body,
        } = function
        else {
            unreachable!("one source binding produces exactly one runtime parameter")
        };
        let value = self.expr(value, ctx)?;
        Ok(self.ir.let_(*binder, value, body))
    }

    pub(super) fn no_inline_function(&mut self, function: &'a Core<'a>) -> &'a Core<'a> {
        if let Core::Lam { params, .. } = function {
            self.no_inline
                .extend(params.iter().map(|param| param.name.unique));
        }
        function
    }

    pub(super) fn case(
        &mut self,
        scrutinee: &'a Located<Expr<'a>>,
        branches: &'a [can::CaseBranch<'a>],
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let ty = self.ty(NodeId::expr(scrutinee), ctx)?;
        let value = self.expr(scrutinee, ctx)?;
        let mut rows = Vec::new();
        let mut records = RecordFields::new();
        let mut literals = HashMap::new();
        for branch in branches {
            let (r, l) = self.pattern_inputs(branch.pattern, ctx)?;
            records.extend(r);
            literals.extend(l);
            let bindings =
                decision_tree::bindings(&self.ir, &mut self.types, ty, branch.pattern, &records)?;
            let mut child = ctx.clone();
            for (name, binder) in &bindings {
                child.env.insert(name, Binding::Value(*binder));
            }
            let body = self.expr(branch.body, &child)?;
            rows.push(MatchBranch {
                pattern: branch.pattern,
                bindings,
                body,
            });
        }
        let fallback = self.match_failure();
        Ok(decision_tree::compile(
            &self.ir,
            &mut self.types,
            ty,
            value,
            &rows,
            MatchInputs {
                record_fields: &records,
                literal_tests: &literals,
            },
            fallback,
        )?)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn refutable_match(
        &mut self,
        node: NodeId,
        value: &'a Located<Expr<'a>>,
        pattern: &'a Located<Pattern<'a>>,
        body: &'a Located<Expr<'a>>,
        fallback: &'a Located<Expr<'a>>,
        site: Option<ConversionSite>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let target = self.ty(NodeId::pattern(pattern), ctx)?;
        if target == DATA
            && matches!(pattern.value, Pattern::Anything)
            && matches!(
                site,
                Some(
                    ConversionSite::ValidatorParameter
                        | ConversionSite::ValidatorRedeemer
                        | ConversionSite::ValidatorContext
                )
            )
        {
            return self.expr(body, ctx);
        }
        let source = self.ty(NodeId::expr(value), ctx)?;
        let kind = if site.is_some() {
            *self
                .solved(ctx)
                .conversions
                .get(&node)
                .ok_or(Error::MissingType(node))?
        } else {
            ConversionKind::Identity
        };
        let value = self.expr(value, ctx)?;
        let input = Binder {
            name: self.ir.fresh("matched"),
            ty: source,
        };
        let input_value = self.ir.var(input.name);
        let refuting_cast = kind == ConversionKind::ValidateData
            && matches!(
                site,
                Some(ConversionSite::CastTest | ConversionSite::CastPattern)
            );
        let converted = match kind {
            ConversionKind::Identity | ConversionKind::ViewData => input_value,
            ConversionKind::ToData => self.ir.cast(CastKind::ToData, source, target, input_value),
            ConversionKind::FromDataShallow => {
                self.ir
                    .cast(CastKind::FromDataShallow, source, target, input_value)
            }
            ConversionKind::FromDataBytesView => self.ir.builtin(F::UnBData, &[input_value]),
            ConversionKind::ValidateData => self.ir.cast(
                if refuting_cast {
                    CastKind::FromDataShallow
                } else {
                    CastKind::ValidateData
                },
                source,
                target,
                input_value,
            ),
            ConversionKind::Ascription(_) => return Err(Error::InvalidInstance(node)),
        };
        let converted_binding = Binder {
            name: self.ir.fresh("decoded"),
            ty: target,
        };
        let (records, literals) = self.pattern_inputs(pattern, ctx)?;
        let bindings =
            decision_tree::bindings(&self.ir, &mut self.types, target, pattern, &records)?;
        let mut child = ctx.clone();
        for (name, binder) in &bindings {
            child.env.insert(name, Binding::Value(*binder));
        }
        let body = self.expr(body, &child)?;
        let fallback = self.expr(fallback, ctx)?;
        let matched = decision_tree::compile(
            &self.ir,
            &mut self.types,
            target,
            self.ir.var(converted_binding.name),
            &[MatchBranch {
                pattern,
                bindings,
                body,
            }],
            MatchInputs {
                record_fields: &records,
                literal_tests: &literals,
            },
            fallback,
        )?;
        let matched = self.ir.let_(converted_binding, converted, matched);
        let matched = if refuting_cast {
            let valid = crate::casts::data_matches(&self.ir, &mut self.types, target, input_value)?;
            self.ir.if_(valid, matched, fallback)
        } else {
            matched
        };
        Ok(self.ir.let_(input, value, matched))
    }

    fn pattern_inputs(
        &mut self,
        pattern: &'a Located<Pattern<'a>>,
        ctx: &Context<'a>,
    ) -> Result<(RecordFields<'a>, HashMap<NodeId, &'a Core<'a>>), Error<'a>> {
        let mut records = RecordFields::new();
        let mut literals = HashMap::new();
        let mut pending = vec![pattern];
        while let Some(pattern) = pending.pop() {
            let node = NodeId::pattern(pattern);
            match &pattern.value {
                Pattern::Record(_) => {
                    let typ = self.can_type(node, ctx)?;
                    let fields = self.types.fields(typ, &ctx.runtime_subst)?;
                    records.insert(
                        node,
                        self.ir.arena.alloc_slice_copy(
                            &fields
                                .into_iter()
                                .map(|(name, ..)| name)
                                .collect::<Vec<_>>(),
                        ),
                    );
                }
                Pattern::Alias { pattern, .. } => pending.push(pattern),
                Pattern::Pair { first, second } => {
                    pending.push(first);
                    pending.push(second);
                }
                Pattern::DataList { elements, tail } => {
                    pending.extend_from_slice(elements);
                    pending.extend(tail.iter().copied());
                }
                Pattern::DataTuple {
                    first,
                    second,
                    rest,
                }
                | Pattern::Tuple {
                    first,
                    second,
                    rest,
                } => {
                    pending.push(first);
                    pending.push(second);
                    pending.extend_from_slice(rest);
                }
                Pattern::Cons { head, tail } => {
                    pending.push(head);
                    pending.push(tail);
                }
                Pattern::List(items) => pending.extend_from_slice(items),
                Pattern::Constructor(ctor) => {
                    for arg in ctor.arguments {
                        pending.push(arg.pattern);
                    }
                }
                Pattern::Int(_) | Pattern::Str(_) | Pattern::Bytes(_) => {
                    literals.insert(node, self.literal_pattern(pattern, ctx)?);
                }
                Pattern::Constant(_)
                | Pattern::Anything
                | Pattern::Var(_)
                | Pattern::Unit
                | Pattern::Bool { .. } => {}
            }
        }
        Ok((records, literals))
    }
    fn literal_pattern(
        &mut self,
        pattern: &'a Located<Pattern<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let node = NodeId::pattern(pattern);
        let (trait_name, method, raw) = match pattern.value {
            Pattern::Int(n) => ("FromInt", "fromInt", self.ir.int(n)),
            Pattern::Str(s) => (
                "FromString",
                "fromString",
                self.ir.lit(Constant::string(self.ir.arena, s)),
            ),
            Pattern::Bytes(b) => (
                "FromBytes",
                "fromBytes",
                self.ir.lit(Constant::byte_string(self.ir.arena, b)),
            ),
            _ => return Err(Error::InvalidInstance(node)),
        };
        let literal = self.literal(trait_name, method, node, raw, ctx, 0)?;
        let typ = self.substitute(self.can_type(node, ctx)?, &ctx.subst)?;
        let trait_ = self.build.inputs[ctx.input]
            .tables
            .core_trait(primitives::eq_trait());
        let annotation = self.method_annotation(trait_, "eq")?;
        let info = self
            .build
            .tables
            .traits
            .get(&trait_)
            .ok_or(Error::MethodEvidence)?;
        if info.parameters.len() != 1 {
            return Err(Error::MethodType);
        }
        let subst = Substitution::from([(info.parameters[0], typ)]);
        let instance = self
            .solved(ctx)
            .instances
            .get(&node)
            .ok_or(Error::InvalidInstance(node))?;
        let eq = instance
            .evidence
            .get(1)
            .ok_or(Error::InvalidInstance(node))?;
        let eq = evidence::ground(
            self.ir.arena,
            &self.build.tables,
            eq,
            &ctx.subst,
            &ctx.givens,
        )?;
        let eq = self.ir.arena.alloc_slice_fill_iter([eq]);
        let function = self.selected_method(trait_, "eq", annotation, &subst, eq)?;
        let value = Binder {
            name: self.ir.fresh("literal"),
            ty: self.ty(node, ctx)?,
        };
        Ok(self.ir.lam(
            &[value],
            self.ir.app(function, &[self.ir.var(value.name), literal]),
        ))
    }

    pub(super) fn let_destruct(
        &mut self,
        pattern: &'a Located<Pattern<'a>>,
        value: &'a Located<Expr<'a>>,
        body: &'a Located<Expr<'a>>,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let group = self.add_group();
        let id = self.add_template(Source::Destruct { pattern, value }, ctx.clone(), group);
        let mut child = ctx.clone();
        for name in bound_names(pattern) {
            child.env.insert(
                name,
                Binding::Template {
                    id,
                    projection: Some(name),
                },
            );
        }
        if self.eager_template(id)? {
            let evidence = self.resolve_context(self.scheme(id)?.annotation.context, &ctx.subst)?;
            self.request(id, ctx.subst.clone(), evidence)?;
        }
        let body = self.expr(body, &child)?;
        self.drain(group)?;
        self.emit_group(group, body, false)
    }
    pub(super) fn destruct_projection(
        &mut self,
        id: usize,
        name: &'a str,
        binder: Binder<'a>,
        _node: NodeId,
        _ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let Source::Destruct { pattern, .. } = self.templates[id].source else {
            return Err(Error::InvalidConstructor);
        };
        let ctx = self
            .instance_context(binder.name)
            .ok_or(Error::InvalidConstructor)?
            .clone();
        let (records, literals) = self.pattern_inputs(pattern, &ctx)?;
        let bindings =
            decision_tree::bindings(&self.ir, &mut self.types, binder.ty, pattern, &records)?;
        let result = self
            .ir
            .var(bindings.get(name).ok_or(Error::UnknownLocal(name))?.name);
        let fallback = self.match_failure();
        Ok(decision_tree::compile(
            &self.ir,
            &mut self.types,
            binder.ty,
            self.ir.var(binder.name),
            &[MatchBranch {
                pattern,
                bindings,
                body: result,
            }],
            MatchInputs {
                record_fields: &records,
                literal_tests: &literals,
            },
            fallback,
        )?)
    }
}

fn bound_names<'a>(pattern: &'a Located<Pattern<'a>>) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut pending = vec![pattern];
    while let Some(pattern) = pending.pop() {
        match &pattern.value {
            Pattern::Var(name) => result.push(*name),
            Pattern::Record(names) => result.extend_from_slice(names),
            Pattern::Alias { pattern, name } => {
                pending.push(pattern);
                result.push(*name);
            }
            Pattern::Pair { first, second } => {
                pending.push(first);
                pending.push(second);
            }
            Pattern::DataList { elements, tail } => {
                pending.extend_from_slice(elements);
                pending.extend(tail.iter().copied());
            }
            Pattern::DataTuple {
                first,
                second,
                rest,
            }
            | Pattern::Tuple {
                first,
                second,
                rest,
            } => {
                pending.push(first);
                pending.push(second);
                pending.extend_from_slice(rest);
            }
            Pattern::List(ps) => pending.extend_from_slice(ps),
            Pattern::Cons { head, tail } => {
                pending.push(head);
                pending.push(tail);
            }
            Pattern::Constructor(ctor) => {
                for arg in ctor.arguments {
                    pending.push(arg.pattern);
                }
            }
            _ => {}
        }
    }
    result
}
