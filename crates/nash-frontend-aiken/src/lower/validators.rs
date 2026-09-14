use super::{Expr, Lower, Result, Type, n};
use crate::validate::validators;
use aiken_lang::ast::{ArgBy, ArgName, Span, UntypedArg, UntypedFunction, UntypedValidator};
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
        let scope = self.locals.len();
        let mut parameters = Vec::new();
        for param in &validator.params {
            parameters.push(self.validator_parameter(param)?);
        }
        let context = self.fresh();
        parameters.push(self.at(span, n::Pattern::Var(context)));
        let context_value = self.var(span, context);
        let fallback = self.validator_body(&validator.fallback, &[context_value])?;
        let body = if let Some(handler) = validator.handlers.first() {
            let fields = self.fresh();
            let fields_value = self.var(span, fields);
            let purpose = self.fresh();
            let purpose_value = self.var(span, purpose);
            let transaction = self.list_item(span, fields_value, 0);
            let redeemer = self.list_item(span, fields_value, 1);
            let policy = self.list_item(
                span,
                self.call(span, self.builtin(span, "sndPair"), &[purpose_value]),
                0,
            );
            let handler_body = self.validator_body(handler, &[redeemer, policy, transaction])?;
            let tag = self.call(span, self.builtin(span, "fstPair"), &[purpose_value]);
            let is_mint = self.call(
                span,
                self.builtin(span, "equalsInteger"),
                &[tag, self.at(span, n::Expr::Constant(n::Constant::Int(0)))],
            );
            let dispatch = self.if_(span, is_mint, handler_body, fallback);
            let purpose_data = self.list_item(span, fields_value, 2);
            let purpose_pair = self.call(span, self.builtin(span, "unConstrData"), &[purpose_data]);
            let dispatch = self.apply_pattern(
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
        } else {
            // Official else-only validators receive the raw context without purpose dispatch.
            fallback
        };
        let unit = self.at(span, n::Expr::Unit);
        let failure = self.call(span, self.builtin(span, "error"), &[unit]);
        let body = self.if_(span, body, unit, failure);
        self.locals.truncate(scope);
        let data = self.validator_data_type(span);
        let args = vec![data; parameters.len()];
        Ok(self.at(
            span,
            n::Value {
                name: self.at(validator.location, "main"),
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
            _ => self.unsupported(arg.location, "validator argument patterns"),
        }
    }

    fn validator_body(
        &mut self,
        function: &UntypedFunction,
        inputs: &[Expr<'a>],
    ) -> Result<Expr<'a>> {
        let scope = self.locals.len();
        let mut bindings = Vec::new();
        for (index, (arg, input)) in function.arguments.iter().zip(inputs).enumerate() {
            let pattern = self.validator_parameter(arg)?;
            let decoder = if validators::primitive(arg.annotation.as_ref(), "Int") {
                Some("unIData")
            } else if validators::primitive(arg.annotation.as_ref(), "ByteArray") {
                Some("unBData")
            } else {
                None
            };
            // The official mint-purpose pattern decodes policy bytes even when
            // the user discards the policy. Only raw Data discards are unforced.
            if decoder.is_none() && matches!(pattern.value, n::Pattern::Anything) {
                continue;
            }
            let input = decoder.map_or(*input, |decoder| {
                self.call(arg.location, self.builtin(arg.location, decoder), &[*input])
            });
            let binding = (arg.location, pattern, input);
            if function.name == "mint" && index == 1 {
                // Purpose-pattern decoding precedes the redeemer's expect.
                bindings.insert(0, binding);
            } else {
                bindings.push(binding);
            }
        }
        let mut body = self.expr(&function.body)?;
        self.locals.truncate(scope);
        for (span, pattern, input) in bindings.into_iter().rev() {
            body = self.apply_pattern(span, pattern, input, body);
        }
        Ok(body)
    }

    fn list_item(&self, span: Span, mut list: Expr<'a>, index: usize) -> Expr<'a> {
        for _ in 0..index {
            list = self.call(span, self.builtin(span, "tailList"), &[list]);
        }
        self.call(span, self.builtin(span, "headList"), &[list])
    }
}
