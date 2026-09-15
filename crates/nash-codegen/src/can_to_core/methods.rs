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
        let mut subst = Substitution::new();
        if !declaration.free_vars.is_empty() {
            let instance = self
                .solved(ctx)
                .instances
                .get(&node)
                .ok_or(Error::InvalidInstance(node))?;
            if instance.type_args.len() != declaration.free_vars.len() {
                return Err(Error::InvalidInstance(node));
            }
            for (name, typ) in declaration.free_vars.iter().zip(instance.type_args) {
                subst.insert(*name, self.substitute(typ, &ctx.runtime_subst)?);
            }
        }
        let mut scheme = declaration.typ;
        let mut binders = Vec::new();
        loop {
            let (arguments, result): (&[&Located<Type<'a>>], _) = match &scheme.value {
                Type::Lambda { from, to } => (std::slice::from_ref(from), *to),
                Type::Function { arguments, result } => (arguments, *result),
                _ => break,
            };
            for from in arguments {
                let from = self.substitute(from, &subst)?;
                binders.push(Binder {
                    name: self.ir.fresh("value"),
                    ty: self.types.ty(from, &Substitution::new())?,
                });
            }
            scheme = result;
        }
        let result = self.substitute(scheme, &subst)?;
        let result = self.types.ty(result, &Substitution::new())?;
        let args: Vec<_> = binders.iter().map(|b| self.ir.var(b.name)).collect();
        let value = args[0];
        let from = binders[0].ty;
        let body = match declaration.lowering {
            B::Identity => value,
            B::Error => self.ir.error(),
            B::CastToData => self.ir.cast(CastKind::ToData, from, result, value),
            B::CastFromDataShallow => self.ir.cast(CastKind::FromDataShallow, from, result, value),
            B::CastValidateData => self.ir.cast(CastKind::ValidateData, from, result, value),
            B::CastLift => self.ir.cast(CastKind::Lift, from, result, value),
            B::CastLower => self.ir.cast(CastKind::Lower, from, result, value),
            B::DataListHead => {
                let head = self.ir.builtin(F::HeadList, &[value]);
                self.decode_list_item(result, head)
            }
            B::DataListCons => {
                let head = if matches!(from, Ty::Const(ConstTy::DataPair(..))) {
                    value
                } else {
                    self.ir.cast(CastKind::ToData, from, DATA, value)
                };
                self.ir.builtin(F::MkCons, &[head, args[1]])
            }
            B::DataPairFirst | B::DataPairSecond => {
                let func = if declaration.lowering == B::DataPairFirst {
                    F::FstPair
                } else {
                    F::SndPair
                };
                self.ir.cast(
                    CastKind::FromDataShallow,
                    DATA,
                    result,
                    self.ir.builtin(func, &[value]),
                )
            }
            B::DataUnConstr => {
                let raw = Binder {
                    name: self.ir.fresh("constr"),
                    ty: Ty::Const(self.ir.arena.alloc(ConstTy::Pair(
                        Ty::Const(&ConstTy::Int),
                        Ty::Const(self.ir.arena.alloc(ConstTy::List(DATA))),
                    ))),
                };
                let pair = self.ir.var(raw.name);
                let index = self
                    .ir
                    .builtin(F::IData, &[self.ir.builtin(F::FstPair, &[pair])]);
                let fields = self
                    .ir
                    .builtin(F::ListData, &[self.ir.builtin(F::SndPair, &[pair])]);
                self.ir.let_(
                    raw,
                    self.ir.builtin(F::UnConstrData, &[value]),
                    self.ir.builtin(F::MkPairData, &[index, fields]),
                )
            }
            B::ConstrIndex | B::ConstrFields => {
                let func = if declaration.lowering == B::ConstrIndex {
                    F::FstPair
                } else {
                    F::SndPair
                };
                self.ir
                    .builtin(func, &[self.ir.builtin(F::UnConstrData, &[value])])
            }
            B::DataWriteBits => {
                let indices = self.decode_integer_list(args[1]);
                self.ir.builtin(F::WriteBits, &[value, indices, args[2]])
            }
            B::ChooseValue => args[1],
            B::Not => self.ir.if_(
                value,
                self.ir.lit(Constant::bool(self.ir.arena, false)),
                self.ir.lit(Constant::bool(self.ir.arena, true)),
            ),
            B::Always => value,
            B::Flip => self.ir.app(value, &[args[2], args[1]]),
            // Pinned prelude typing advertises Void, but its implementation
            // returns the Boolean result of reflexive equality.
            B::Tautology => self.ir.lit(Constant::bool(self.ir.arena, true)),
            B::Enumerate => return Ok(self.enumerate_builtin(&binders, result)),
            B::EncodeBase16 => return Ok(self.encode_base16_builtin()),
            B::FromInt => return Ok(self.integer_decimal_builtin(false)),
            B::DoFromInt => return Ok(self.integer_decimal_builtin(true)),
            B::Diagnostic => return Ok(self.diagnostic_builtin()),
            B::Plutus(_) => return Err(Error::InvalidInstance(node)),
        };
        Ok(self.ir.lam(&binders, body))
    }

    fn decode_list_item(&self, typ: Ty<'a>, item: &'a Core<'a>) -> &'a Core<'a> {
        if matches!(typ, Ty::Const(ConstTy::DataPair(..))) {
            item
        } else {
            self.ir.cast(CastKind::FromDataShallow, DATA, typ, item)
        }
    }

    fn decode_integer_list(&self, value: &'a Core<'a>) -> &'a Core<'a> {
        let list = Ty::Const(
            self.ir
                .arena
                .alloc(ConstTy::DataList(Ty::Const(&ConstTy::Int))),
        );
        let output = Ty::Const(self.ir.arena.alloc(ConstTy::List(Ty::Const(&ConstTy::Int))));
        let arg = Binder {
            name: self.ir.fresh("indices"),
            ty: list,
        };
        let function = Binder {
            name: self.ir.fresh("decodeIndices"),
            ty: Ty::Term(
                self.ir
                    .arena
                    .alloc(TermTy::Fun(self.ir.arena.alloc_slice_copy(&[list]), output)),
            ),
        };
        let xs = self.ir.var(arg.name);
        let empty = self.ir.lit(Constant::proto_list(
            self.ir.arena,
            nash_plutus::typ::Type::integer(self.ir.arena),
            &[],
        ));
        let body = self.ir.if_(
            self.ir.builtin(F::NullList, &[xs]),
            empty,
            self.ir.builtin(
                F::MkCons,
                &[
                    self.ir
                        .builtin(F::UnIData, &[self.ir.builtin(F::HeadList, &[xs])]),
                    self.ir.app(
                        self.ir.var(function.name),
                        &[self.ir.builtin(F::TailList, &[xs])],
                    ),
                ],
            ),
        );
        self.ir.let_rec(
            &[RecBinder {
                binder: function,
                params: self.ir.arena.alloc_slice_copy(&[arg]),
                static_params: &[],
                body,
            }],
            self.ir.app(self.ir.var(function.name), &[value]),
        )
    }

    fn builtin_function(&self, name: &'static str, args: &[Ty<'a>], result: Ty<'a>) -> Binder<'a> {
        Binder {
            name: self.ir.fresh(name),
            ty: Ty::Term(
                self.ir
                    .arena
                    .alloc(TermTy::Fun(self.ir.arena.alloc_slice_copy(args), result)),
            ),
        }
    }

    fn builtin_bytes(&self, bytes: &'static [u8]) -> &'a Core<'a> {
        self.ir.lit(Constant::byte_string(self.ir.arena, bytes))
    }

    fn enumerate_builtin(&self, params: &[Binder<'a>], result: Ty<'a>) -> &'a Core<'a> {
        let Ty::Const(ConstTy::DataList(item)) = params[0].ty else {
            unreachable!("enumerate has an encoded-list scheme");
        };
        let function = self.builtin_function(
            "enumerate",
            &params.iter().map(|b| b.ty).collect::<Vec<_>>(),
            result,
        );
        let args: Vec<_> = params.iter().map(|b| self.ir.var(b.name)).collect();
        let tail = Binder {
            name: self.ir.fresh("tail"),
            ty: params[0].ty,
        };
        let xs = self.ir.var(tail.name);
        let head = self.decode_list_item(*item, self.ir.builtin(F::HeadList, &[args[0]]));
        let step = self.ir.if_(
            self.ir.builtin(F::NullList, &[xs]),
            self.ir.app(args[3], &[head, args[1]]),
            self.ir.app(
                args[2],
                &[
                    head,
                    self.ir
                        .app(self.ir.var(function.name), &[xs, args[1], args[2], args[3]]),
                ],
            ),
        );
        let body = self.ir.if_(
            self.ir.builtin(F::NullList, &[args[0]]),
            args[1],
            self.ir
                .let_(tail, self.ir.builtin(F::TailList, &[args[0]]), step),
        );
        self.ir.let_rec(
            &[RecBinder {
                binder: function,
                params: self.ir.arena.alloc_slice_copy(params),
                static_params: &[1, 2, 3],
                body,
            }],
            self.ir.var(function.name),
        )
    }

    fn encode_base16_builtin(&self) -> &'a Core<'a> {
        let bytes = Ty::Const(&ConstTy::Bytes);
        let int = Ty::Const(&ConstTy::Int);
        let function = self.builtin_function("encodeBase16", &[bytes, int, bytes], bytes);
        let input = Binder {
            name: self.ir.fresh("bytes"),
            ty: bytes,
        };
        let index = Binder {
            name: self.ir.fresh("index"),
            ty: int,
        };
        let builder = Binder {
            name: self.ir.fresh("builder"),
            ty: bytes,
        };
        let byte = Binder {
            name: self.ir.fresh("byte"),
            ty: int,
        };
        let digit = |n| {
            self.ir.builtin(
                F::AddInteger,
                &[
                    n,
                    self.ir.if_(
                        self.ir.builtin(F::LessThanInteger, &[n, self.ir.int(10)]),
                        self.ir.int(48),
                        self.ir.int(55),
                    ),
                ],
            )
        };
        let high = self
            .ir
            .builtin(F::DivideInteger, &[self.ir.var(byte.name), self.ir.int(16)]);
        let low = self
            .ir
            .builtin(F::ModInteger, &[self.ir.var(byte.name), self.ir.int(16)]);
        let next = self.ir.builtin(
            F::ConsByteString,
            &[
                digit(high),
                self.ir
                    .builtin(F::ConsByteString, &[digit(low), self.ir.var(builder.name)]),
            ],
        );
        let body = self.ir.if_(
            self.ir.builtin(
                F::LessThanInteger,
                &[self.ir.var(index.name), self.ir.int(0)],
            ),
            self.ir.var(builder.name),
            self.ir.let_(
                byte,
                self.ir.builtin(
                    F::IndexByteString,
                    &[self.ir.var(input.name), self.ir.var(index.name)],
                ),
                self.ir.app(
                    self.ir.var(function.name),
                    &[
                        self.ir.var(input.name),
                        self.ir.builtin(
                            F::SubtractInteger,
                            &[self.ir.var(index.name), self.ir.int(1)],
                        ),
                        next,
                    ],
                ),
            ),
        );
        self.ir.let_rec(
            &[RecBinder {
                binder: function,
                params: self.ir.arena.alloc_slice_copy(&[input, index, builder]),
                static_params: &[0],
                body,
            }],
            self.ir.var(function.name),
        )
    }

    fn integer_decimal_builtin(&self, positive_only: bool) -> &'a Core<'a> {
        let bytes = Ty::Const(&ConstTy::Bytes);
        let int = Ty::Const(&ConstTy::Int);
        let function = self.builtin_function("decimalDigits", &[int, bytes], bytes);
        let number = Binder {
            name: self.ir.fresh("number"),
            ty: int,
        };
        let builder = Binder {
            name: self.ir.fresh("builder"),
            ty: bytes,
        };
        let n = self.ir.var(number.name);
        let suffix = self.ir.var(builder.name);
        let next = self.ir.builtin(
            F::ConsByteString,
            &[
                self.ir.builtin(
                    F::AddInteger,
                    &[
                        self.ir.builtin(F::RemainderInteger, &[n, self.ir.int(10)]),
                        self.ir.int(48),
                    ],
                ),
                suffix,
            ],
        );
        let body = self.ir.if_(
            self.ir
                .builtin(F::LessThanEqualsInteger, &[n, self.ir.int(0)]),
            suffix,
            self.ir.app(
                self.ir.var(function.name),
                &[
                    self.ir.builtin(F::QuotientInteger, &[n, self.ir.int(10)]),
                    next,
                ],
            ),
        );
        let result = if positive_only {
            self.ir.var(function.name)
        } else {
            let number = Binder {
                name: self.ir.fresh("signedNumber"),
                ty: int,
            };
            let builder = Binder {
                name: self.ir.fresh("suffix"),
                ty: bytes,
            };
            let n = self.ir.var(number.name);
            let suffix = self.ir.var(builder.name);
            let rendered = self.ir.app(
                self.ir.var(function.name),
                &[
                    self.ir.builtin(F::SubtractInteger, &[self.ir.int(0), n]),
                    suffix,
                ],
            );
            self.ir.lam(
                &[number, builder],
                self.ir.if_(
                    self.ir.builtin(F::EqualsInteger, &[n, self.ir.int(0)]),
                    self.ir
                        .builtin(F::AppendByteString, &[self.builtin_bytes(b"0"), suffix]),
                    self.ir.if_(
                        self.ir.builtin(F::LessThanInteger, &[n, self.ir.int(0)]),
                        self.ir
                            .builtin(F::AppendByteString, &[self.builtin_bytes(b"-"), rendered]),
                        self.ir.app(self.ir.var(function.name), &[n, suffix]),
                    ),
                ),
            )
        };
        self.ir.let_rec(
            &[RecBinder {
                binder: function,
                params: self.ir.arena.alloc_slice_copy(&[number, builder]),
                static_params: &[],
                body,
            }],
            result,
        )
    }

    fn diagnostic_sequence(
        &self,
        diagnostic: &'a Core<'a>,
        list: &'a Core<'a>,
        suffix: &'a Core<'a>,
        pairs: bool,
    ) -> &'a Core<'a> {
        let bytes = Ty::Const(&ConstTy::Bytes);
        let item = if pairs {
            Ty::Const(self.ir.arena.alloc(ConstTy::DataPair(DATA, DATA)))
        } else {
            DATA
        };
        let list_ty = Ty::Const(self.ir.arena.alloc(ConstTy::DataList(item)));
        let function = self.builtin_function("diagnosticSequence", &[list_ty, bytes], bytes);
        let items = Binder {
            name: self.ir.fresh("items"),
            ty: list_ty,
        };
        let builder = Binder {
            name: self.ir.fresh("builder"),
            ty: bytes,
        };
        let tail = Binder {
            name: self.ir.fresh("tail"),
            ty: list_ty,
        };
        let xs = self.ir.var(items.name);
        let rest = self.ir.var(tail.name);
        let after = self.ir.if_(
            self.ir.builtin(F::NullList, &[rest]),
            self.ir.var(builder.name),
            self.ir.builtin(
                F::AppendByteString,
                &[
                    self.builtin_bytes(b", "),
                    self.ir.app(
                        self.ir.var(function.name),
                        &[rest, self.ir.var(builder.name)],
                    ),
                ],
            ),
        );
        let first = self.ir.builtin(F::HeadList, &[xs]);
        let element = if pairs {
            let value = self
                .ir
                .app(diagnostic, &[self.ir.builtin(F::SndPair, &[first]), after]);
            self.ir.app(
                diagnostic,
                &[
                    self.ir.builtin(F::FstPair, &[first]),
                    self.ir
                        .builtin(F::AppendByteString, &[self.builtin_bytes(b": "), value]),
                ],
            )
        } else {
            self.ir.app(diagnostic, &[first, after])
        };
        let body = self.ir.if_(
            self.ir.builtin(F::NullList, &[xs]),
            self.ir.var(builder.name),
            self.ir
                .let_(tail, self.ir.builtin(F::TailList, &[xs]), element),
        );
        self.ir.let_rec(
            &[RecBinder {
                binder: function,
                params: self.ir.arena.alloc_slice_copy(&[items, builder]),
                static_params: &[1],
                body,
            }],
            self.ir.app(self.ir.var(function.name), &[list, suffix]),
        )
    }

    pub(crate) fn diagnostic_builtin(&self) -> &'a Core<'a> {
        let bytes = Ty::Const(&ConstTy::Bytes);
        let function = self.builtin_function("diagnostic", &[DATA, bytes], bytes);
        let input = Binder {
            name: self.ir.fresh("data"),
            ty: DATA,
        };
        let builder = Binder {
            name: self.ir.fresh("builder"),
            ty: bytes,
        };
        let value = self.ir.var(input.name);
        let suffix = self.ir.var(builder.name);
        let append = |text, rest| {
            self.ir
                .builtin(F::AppendByteString, &[self.builtin_bytes(text), rest])
        };
        let render_list = |list, pairs, end| {
            let empty = if pairs {
                b"{}".as_slice()
            } else {
                b"[]".as_slice()
            };
            let opening = if pairs {
                b"{_ ".as_slice()
            } else {
                b"[_ ".as_slice()
            };
            self.ir.if_(
                self.ir.builtin(F::NullList, &[list]),
                append(empty, end),
                append(
                    opening,
                    self.diagnostic_sequence(
                        self.ir.var(function.name),
                        list,
                        append(if pairs { b" }" } else { b"]" }, end),
                        pairs,
                    ),
                ),
            )
        };
        let raw = Binder {
            name: self.ir.fresh("constr"),
            ty: Ty::Const(self.ir.arena.alloc(ConstTy::Pair(
                Ty::Const(&ConstTy::Int),
                Ty::Const(self.ir.arena.alloc(ConstTy::List(DATA))),
            ))),
        };
        let tag = self.ir.builtin(F::FstPair, &[self.ir.var(raw.name)]);
        let failure = if self.trace.user == TraceLevel::Silent {
            self.ir.error()
        } else {
            self.ir.trace(
                self.ir.lit(Constant::string(
                    self.ir.arena,
                    "What are you doing? No I mean, seriously.",
                )),
                self.ir.error(),
            )
        };
        let cbor_tag = self.ir.if_(
            self.ir.builtin(F::LessThanInteger, &[tag, self.ir.int(7)]),
            self.ir.builtin(F::AddInteger, &[self.ir.int(121), tag]),
            self.ir.if_(
                self.ir
                    .builtin(F::LessThanInteger, &[tag, self.ir.int(128)]),
                self.ir.builtin(F::AddInteger, &[self.ir.int(1273), tag]),
                failure,
            ),
        );
        let fields = self.ir.builtin(F::SndPair, &[self.ir.var(raw.name)]);
        let constr = self.ir.let_(
            raw,
            self.ir.builtin(F::UnConstrData, &[value]),
            self.ir.app(
                self.integer_decimal_builtin(false),
                &[
                    cbor_tag,
                    append(b"(", render_list(fields, false, append(b")", suffix))),
                ],
            ),
        );
        let map = render_list(self.ir.builtin(F::UnMapData, &[value]), true, suffix);
        let list = render_list(self.ir.builtin(F::UnListData, &[value]), false, suffix);
        let integer = self.ir.app(
            self.integer_decimal_builtin(false),
            &[self.ir.builtin(F::UnIData, &[value]), suffix],
        );
        let raw_bytes = Binder {
            name: self.ir.fresh("bytes"),
            ty: bytes,
        };
        let byte_value = self.ir.var(raw_bytes.name);
        let byte_string = self.ir.let_(
            raw_bytes,
            self.ir.builtin(F::UnBData, &[value]),
            append(
                b"h'",
                self.ir.app(
                    self.encode_base16_builtin(),
                    &[
                        byte_value,
                        self.ir.builtin(
                            F::SubtractInteger,
                            &[
                                self.ir.builtin(F::LengthOfByteString, &[byte_value]),
                                self.ir.int(1),
                            ],
                        ),
                        append(b"'", suffix),
                    ],
                ),
            ),
        );
        let body = self.ir.force(self.ir.builtin(
            F::ChooseData,
            &[
                value,
                self.ir.delay(constr),
                self.ir.delay(map),
                self.ir.delay(list),
                self.ir.delay(integer),
                self.ir.delay(byte_string),
            ],
        ));
        self.ir.let_rec(
            &[RecBinder {
                binder: function,
                params: self.ir.arena.alloc_slice_copy(&[input, builder]),
                static_params: &[],
                body,
            }],
            self.ir.var(function.name),
        )
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
        let trait_ = self.build.inputs[ctx.input]
            .tables
            .core_trait(QualifiedName {
                home: primitives::literal_home(),
                name: trait_name,
            });
        let annotation = self.method_annotation(trait_, method)?;
        let typ = self.substitute(self.can_type(node, ctx)?, &ctx.subst)?;
        let info = self
            .build
            .tables
            .traits
            .get(&trait_)
            .ok_or_else(|| Error::MissingMethod {
                trait_: self.ir.arena.alloc(trait_),
                method,
            })?;
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
            .ok_or_else(|| Error::MissingMethod {
                trait_: self.ir.arena.alloc(trait_),
                method,
            })
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
            Evidence::ReflexiveLift { .. } if trait_.is_core_trait(primitives::lift_trait()) => {
                let value = Binder {
                    name: self.ir.fresh("identity"),
                    ty: Ty::Erased,
                };
                return Ok(self.ir.lam(&[value], self.ir.var(value.name)));
            }
            Evidence::StructuralEq { .. } if trait_.is_core_trait(primitives::eq_trait()) => {
                if method == "eq" {
                    return Ok(self.ir.builtin(F::EqualsData, &[]));
                }
                // A default such as neq must still execute its source body.
                *self
                    .defaults
                    .get(&(trait_, method))
                    .ok_or_else(|| Error::MissingMethod {
                        trait_: self.ir.arena.alloc(trait_),
                        method,
                    })?
            }
            Evidence::Impl { impl_, .. } => *self
                .methods
                .get(&(*impl_, method))
                .or_else(|| self.defaults.get(&(trait_, method)))
                .ok_or_else(|| Error::MissingMethod {
                    trait_: self.ir.arena.alloc(trait_),
                    method,
                })?,
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
        if let Type::DeclaredHole(hole) = &actual.value
            && let Some(solution) = self
                .build
                .tables
                .kinds
                .declared
                .resolve(self.ir.arena.as_bump(), hole)
        {
            return self.match_type(pattern, solution, variables, subst);
        }
        if let Type::Var(name) = pattern.value
            && variables.contains(&name)
        {
            if let Some(previous) = subst.get(name) {
                if (Evidence::StructuralEq {
                    trait_: primitives::eq_trait(),
                    typ: previous,
                }) != (Evidence::StructuralEq {
                    trait_: primitives::eq_trait(),
                    typ: actual,
                }) {
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
            (Type::DeclaredHole(a), Type::DeclaredHole(b)) if a.id == b.id && a.kind == b.kind => {
                Ok(())
            }
            (Type::Lambda { from: a, to: b }, Type::Lambda { from: c, to: d }) => {
                self.match_type(a, c, variables, subst)?;
                self.match_type(b, d, variables, subst)
            }
            (
                Type::Function {
                    arguments: a,
                    result: b,
                },
                Type::Function {
                    arguments: c,
                    result: d,
                },
            ) => {
                self.match_types(a, c, variables, subst)?;
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
        if let Type::DeclaredHole(hole) = &typ.value
            && let Some(solution) = self
                .build
                .tables
                .kinds
                .declared
                .resolve(self.ir.arena.as_bump(), hole)
        {
            return self.open_alias(solution);
        }
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
