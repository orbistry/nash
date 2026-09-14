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
                let region = self.spans.region(*location);
                let (module, name) = if let Some(module) = module {
                    (
                        Some(self.text(module)),
                        self.text(&profile::user_type(name)),
                    )
                } else if let Some((module, original)) = self.imported.get(name.as_str()) {
                    (Some(*module), self.text(&profile::user_type(original)))
                } else if let Some(name) = profile::primitive(name) {
                    (Some("Builtin"), name)
                } else {
                    (None, self.text(&profile::user_type(name)))
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
            Annotation::Hole { location, .. } => {
                return self.unsupported(*location, "type annotation holes");
            }
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
                        n::Type::Tuple {
                            first,
                            second,
                            rest: self.slice(rest),
                        },
                    ),
                    _ => return self.unsupported(*location, "tuples with fewer than two elements"),
                }
            }
            Annotation::Pair { location, fst, snd } => self.at(
                *location,
                n::Type::TypeQual {
                    region: self.spans.region(*location),
                    module: "Builtin",
                    name: "pair",
                    args: self.slice(&[self.typ(fst)?, self.typ(snd)?]),
                },
            ),
        })
    }

    pub(super) fn function_type(
        &self,
        span: Span,
        args: &[Type<'a>],
        mut ret: Type<'a>,
    ) -> Type<'a> {
        if args.is_empty() {
            return self.at(
                span,
                n::Type::Lambda {
                    from: self.at(span, n::Type::Unit),
                    to: ret,
                },
            );
        }
        for arg in args.iter().rev() {
            ret = self.at(span, n::Type::Lambda { from: arg, to: ret });
        }
        ret
    }

    pub(super) fn has_type_variable(annotation: &Annotation) -> bool {
        match annotation {
            Annotation::Var { .. } | Annotation::Hole { .. } => true,
            Annotation::Constructor { arguments, .. } => {
                arguments.iter().any(Self::has_type_variable)
            }
            Annotation::Fn { arguments, ret, .. } => {
                arguments.iter().any(Self::has_type_variable) || Self::has_type_variable(ret)
            }
            Annotation::Tuple { elems, .. } => elems.iter().any(Self::has_type_variable),
            Annotation::Pair { fst, snd, .. } => {
                Self::has_type_variable(fst) || Self::has_type_variable(snd)
            }
        }
    }
}
