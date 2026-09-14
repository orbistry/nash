use super::{Expr, Lower, Pattern, Result, n, profile};
use aiken_lang::{
    ast::{Annotation, ArgBy, ArgName, Definition, Span, UntypedArg, UntypedModule},
    expr::UntypedExpr,
};
use nash_frontend::SourceInput;

type Function<'a> = (&'a [Pattern<'a>], Expr<'a>, Option<&'a n::Annotation<'a>>);

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
        let mut kind = n::ModuleKind::Normal;
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
                    kind = n::ModuleKind::Validator(self.spans.region(validator.location));
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
                    let name = self.at(fun.location, self.text(&fun.name));
                    values.push(self.at(
                        span,
                        n::Value {
                            name,
                            arguments,
                            body,
                            annotation,
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
                    let name =
                        self.at(alias.location, self.text(&profile::user_type(&alias.alias)));
                    let arguments = self.type_parameters(alias.location, &alias.parameters);
                    aliases.push(self.at(
                        alias.location,
                        n::Alias {
                            name,
                            arguments,
                            typ: self.typ(&alias.annotation)?,
                            attributes: &[],
                        },
                    ));
                    if alias.public {
                        exports.push(self.alloc(n::Exposed::LowerType {
                            name,
                            privacy: n::Privacy::Private,
                        }));
                    }
                }
                Definition::DataType(data) => {
                    let name = self.at(data.location, self.text(&profile::user_type(&data.name)));
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
                                    return self
                                        .unsupported(arg.location, "mixed constructor labels");
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
                            arguments: self.type_parameters(data.location, &data.parameters),
                            ctors: self.slice(&ctors),
                            attributes: &[],
                        },
                    ));
                    if data.public {
                        exports.push(self.alloc(n::Exposed::LowerType {
                            name,
                            privacy: if data.opaque {
                                n::Privacy::Private
                            } else {
                                n::Privacy::Public(self.spans.region(data.location))
                            },
                        }));
                    }
                }
                _ => return self.unsupported(def.location(), "this declaration"),
            }
        }
        Ok(self.alloc(n::Module {
            kind,
            name: Some(self.at(span, self.text(input.expected_module.as_str()))),
            exports: self.at(span, n::Exposing::Explicit(self.slice(&exports))),
            docs: self.alloc(n::Docs::NoDocs(self.spans.region(span))),
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

    pub(super) fn function(
        &mut self,
        span: Span,
        arguments: &[UntypedArg],
        ret: Option<&Annotation>,
        body: &UntypedExpr,
    ) -> Result<Function<'a>> {
        let complete = ret.is_some() && arguments.iter().all(|arg| arg.annotation.is_some());
        if !complete
            && (ret.is_some_and(Self::has_type_variable)
                || arguments
                    .iter()
                    .filter_map(|arg| arg.annotation.as_ref())
                    .any(Self::has_type_variable))
        {
            return self.unsupported(span, "partial polymorphic function annotations (annotate every argument and the return type)");
        }
        let scope = self.locals.len();
        let mut parameters = Vec::new();
        let mut checks = Vec::new();
        for argument in arguments {
            let pattern = match &argument.by {
                ArgBy::ByName(ArgName::Named {
                    name,
                    label,
                    location,
                }) => {
                    if name != label {
                        return self.unsupported(*location, "renamed function parameter labels");
                    }
                    let name = self.bind(name);
                    self.at(*location, n::Pattern::Var(name))
                }
                ArgBy::ByName(ArgName::Discarded {
                    name,
                    label,
                    location,
                }) => {
                    if name != label {
                        return self.unsupported(*location, "renamed function parameter labels");
                    }
                    self.at(*location, n::Pattern::Anything)
                }
                ArgBy::ByPattern(pattern) => self.pattern(pattern)?,
            };
            if !complete && argument.annotation.is_some() {
                let fresh = self.fresh();
                parameters.push(self.at(argument.location, n::Pattern::Var(fresh)));
                let mut value = self.var(argument.location, fresh);
                if let Some(annotation) = &argument.annotation {
                    let typ = self.typ(annotation)?;
                    value = self.ascribe(argument.location, value, typ);
                }
                checks.push((argument.location, pattern, value));
            } else {
                parameters.push(pattern);
            }
        }
        let mut names = std::collections::HashSet::new();
        if self.locals[scope..]
            .iter()
            .any(|(name, _)| !names.insert(name))
        {
            return self.unsupported(span, "duplicate function parameter names");
        }
        if parameters.is_empty() {
            parameters.push(self.at(span, n::Pattern::Unit));
        }
        let mut body = self.expr(body)?;
        if !complete {
            if let Some(ret) = ret {
                let typ = self.typ(ret)?;
                body = self.ascribe(span, body, typ);
            }
            for (location, pattern, value) in checks.into_iter().rev() {
                body = self.apply_pattern(location, pattern, value, body);
            }
        }
        self.locals.truncate(scope);
        let annotation = if complete {
            let args = arguments
                .iter()
                .filter_map(|arg| arg.annotation.as_ref())
                .map(|arg| self.typ(arg))
                .collect::<Result<Vec<_>>>()?;
            match ret {
                Some(ret) => Some(self.alloc(n::Annotation {
                    constraints: &[],
                    typ: self.function_type(span, &args, self.typ(ret)?),
                })),
                None => None,
            }
        } else {
            None
        };
        Ok((self.slice(&parameters), body, annotation))
    }
}
