use super::{Expr, Lower, Result, Type, n};
use crate::validate::validators;
use aiken_lang::ast::{
    Annotation, ArgBy, ArgName, Span, UntypedArg, UntypedFunction, UntypedValidator,
};
use nash_region::Located;

impl<'a> Lower<'a, '_> {
    pub(super) fn validator(
        &mut self,
        validator: &UntypedValidator,
    ) -> Result<&'a Located<n::Value<'a>>> {
        let span = Span {
            start: validator.location.start,
            end: validator.end_position.saturating_add(1),
        };
        let handlers = self.add_validator_handlers(validator)?;
        let parameter_metadata = validator
            .params
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let mut argument = argument.clone();
                argument
                    .annotation
                    .get_or_insert_with(|| Annotation::data(argument.location));
                self.boundary_metadata(&argument, index)
            })
            .collect::<Result<Vec<_>>>()?;
        let entry_name = self.text(&format!("$validator.{}.entry", validator.name));
        self.entry_points.push(nash_frontend::SourceEntryPoint {
            name: self.text(&validator.name),
            function: entry_name,
            docs: validator.doc.as_deref().map(|doc| self.text(doc)),
            parameters: self.slice(&parameter_metadata),
            handlers: self.slice(&handlers),
            region: self.spans.region(span),
        });
        let scope = self.locals.len();
        let mut parameters = Vec::new();
        let mut parameter_bindings = Vec::new();
        for param in &validator.params {
            let raw = self.fresh();
            parameters.push(self.at(param.location, n::Pattern::Var(raw)));
            let pattern = self.validator_parameter(param)?;
            let typ = param
                .annotation
                .as_ref()
                .map(|annotation| self.typ(annotation))
                .transpose()?
                .unwrap_or_else(|| self.validator_data_type(param.location));
            parameter_bindings.push((param.location, pattern, self.var(param.location, raw), typ));
        }
        let context = self.fresh();
        parameters.push(self.at(span, n::Pattern::Var(context)));
        let context_value = self.var(span, context);
        let fallback = self.validator_body(&validator.fallback, &[context_value])?;
        let mut body = if validator.handlers.is_empty() {
            fallback
        } else {
            let fields = self.fresh();
            let fields_value = self.var(span, fields);
            let purpose = self.fresh();
            let purpose_value = self.var(span, purpose);
            let purpose_fields = self.call(span, self.builtin(span, "sndPair"), &[purpose_value]);
            let tag = self.call(span, self.builtin(span, "fstPair"), &[purpose_value]);
            let transaction = self.list_item(span, fields_value, 0);
            let redeemer = self.list_item(span, fields_value, 1);
            let mut dispatch = fallback;
            for handler in validator.handlers.iter().rev() {
                let Some((purpose_tag, _, argument_index)) = validators::purpose(&handler.name)
                else {
                    return Err(validators::invalid(
                        self.spans,
                        handler.location,
                        "Unknown validator purpose.",
                    ));
                };
                let argument = self.list_item(handler.location, purpose_fields, argument_index);
                let inputs = if handler.name == "spend" {
                    vec![
                        self.list_item(handler.location, purpose_fields, 1),
                        redeemer,
                        argument,
                        transaction,
                    ]
                } else {
                    vec![redeemer, argument, transaction]
                };
                let mut branch = self.validator_body(handler, &inputs)?;
                if matches!(handler.name.as_str(), "publish" | "propose") {
                    // The purpose pattern binds the integer index before redeemer decoding.
                    let index = self.list_item(handler.location, purpose_fields, 0);
                    let index = self.call(
                        handler.location,
                        self.builtin(handler.location, "unIData"),
                        &[index],
                    );
                    branch = self.apply_pattern(
                        handler.location,
                        self.at(handler.location, n::Pattern::Anything),
                        index,
                        branch,
                    );
                }
                let condition = self.call(
                    span,
                    self.builtin(span, "equalsInteger"),
                    &[
                        tag,
                        self.at(
                            span,
                            n::Expr::Constant(n::Constant::Int(purpose_tag as i128)),
                        ),
                    ],
                );
                dispatch = self.if_(span, condition, branch, dispatch);
            }
            let purpose_data = self.list_item(span, fields_value, 2);
            let purpose_pair = self.call(span, self.builtin(span, "unConstrData"), &[purpose_data]);
            dispatch = self.apply_pattern(
                span,
                self.at(span, n::Pattern::Var(purpose)),
                purpose_pair,
                dispatch,
            );
            let context_pair =
                self.call(span, self.builtin(span, "unConstrData"), &[context_value]);
            let context_fields = self.call(span, self.builtin(span, "sndPair"), &[context_pair]);
            self.apply_pattern(
                span,
                self.at(span, n::Pattern::Var(fields)),
                context_fields,
                dispatch,
            )
        };
        let unit = self.at(span, n::Expr::Unit);
        let failure = self.call(span, self.builtin(span, "error"), &[unit]);
        body = self.if_(span, body, unit, failure);
        for (location, pattern, value, typ) in parameter_bindings.into_iter().rev() {
            body = self.apply_boundary(
                location,
                pattern,
                value,
                typ,
                n::ConversionSite::ValidatorParameter,
                body,
            );
        }
        self.locals.truncate(scope);
        let args = vec![self.validator_data_type(span); parameters.len()];
        Ok(self.at(
            span,
            n::Value {
                name: self.at(validator.location, entry_name),
                arguments: self.slice(&parameters),
                body,
                annotation: Some(self.alloc(n::Annotation {
                    constraints: &[],
                    typ: self.function_type(span, &args, self.at(span, n::Type::Unit)),
                })),
                attributes: &[],
            },
        ))
    }

    fn validator_data_type(&self, span: Span) -> Type<'a> {
        self.at(
            span,
            n::Type::TypeQual {
                region: self.spans.region(span),
                module: "Builtin",
                name: "Data",
                args: &[],
            },
        )
    }

    fn validator_parameter(&mut self, arg: &UntypedArg) -> Result<super::Pattern<'a>> {
        match &arg.by {
            ArgBy::ByName(ArgName::Named { name, .. }) => {
                let name = self.bind(name);
                Ok(self.at(arg.location, n::Pattern::Var(name)))
            }
            ArgBy::ByName(ArgName::Discarded { .. }) => {
                Ok(self.at(arg.location, n::Pattern::Anything))
            }
            ArgBy::ByPattern(pattern) => self.pattern(pattern),
        }
    }

    fn apply_boundary(
        &self,
        span: Span,
        pattern: super::Pattern<'a>,
        value: Expr<'a>,
        typ: Type<'a>,
        site: n::ConversionSite,
        body: Expr<'a>,
    ) -> Expr<'a> {
        let fallback = self.call(
            span,
            self.builtin(span, "error"),
            &[self.at(span, n::Expr::Unit)],
        );
        self.at(
            span,
            n::Expr::Match {
                value,
                pattern,
                body,
                fallback,
                annotation: Some(typ),
                conversion: Some(site),
            },
        )
    }

    fn validator_body(
        &mut self,
        function: &UntypedFunction,
        inputs: &[Expr<'a>],
    ) -> Result<Expr<'a>> {
        let scope = self.locals.len();
        let mut bindings = Vec::new();
        let redeemer_index = usize::from(function.name == "spend");
        let arguments = Self::handler_arguments(function);
        for (index, (arg, input)) in arguments.iter().zip(inputs).enumerate() {
            let pattern = self.validator_parameter(arg)?;
            let typ = arg
                .annotation
                .as_ref()
                .map(|annotation| self.typ(annotation))
                .transpose()?
                .unwrap_or_else(|| self.validator_data_type(arg.location));
            let site = if function.name == "else" || index + 1 == arguments.len() {
                n::ConversionSite::ValidatorContext
            } else if index == redeemer_index {
                n::ConversionSite::ValidatorRedeemer
            } else if function.name == "mint" && index == 1 {
                n::ConversionSite::ValidatorMintPolicy
            } else if function.name == "spend" && index == 0 {
                n::ConversionSite::ValidatorDatum
            } else {
                n::ConversionSite::ValidatorPurpose
            };
            let binding = (arg.location, pattern, *input, typ, site);
            if function.name == "mint" && index == 1 {
                bindings.insert(0, binding);
            } else {
                bindings.push(binding);
            }
        }
        let mut body = self.expr(&function.body)?;
        self.locals.truncate(scope);
        for (span, pattern, input, typ, site) in bindings.into_iter().rev() {
            body = self.apply_boundary(span, pattern, input, typ, site, body);
        }
        Ok(body)
    }

    fn boundary_metadata(
        &self,
        argument: &UntypedArg,
        position: usize,
    ) -> Result<nash_frontend::SourceBoundaryBinding<'a>> {
        let name = argument.arg_name(position);
        Ok(nash_frontend::SourceBoundaryBinding {
            name: self.text(&name.get_name()),
            label: self.text(&name.get_label()),
            position,
            annotation: argument
                .annotation
                .as_ref()
                .map(|annotation| self.typ(annotation))
                .transpose()?,
            region: self.spans.region(argument.location),
        })
    }

    fn handler_arguments(function: &UntypedFunction) -> Vec<UntypedArg> {
        function
            .arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let mut argument = argument.clone();
                argument.annotation.get_or_insert_with(|| {
                    if function.name == "spend" && index == 0 {
                        Annotation::option(Annotation::data(argument.location))
                    } else if function.name == "mint" && index == 1 {
                        Annotation::bytearray(argument.location)
                    } else {
                        Annotation::data(argument.location)
                    }
                });
                argument
            })
            .collect()
    }

    fn add_validator_handlers(
        &mut self,
        validator: &UntypedValidator,
    ) -> Result<Vec<nash_frontend::SourceHandler<'a>>> {
        let mut handlers = Vec::new();
        for handler in validator
            .handlers
            .iter()
            .chain(std::iter::once(&validator.fallback))
        {
            let handler_arguments = Self::handler_arguments(handler);
            let arguments = validator
                .params
                .iter()
                .cloned()
                .map(|mut argument| {
                    argument
                        .annotation
                        .get_or_insert_with(|| Annotation::data(argument.location));
                    argument
                })
                .chain(handler_arguments.iter().cloned())
                .collect::<Vec<_>>();
            let span = Span {
                start: handler.location.start,
                end: handler.end_position.saturating_add(1),
            };
            let name = self.validator_handlers[validator.name.as_str()][handler.name.as_str()];
            let (patterns, body, annotation) = self.function(
                span,
                &arguments,
                handler.return_annotation.as_ref(),
                &handler.body,
            )?;
            let body = self.callable(span, &arguments, body);
            self.extra_values.push(self.at(
                span,
                n::Value {
                    name: self.at(handler.location, name),
                    arguments: patterns,
                    body,
                    annotation: Some(annotation),
                    attributes: &[],
                },
            ));
            let metadata = handler_arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| self.boundary_metadata(argument, index))
                .collect::<Result<Vec<_>>>()?;
            handlers.push(nash_frontend::SourceHandler {
                purpose: self.text(&handler.name),
                function: name,
                docs: handler.doc.as_deref().map(|doc| self.text(doc)),
                arguments: self.slice(&metadata),
                region: self.spans.region(handler.location),
            });
        }
        Ok(handlers)
    }

    fn list_item(&self, span: Span, mut list: Expr<'a>, index: usize) -> Expr<'a> {
        for _ in 0..index {
            list = self.call(span, self.builtin(span, "tailList"), &[list]);
        }
        self.call(span, self.builtin(span, "headList"), &[list])
    }
}
