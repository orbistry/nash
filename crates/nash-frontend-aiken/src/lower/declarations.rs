use super::{Expr, Lower, Pattern, Result, n, profile};
use aiken_lang::{
    ast::{Annotation, ArgBy, ArgName, Definition, Span, UntypedArg, UntypedModule, UntypedTest},
    expr::UntypedExpr,
};
use nash_frontend::SourceInput;

type Function<'a> = (&'a [Pattern<'a>], Expr<'a>, &'a n::Annotation<'a>);

impl<'a> Lower<'a, '_> {
    pub(crate) fn module(
        &mut self,
        input: SourceInput<'_, '_>,
        ast: &UntypedModule,
    ) -> Result<&'a n::Module<'a>> {
        let span = Span {
            start: 0,
            end: input.source.len(),
        };
        let mut imports = vec![self.alloc(n::Import {
            import: self.at(Span { start: 0, end: 0 }, "Builtin"),
            alias: None,
            exposing: self.alloc(n::Exposing::Explicit(&[])),
        })];
        let mut values = Vec::new();
        let mut unions = Vec::new();
        let mut aliases = Vec::new();
        let mut exports = Vec::new();
        let kind = n::ModuleKind::Normal;
        for def in &ast.definitions {
            match def {
                Definition::Use(import) => {
                    let alias = import
                        .as_name
                        .as_deref()
                        .or_else(|| import.module.last().map(String::as_str));
                    imports.push(self.alloc(n::Import {
                        import: self.at(
                            import.location,
                            self.text(&profile::module_name(&import.module)),
                        ),
                        alias: alias.map(|alias| self.text(alias)),
                        exposing: self.alloc(n::Exposing::Explicit(&[])),
                    }));
                }
                Definition::Validator(validator) => {
                    let value = self.validator(validator)?;
                    exports.push(self.alloc(n::Exposed::Lower(value.value.name)));
                    values.push(value);
                }
                Definition::Fn(fun) => {
                    let span = Span {
                        start: fun.location.start,
                        end: fun.end_position.saturating_add(1),
                    };
                    let (arguments, body, annotation) = self.function(
                        span,
                        &fun.arguments,
                        fun.return_annotation.as_ref(),
                        &fun.body,
                    )?;
                    let body = self.callable(span, &fun.arguments, body);
                    let name = self.at(fun.location, self.text(&fun.name));
                    values.push(self.at(
                        span,
                        n::Value {
                            name,
                            arguments,
                            body,
                            annotation: Some(annotation),
                            attributes: &[],
                        },
                    ));
                    if fun.public {
                        exports.push(self.alloc(n::Exposed::Lower(name)));
                    }
                }
                Definition::ModuleConstant(constant) => {
                    let name = self.at(constant.location, self.text(&constant.name));
                    let body = self.expr(&constant.value)?;
                    let annotation = constant
                        .annotation
                        .as_ref()
                        .map(|typ| {
                            self.typ(typ).map(|typ| {
                                self.alloc(n::Annotation {
                                    constraints: &[],
                                    typ,
                                })
                            })
                        })
                        .transpose()?;
                    let body = match annotation {
                        Some(annotation) => self.conversion_ascribe(
                            constant.value.location(),
                            body,
                            annotation.typ,
                            n::ConversionSite::ModuleConstant,
                        ),
                        None => body,
                    };
                    let body = self.at(
                        constant.location,
                        n::Expr::ModuleConstantCheck { value: body },
                    );
                    values.push(self.at(
                        constant.location,
                        n::Value {
                            name,
                            arguments: &[],
                            body,
                            annotation,
                            attributes: &[],
                        },
                    ));
                    if constant.public {
                        exports.push(self.alloc(n::Exposed::Lower(name)));
                    }
                }
                Definition::TypeAlias(alias) => {
                    let name = self.at(alias.location, self.text(&alias.alias));
                    let arguments = self.type_parameters(alias.location, &alias.parameters);
                    aliases.push(self.at(
                        alias.location,
                        n::Alias {
                            name,
                            transparent: true,
                            arguments,
                            typ: self.typ(&alias.annotation)?,
                            attributes: &[],
                        },
                    ));
                    if alias.public {
                        exports.push(self.alloc(n::Exposed::Upper {
                            name,
                            privacy: n::Privacy::Private,
                        }));
                    }
                }
                Definition::DataType(data) => {
                    let name = self.at(data.location, self.text(&data.name));
                    let (encoding, tags) = crate::validate::data_layout(self.spans, data)?;
                    let mut ctors = Vec::new();
                    for ctor in &data.constructors {
                        let arguments = if ctor
                            .arguments
                            .first()
                            .is_some_and(|arg| arg.label.is_some())
                        {
                            let mut fields = Vec::new();
                            for arg in &ctor.arguments {
                                let Some(label) = arg.label.as_deref() else {
                                    return self.invalid_declaration(
                                        arg.location,
                                        "A constructor cannot mix labeled and positional fields.",
                                    );
                                };
                                fields.push((
                                    self.at(arg.location, self.text(label)),
                                    self.typ(&arg.annotation)?,
                                ));
                            }
                            n::CtorArgs::Labeled(self.slice(&fields))
                        } else {
                            let args = ctor
                                .arguments
                                .iter()
                                .map(|arg| self.typ(&arg.annotation))
                                .collect::<Result<Vec<_>>>()?;
                            n::CtorArgs::Positional(self.slice(&args))
                        };
                        ctors.push(self.alloc(n::Ctor {
                            name: self.at(ctor.location, self.text(&ctor.name)),
                            arguments,
                        }));
                    }
                    unions.push(self.at(
                        data.location,
                        n::Union {
                            name,
                            data_layout: Some(n::DataLayout {
                                encoding,
                                tags: self.slice(&tags),
                                opaque: data.opaque,
                            }),
                            arguments: self.type_parameters(data.location, &data.parameters),
                            ctors: self.slice(&ctors),
                            attributes: &[],
                        },
                    ));
                    if data.public {
                        exports.push(self.alloc(n::Exposed::Upper {
                            name,
                            privacy: if data.opaque {
                                n::Privacy::Private
                            } else {
                                n::Privacy::Public(self.spans.region(data.location))
                            },
                        }));
                    }
                }
                Definition::Test(test) => {
                    if input.origin != nash_frontend::SourceOrigin::Dependency {
                        values.push(self.runnable(test, false)?);
                    }
                }
                Definition::Benchmark(benchmark) => {
                    if input.origin != nash_frontend::SourceOrigin::Dependency {
                        values.push(self.runnable(benchmark, true)?);
                    }
                }
            }
        }
        for value in &self.extra_values {
            exports.push(self.alloc(n::Exposed::Lower(value.value.name)));
        }
        values.extend_from_slice(&self.extra_values);
        Ok(self.alloc(n::Module {
            kind,
            name: Some(self.at(span, self.text(input.expected_module.as_str()))),
            exports: self.at(span, n::Exposing::Explicit(self.slice(&exports))),
            docs: self.module_docs(ast, span),
            imports: self.slice(&imports),
            values: self.slice(&values),
            unions: self.slice(&unions),
            aliases: self.slice(&aliases),
            traits: &[],
            impls: &[],
            tests: None,
            binops: &[],
        }))
    }

    fn type_parameters(&self, span: Span, parameters: &[String]) -> &'a [&'a n::TypeParam<'a>] {
        let params = parameters
            .iter()
            .map(|name| {
                self.alloc(n::TypeParam {
                    name: self.at(span, self.text(name)),
                    repr: None,
                })
            })
            .collect::<Vec<_>>();
        self.slice(&params)
    }

    pub(super) fn callable(
        &self,
        span: Span,
        arguments: &[UntypedArg],
        value: Expr<'a>,
    ) -> Expr<'a> {
        let labels = arguments
            .iter()
            .enumerate()
            .map(|(index, arg)| self.text(&arg.arg_name(index).get_label()))
            .collect::<Vec<_>>();
        self.at(
            span,
            n::Expr::Callable {
                arity: arguments.len(),
                labels: self.slice(&labels),
                value,
            },
        )
    }

    fn runnable(
        &mut self,
        runnable: &UntypedTest,
        benchmark: bool,
    ) -> Result<&'a nash_region::Located<n::Value<'a>>> {
        let span = Span {
            start: runnable.location.start,
            end: runnable.end_position.saturating_add(1),
        };
        if (benchmark && runnable.arguments.len() != 1) || runnable.arguments.len() > 1 {
            return self.invalid_declaration(
                span,
                if benchmark {
                    "A benchmark requires exactly one parameter with a sampler."
                } else {
                    "A test accepts at most one parameter with a fuzzer."
                },
            );
        }
        // Generator expressions are outside the parameter's lexical scope.
        let generator = runnable
            .arguments
            .first()
            .map(|argument| self.expr(&argument.via))
            .transpose()?;
        let arguments = runnable
            .arguments
            .iter()
            .map(|argument| argument.arg.clone())
            .collect::<Vec<_>>();
        let (parameters, body, annotation) = self.function(
            span,
            &arguments,
            runnable.return_annotation.as_ref(),
            &runnable.body,
        )?;
        let parameters = if arguments.is_empty() {
            &[][..]
        } else {
            parameters
        };
        let function = self.at(span, n::Expr::Function { parameters, body });
        let function = self.callable(span, &arguments, function);
        let n::Type::Function {
            arguments: types,
            result,
        } = &annotation.typ.value
        else {
            unreachable!("function signature retains grouped arguments");
        };
        let argument_type = runnable
            .arguments
            .first()
            .and_then(|argument| argument.arg.annotation.as_ref())
            .map(|_| types[0]);
        let return_type = runnable.return_annotation.as_ref().map(|_| *result);
        let body = self.at(
            span,
            n::Expr::RunnableCheck {
                generator,
                argument_type,
                return_type,
                function,
                benchmark,
            },
        );
        Ok(self.at(
            span,
            n::Value {
                name: self.at(runnable.location, self.text(&runnable.name)),
                arguments: &[],
                body,
                annotation: None,
                attributes: &[],
            },
        ))
    }

    pub(super) fn function(
        &mut self,
        span: Span,
        arguments: &[UntypedArg],
        ret: Option<&Annotation>,
        body: &UntypedExpr,
    ) -> Result<Function<'a>> {
        let scope = self.locals.len();
        let mut parameters = Vec::with_capacity(arguments.len().max(1));
        let mut names = std::collections::HashSet::new();
        let mut destructures = Vec::new();
        for argument in arguments {
            let pattern = match &argument.by {
                ArgBy::ByName(ArgName::Named { name, location, .. }) => {
                    if !names.insert(name) {
                        return self
                            .invalid_declaration(*location, "Duplicate function parameter name.");
                    }
                    let name = self.bind(name);
                    self.at(*location, n::Pattern::Var(name))
                }
                ArgBy::ByName(ArgName::Discarded { location, .. }) => {
                    self.at(*location, n::Pattern::Anything)
                }
                ArgBy::ByPattern(pattern) => {
                    let name = self.fresh();
                    destructures.push((argument.location, pattern, name));
                    self.at(argument.location, n::Pattern::Var(name))
                }
            };
            parameters.push(pattern);
        }
        if parameters.is_empty() {
            parameters.push(self.at(span, n::Pattern::Unit));
        }
        // Pattern parameters are sequential let-bindings after all ordinary
        // parameters enter scope, matching ArgBy::into_extra_assignment.
        let mut checks = Vec::with_capacity(destructures.len());
        for (location, pattern, name) in destructures {
            checks.push((location, self.pattern(pattern)?, self.var(location, name)));
        }
        let mut body = self.expr(body)?;
        for (location, pattern, value) in checks.into_iter().rev() {
            body = self.apply_pattern(location, pattern, value, body);
        }
        let body = self.at(span, n::Expr::TypeScope { value: body });
        self.locals.truncate(scope);
        // One signature hydrates all written variables together. Omitted positions
        // and explicit holes are independently flexible, not rigid quantified names.
        let args = arguments
            .iter()
            .map(|arg| match &arg.annotation {
                Some(annotation) => self.typ(annotation),
                None => Ok(self.at(arg.location, n::Type::Hole)),
            })
            .collect::<Result<Vec<_>>>()?;
        let ret = match ret {
            Some(annotation) => self.typ(annotation)?,
            None => self.at(span, n::Type::Hole),
        };
        let annotation = self.alloc(n::Annotation {
            constraints: &[],
            typ: self.function_type(span, &args, ret),
        });
        Ok((self.slice(&parameters), body, annotation))
    }

    pub(super) fn invalid_declaration<T>(&self, span: Span, message: &str) -> Result<T> {
        Err(nash_frontend::FrontendDiagnostic::error(
            "NAF2202",
            "Invalid Aiken declaration",
            message,
            Some(self.spans.region(span)),
        )
        .into())
    }
}
