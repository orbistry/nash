use super::{Lower, Result, Type, n, profile};
use aiken_lang::ast::{Annotation, Span};

impl<'a> Lower<'a, '_> {
    pub(super) fn typ(&self, annotation: &Annotation) -> Result<Type<'a>> {
        Ok(match annotation {
            Annotation::Constructor {
                location,
                module,
                name,
                arguments,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|arg| self.typ(arg))
                    .collect::<Result<Vec<_>>>()?;
                let prelude_name = match module {
                    Some(module) if self.prelude_qualifiers.contains(module.as_str()) => {
                        Some(name.as_str())
                    }
                    Some(_) => None,
                    None if self.local_types.contains(name.as_str()) => None,
                    None => match self.imported.get(name.as_str()) {
                        Some((module, original)) if self.prelude_qualifiers.contains(*module) => {
                            Some(*original)
                        }
                        Some(_) => None,
                        None => Some(name.as_str()),
                    },
                };
                if let Some(prelude_name) = prelude_name {
                    match (prelude_name, arguments.as_slice()) {
                        ("Pairs", [first, second]) => {
                            return Ok(self.builtin_type(
                                *location,
                                "data_list",
                                &[self.builtin_type(*location, "data_pair", &[*first, *second])],
                            ));
                        }
                        ("Fuzzer", [item]) => return Ok(self.fuzzer_type(*location, item)),
                        ("Sampler", [item]) => {
                            let int = self.builtin_type(*location, "int", &[]);
                            return Ok(self.function_type(
                                *location,
                                &[int],
                                self.fuzzer_type(*location, item),
                            ));
                        }
                        ("Pairs" | "Fuzzer" | "Sampler", _) => {
                            return self.invalid_declaration(
                                *location,
                                "Incorrect number of type arguments.",
                            );
                        }
                        _ => {}
                    }
                    if let Some(primitive) = profile::primitive(prelude_name) {
                        return Ok(self.builtin_type(*location, primitive, &arguments));
                    }
                }
                let region = self.spans.region(*location);
                let (module, name) = if let Some(module) = module {
                    (Some(self.text(module)), self.text(name))
                } else if self.local_types.contains(name.as_str()) {
                    (None, self.text(name))
                } else if let Some((module, original)) = self.imported.get(name.as_str()) {
                    (Some(*module), self.text(original))
                } else {
                    (None, self.text(name))
                };
                let args = self.slice(&arguments);
                self.at(
                    *location,
                    match module {
                        Some(module) => n::Type::TypeQual {
                            region,
                            module,
                            name,
                            args,
                        },
                        None => n::Type::Type { region, name, args },
                    },
                )
            }
            Annotation::Var { location, name } => self.at(*location, n::Type::Var(self.text(name))),
            Annotation::Hole { location, .. } => self.at(*location, n::Type::Hole),
            Annotation::Fn {
                location,
                arguments,
                ret,
            } => {
                let args = arguments
                    .iter()
                    .map(|arg| self.typ(arg))
                    .collect::<Result<Vec<_>>>()?;
                self.function_type(*location, &args, self.typ(ret)?)
            }
            Annotation::Tuple { location, elems } => {
                let elems = elems
                    .iter()
                    .map(|arg| self.typ(arg))
                    .collect::<Result<Vec<_>>>()?;
                match elems.as_slice() {
                    [first, second, rest @ ..] => self.at(
                        *location,
                        n::Type::TypeQual {
                            region: self.spans.region(*location),
                            module: "Builtin",
                            name: "data_tuple",
                            args: self.slice(&[self.at(
                                *location,
                                n::Type::Tuple {
                                    first,
                                    second,
                                    rest: self.slice(rest),
                                },
                            )]),
                        },
                    ),
                    _ => {
                        return self.invalid_declaration(
                            *location,
                            "Tuples require at least two elements.",
                        );
                    }
                }
            }
            Annotation::Pair { location, fst, snd } => self.at(
                *location,
                n::Type::TypeQual {
                    region: self.spans.region(*location),
                    module: "Builtin",
                    name: "data_pair",
                    args: self.slice(&[self.typ(fst)?, self.typ(snd)?]),
                },
            ),
        })
    }

    pub(super) fn function_type(&self, span: Span, args: &[Type<'a>], ret: Type<'a>) -> Type<'a> {
        self.at(
            span,
            n::Type::Function {
                arguments: self.slice(args),
                result: ret,
            },
        )
    }

    pub(super) fn builtin_type(&self, span: Span, name: &'a str, args: &[Type<'a>]) -> Type<'a> {
        self.at(
            span,
            n::Type::TypeQual {
                region: self.spans.region(span),
                module: "Builtin",
                name,
                args: self.slice(args),
            },
        )
    }

    fn fuzzer_type(&self, span: Span, item: Type<'a>) -> Type<'a> {
        let prng = self.builtin_type(span, "data_prng", &[]);
        let pair = self.at(
            span,
            n::Type::Tuple {
                first: prng,
                second: item,
                rest: &[],
            },
        );
        let tuple = self.builtin_type(span, "data_tuple", &[pair]);
        let option = self.builtin_type(span, "data_option", &[tuple]);
        self.function_type(span, &[prng], option)
    }
}
