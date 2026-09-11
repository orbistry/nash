use super::*;
use crate::{evidence, ty_of::Substitution};

impl<'a> Engine<'a, '_, '_> {
    pub(crate) fn builtin(
        &mut self,
        name: &'a str,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        use primitives::BuiltinLowering as B;
        let declaration =
            crate::builtins::definition(name).ok_or(Error::UnknownDefinition(QualifiedName {
                home: primitives::builtin_home(),
                name,
            }))?;
        if let Some(func) = crate::builtins::by_name(name) {
            return Ok(self.ir.builtin(func, &[]));
        }
        let instance = self
            .solved(ctx)
            .instances
            .get(&node)
            .ok_or(Error::InvalidInstance(node))?;
        if instance.type_args.len() != declaration.free_vars.len() {
            return Err(Error::InvalidInstance(node));
        }
        let mut subst = Substitution::new();
        for (name, typ) in declaration.free_vars.iter().zip(instance.type_args) {
            subst.insert(*name, self.substitute(typ, &ctx.runtime_subst)?);
        }
        let typ = self.substitute(declaration.typ, &subst)?;
        let Type::Lambda { from, to } = typ.value else {
            return Err(Error::InvalidInstance(node));
        };
        let from = self.types.ty(from, &Substitution::new())?;
        let to = self.types.ty(to, &Substitution::new())?;
        let binder = Binder {
            name: self.ir.fresh("value"),
            ty: from,
        };
        let value = self.ir.var(binder.name);
        let body = match declaration.lowering {
            B::Identity => value,
            B::Error => self.ir.error(),
            B::CastToData => self.ir.cast(CastKind::ToData, from, to, value),
            B::CastFromDataShallow => self.ir.cast(CastKind::FromDataShallow, from, to, value),
            B::CastValidateData => self.ir.cast(CastKind::ValidateData, from, to, value),
            B::CastLift => self.ir.cast(CastKind::Lift, from, to, value),
            B::CastLower => self.ir.cast(CastKind::Lower, from, to, value),
            B::Plutus(_) => return Err(Error::InvalidInstance(node)),
        };
        Ok(self.ir.lam(&[binder], body))
    }

    pub(crate) fn method(
        &mut self,
        trait_: QualifiedName<'a>,
        method: &'a str,
        annotation: &'a Annotation<'a>,
        node: NodeId,
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let instance = self
            .solved(ctx)
            .instances
            .get(&node)
            .ok_or(Error::InvalidInstance(node))?;
        if annotation.free_vars.len() != instance.type_args.len() {
            return Err(Error::InvalidInstance(node));
        }
        let mut subst = Substitution::new();
        for (name, typ) in annotation.free_vars.iter().zip(instance.type_args) {
            subst.insert(*name, self.substitute(typ, &ctx.subst)?);
        }
        let values = evidence::ground_arguments(
            self.ir.arena,
            &self.build.tables,
            instance.evidence,
            &ctx.subst,
            &ctx.givens,
        )?;
        self.selected_method(trait_, method, annotation, &subst, values)
    }

    /// Literal syntax calls the selected source implementation with a raw
    /// builtin constant. No nominal literal implementation is hard-coded here.
    pub(crate) fn literal(
        &mut self,
        trait_name: &'a str,
        method: &'a str,
        node: NodeId,
        raw: &'a Core<'a>,
        ctx: &Context<'a>,
        slot: usize,
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let trait_ = QualifiedName {
            home: primitives::literal_home(),
            name: trait_name,
        };
        let annotation = self.method_annotation(trait_, method)?;
        let typ = self.substitute(self.can_type(node, ctx)?, &ctx.subst)?;
        let info = self
            .build
            .tables
            .traits
            .get(&trait_)
            .ok_or(Error::MissingMethod { trait_, method })?;
        let mut subst = Substitution::new();
        if info.parameters.len() != 1 {
            return Err(Error::MethodType);
        }
        subst.insert(info.parameters[0], typ);
        let instance = self
            .solved(ctx)
            .instances
            .get(&node)
            .ok_or(Error::InvalidInstance(node))?;
        let ev = instance
            .evidence
            .get(slot)
            .ok_or(Error::InvalidInstance(node))?;
        let ev = evidence::ground(
            self.ir.arena,
            &self.build.tables,
            ev,
            &ctx.subst,
            &ctx.givens,
        )?;
        let values = self.ir.arena.alloc_slice_fill_iter([ev]);
        let function = self.selected_method(trait_, method, annotation, &subst, values)?;
        Ok(self.ir.app(function, &[raw]))
    }

    pub(crate) fn method_annotation(
        &self,
        trait_: QualifiedName<'a>,
        method: &'a str,
    ) -> Result<&'a Annotation<'a>, Error<'a>> {
        self.build
            .tables
            .traits
            .get(&trait_)
            .and_then(|info| info.methods.iter().find(|m| m.name == method))
            .map(|m| m.annotation)
            .ok_or(Error::MissingMethod { trait_, method })
    }

    pub(crate) fn selected_method(
        &mut self,
        trait_: QualifiedName<'a>,
        method: &'a str,
        annotation: &'a Annotation<'a>,
        caller_subst: &Substitution<'a>,
        caller_evidence: &'a [Evidence<'a>],
    ) -> Result<&'a Core<'a>, Error<'a>> {
        let owning = caller_evidence.first().ok_or(Error::MethodEvidence)?;
        let template = match owning {
            Evidence::ReflexiveLift { .. } if trait_ == primitives::lift_trait() => {
                let value = Binder {
                    name: self.ir.fresh("identity"),
                    ty: Ty::Erased,
                };
                return Ok(self.ir.lam(&[value], self.ir.var(value.name)));
            }
            Evidence::StructuralEq { .. } if trait_ == primitives::eq_trait() => {
                if method == "eq" {
                    return Ok(self.ir.builtin(F::EqualsData, &[]));
                }
                // A default such as neq must still execute its source body.
                *self
                    .defaults
                    .get(&(trait_, method))
                    .ok_or(Error::MissingMethod { trait_, method })?
            }
            Evidence::Impl { impl_, .. } => *self
                .methods
                .get(&(*impl_, method))
                .or_else(|| self.defaults.get(&(trait_, method)))
                .ok_or(Error::MissingMethod { trait_, method })?,
            _ => return Err(Error::MethodEvidence),
        };
        let scheme = self.scheme(template)?;
        let caller_type = self.substitute(annotation.typ, caller_subst)?;
        let mut body_subst = self.templates[template].captured.subst.clone();
        self.match_type(
            scheme.annotation.typ,
            caller_type,
            scheme.annotation.free_vars,
            &mut body_subst,
        )?;

        let mut candidates = Vec::new();
        for (pred, evidence) in annotation
            .context
            .iter()
            .filter(|p| p.trait_ref().is_some())
            .zip(caller_evidence)
        {
            candidates.push((self.predicate(*pred, caller_subst)?.key(), evidence));
        }
        if let Evidence::Impl {
            impl_,
            type_args,
            args,
        } = owning
        {
            let info = self
                .build
                .tables
                .impls
                .get(&impl_.key)
                .ok_or(Error::MethodEvidence)?;
            let subst = info
                .variables
                .iter()
                .copied()
                .zip(type_args.iter().copied())
                .collect();
            for (pred, evidence) in info
                .context
                .iter()
                .filter(|p| p.trait_ref().is_some())
                .zip(*args)
            {
                candidates.push((self.predicate(*pred, &subst)?.key(), evidence));
            }
        }
        let mut values = Vec::new();
        for pred in scheme.annotation.context {
            if pred.trait_ref().is_none() {
                continue;
            }
            let predicate = self.predicate(*pred, &body_subst)?;
            if let Some((_, value)) = candidates.iter().find(|(key, _)| *key == predicate.key()) {
                values.push(evidence::ground(
                    self.ir.arena,
                    &self.build.tables,
                    value,
                    &Substitution::new(),
                    &std::collections::HashMap::new(),
                )?);
            } else {
                let resolved = nash_solve::evidence::resolve(
                    self.ir.arena.as_bump(),
                    &self.build.tables,
                    &predicate,
                )
                .map_err(|e| Error::Resolution(e.reason))?
                .ok_or(Error::MethodEvidence)?;
                values.push(resolved);
            }
        }
        let values = self.ir.arena.alloc_slice_fill_iter(values);
        let binder = self.request(template, body_subst, values)?;
        Ok(self.ir.var(binder.name))
    }

    /// One-way matching against the selected body's own variables avoids the
    /// lexical renaming and ordering differences of impl method schemes.
    fn match_type(
        &self,
        pattern: &'a Located<Type<'a>>,
        actual: &'a Located<Type<'a>>,
        variables: &[&'a str],
        subst: &mut Substitution<'a>,
    ) -> Result<(), Error<'a>> {
        if let Type::Var(name) = pattern.value
            && variables.contains(&name)
        {
            if let Some(previous) = subst.get(name) {
                if (Evidence::StructuralEq { typ: previous })
                    != (Evidence::StructuralEq { typ: actual })
                {
                    return Err(Error::MethodType);
                }
            } else {
                subst.insert(name, actual);
            }
            return Ok(());
        }
        let opened = self.open_alias(pattern)?;
        if !std::ptr::eq(pattern, opened) {
            return self.match_type(opened, actual, variables, subst);
        }
        let actual = self.open_alias(actual)?;
        match (&pattern.value, &actual.value) {
            (Type::Var(a), Type::Var(b)) if a == b => Ok(()),
            (Type::Lambda { from: a, to: b }, Type::Lambda { from: c, to: d }) => {
                self.match_type(a, c, variables, subst)?;
                self.match_type(b, d, variables, subst)
            }
            (
                Type::Named {
                    reference: a,
                    args: aa,
                },
                Type::Named {
                    reference: b,
                    args: ba,
                },
            ) if a == b && aa.len() == ba.len() => self.match_types(aa, ba, variables, subst),
            (
                Type::Tuple {
                    first: a,
                    second: b,
                    rest: ar,
                },
                Type::Tuple {
                    first: c,
                    second: d,
                    rest: br,
                },
            ) if ar.len() == br.len() => {
                self.match_type(a, c, variables, subst)?;
                self.match_type(b, d, variables, subst)?;
                self.match_types(ar, br, variables, subst)
            }
            (Type::Record { fields: a }, Type::Record { fields: b }) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(*b) {
                    if a.field != b.field || a.index != b.index {
                        return Err(Error::MethodType);
                    }
                    self.match_type(a.typ, b.typ, variables, subst)?;
                }
                Ok(())
            }
            (Type::App { head, args }, _) => {
                let (actual_head, actual_args) = self.split_application(actual, args.len())?;
                self.match_type(head, actual_head, variables, subst)?;
                self.match_types(args, &actual_args, variables, subst)
            }
            (
                Type::Alias {
                    reference: a,
                    arguments: aa,
                    remaining: ap,
                    ..
                },
                Type::Alias {
                    reference: b,
                    arguments: ba,
                    remaining: bp,
                    ..
                },
            ) if a == b && aa.len() == ba.len() && ap == bp => {
                for (a, b) in aa.iter().zip(*ba) {
                    self.match_type(a.typ, b.typ, variables, subst)?;
                }
                Ok(())
            }
            _ => Err(Error::MethodType),
        }
    }
    fn match_types(
        &self,
        patterns: &[&'a Located<Type<'a>>],
        actual: &[&'a Located<Type<'a>>],
        variables: &[&'a str],
        subst: &mut Substitution<'a>,
    ) -> Result<(), Error<'a>> {
        if patterns.len() != actual.len() {
            return Err(Error::MethodType);
        }
        for (p, a) in patterns.iter().zip(actual) {
            self.match_type(p, a, variables, subst)?;
        }
        Ok(())
    }
    fn open_alias(&self, typ: &'a Located<Type<'a>>) -> Result<&'a Located<Type<'a>>, Error<'a>> {
        if let Type::Alias {
            arguments,
            remaining: [],
            target,
            ..
        } = &typ.value
        {
            let body = match target {
                AliasType::Open(body) | AliasType::Filled { body, .. } => *body,
            };
            if !matches!(body.value, Type::Record { .. }) {
                return self.open_alias(
                    self.substitute(body, &arguments.iter().map(|a| (a.name, a.typ)).collect())?,
                );
            }
        }
        Ok(typ)
    }
    fn split_application(
        &self,
        actual: &'a Located<Type<'a>>,
        suffix: usize,
    ) -> Result<(&'a Located<Type<'a>>, Vec<&'a Located<Type<'a>>>), Error<'a>> {
        match &actual.value {
            Type::Named { reference, args } if args.len() >= suffix => {
                let split = args.len() - suffix;
                let head = self.ir.arena.alloc(Located::at_zero(Type::Named {
                    reference: *reference,
                    args: self.ir.arena.alloc_slice_copy(&args[..split]),
                }));
                Ok((head, args[split..].to_vec()))
            }
            Type::App { head, args } if args.len() == suffix => Ok((head, args.to_vec())),
            Type::Alias {
                reference,
                arguments,
                ..
            } if arguments.len() >= suffix => {
                let split = arguments.len() - suffix;
                let prefix = arguments[..split].iter().map(|a| a.typ).collect::<Vec<_>>();
                let head = self.ir.arena.alloc(Located::at_zero(Type::Named {
                    reference: *reference,
                    args: self.ir.arena.alloc_slice_copy(&prefix),
                }));
                Ok((head, arguments[split..].iter().map(|a| a.typ).collect()))
            }
            _ => Err(Error::MethodType),
        }
    }
}
