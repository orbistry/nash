use super::{Lower, Pattern, Result, n, profile};
use aiken_lang::ast::{Namespace, Pattern as A, UntypedPattern};

impl<'a> Lower<'a, '_> {
    pub(super) fn pattern(&mut self, pattern: &UntypedPattern) -> Result<Pattern<'a>> {
        let start = self.locals.len();
        let lowered = self.pattern_inner(pattern)?;
        let mut names = std::collections::HashSet::new();
        for (name, _) in &self.locals[start..] {
            if !names.insert(name) {
                return Err(nash_frontend::FrontendDiagnostic::error(
                    "NAF2102",
                    "Duplicate pattern binding",
                    format!("The name `{name}` is bound more than once in this pattern."),
                    Some(self.spans.region(pattern.location())),
                )
                .into());
            }
        }
        Ok(lowered)
    }

    fn pattern_inner(&mut self, pattern: &UntypedPattern) -> Result<Pattern<'a>> {
        let span = pattern.location();
        let value = match pattern {
            A::Var { name, .. } => n::Pattern::Var(self.bind(name)),
            A::Discard { .. } => n::Pattern::Anything,
            A::Assign { name, pattern, .. } => {
                let pattern = self.pattern_inner(pattern)?;
                let name = self.bind(name);
                n::Pattern::Alias {
                    pattern,
                    name: self.at(span, name),
                }
            }
            A::Int { value, .. } => n::Pattern::Constant(self.integer(span, value)?),
            A::ByteArray { value, .. } => {
                let bytes = value.iter().map(|(byte, _)| *byte).collect::<Vec<_>>();
                n::Pattern::Constant(n::Constant::Bytes(self.slice(&bytes)))
            }
            A::List { elements, tail, .. } => {
                let elements = elements
                    .iter()
                    .map(|item| self.pattern_inner(item))
                    .collect::<Result<Vec<_>>>()?;
                let tail = tail
                    .as_deref()
                    .map(|tail| self.pattern_inner(tail))
                    .transpose()?;
                n::Pattern::DataList {
                    elements: self.slice(&elements),
                    tail,
                }
            }
            A::Tuple { elems, .. } => {
                let elems = elems
                    .iter()
                    .map(|item| self.pattern_inner(item))
                    .collect::<Result<Vec<_>>>()?;
                match elems.as_slice() {
                    [first, second, rest @ ..] => n::Pattern::DataTuple {
                        first,
                        second,
                        rest: self.slice(rest),
                    },
                    _ => return self.unsupported(span, "tuples with fewer than two elements"),
                }
            }
            A::Pair { fst, snd, .. } => n::Pattern::Pair {
                first: self.pattern_inner(fst)?,
                second: self.pattern_inner(snd)?,
            },
            A::Constructor {
                name,
                arguments,
                module,
                spread_location,
                ..
            } => {
                let args = arguments
                    .iter()
                    .map(|arg| {
                        Ok(n::PatternArgument {
                            label: arg
                                .label
                                .as_ref()
                                .map(|label| self.at(arg.location, self.text(label))),
                            pattern: self.pattern_inner(&arg.value)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let (module, type_name, name) = match module {
                    Some(Namespace::Module(module))
                        if self.prelude_qualifiers.contains(module.as_str()) =>
                    {
                        match profile::prelude_constructor(name) {
                            Some((type_name, name)) => (Some("Builtin"), Some(type_name), name),
                            None => (Some("Builtin"), None, self.text(name)),
                        }
                    }
                    Some(Namespace::Module(module)) => {
                        (Some(self.text(module)), None, self.text(name))
                    }
                    Some(Namespace::Type(module, type_name)) => {
                        let unavailable_owner = match module {
                            Some(module) => self.prelude_qualifiers.contains(module.as_str()),
                            None if self.local_types.contains(type_name.as_str()) => true,
                            None => match self.imported.get(type_name.as_str()) {
                                Some((module, _)) => self.prelude_qualifiers.contains(*module),
                                None => profile::primitive(type_name).is_some(),
                            },
                        };
                        if unavailable_owner {
                            return Err(nash_frontend::FrontendDiagnostic::error(
                                "NAF2102",
                                "Invalid constructor namespace",
                                "Type-qualified constructors require an imported user-defined type; use the constructor directly for local and prelude types.",
                                Some(self.spans.region(span)),
                            ).into());
                        }
                        let (module, type_name) = match module {
                            Some(module) => (Some(self.text(module)), self.text(type_name)),
                            None => match self.imported.get(type_name.as_str()) {
                                Some((module, original)) => (Some(*module), *original),
                                None => (None, self.text(type_name)),
                            },
                        };
                        (module, Some(type_name), self.text(name))
                    }
                    None if !self.globals.contains(name.as_str()) => {
                        if let Some((module, original)) = self.imported.get(name.as_str()) {
                            if self.prelude_qualifiers.contains(*module) {
                                match profile::prelude_constructor(original) {
                                    Some((type_name, name)) => {
                                        (Some("Builtin"), Some(type_name), name)
                                    }
                                    None => (Some("Builtin"), None, *original),
                                }
                            } else {
                                (Some(*module), None, *original)
                            }
                        } else if let Some((type_name, name)) = profile::prelude_constructor(name) {
                            (Some("Builtin"), Some(type_name), name)
                        } else {
                            (None, None, self.text(name))
                        }
                    }
                    None => (None, None, self.text(name)),
                };
                n::Pattern::Constructor {
                    region: self.spans.region(span),
                    module,
                    type_name,
                    name,
                    args: self.slice(&args),
                    spread: spread_location.map(|location| self.spans.region(location)),
                }
            }
        };
        Ok(self.at(span, value))
    }
}
