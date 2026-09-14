use super::{Lower, Pattern, Result, n};
use aiken_lang::ast::{Namespace, Pattern as A, UntypedPattern};

impl<'a> Lower<'a, '_> {
    pub(super) fn pattern(&mut self, pattern: &UntypedPattern) -> Result<Pattern<'a>> {
        let start = self.locals.len();
        let lowered = self.pattern_inner(pattern)?;
        let mut names = std::collections::HashSet::new();
        for (name, _) in &self.locals[start..] {
            if !names.insert(name) {
                return self.unsupported(pattern.location(), "duplicate names in a pattern");
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
            A::Int { value, .. } => {
                n::Pattern::Constant(n::Constant::Int(self.integer(span, value)?))
            }
            A::ByteArray { value, .. } => {
                let bytes = value.iter().map(|(byte, _)| *byte).collect::<Vec<_>>();
                n::Pattern::Constant(n::Constant::Bytes(self.slice(&bytes)))
            }
            A::List { elements, tail, .. } => {
                let elements = elements
                    .iter()
                    .map(|item| self.pattern_inner(item))
                    .collect::<Result<Vec<_>>>()?;
                if let Some(tail) = tail {
                    let mut result = self.pattern_inner(tail)?;
                    for head in elements.iter().rev() {
                        result = self.at(span, n::Pattern::Cons { head, tail: result });
                    }
                    return Ok(result);
                }
                n::Pattern::List(self.slice(&elements))
            }
            A::Tuple { elems, .. } => {
                let elems = elems
                    .iter()
                    .map(|item| self.pattern_inner(item))
                    .collect::<Result<Vec<_>>>()?;
                match elems.as_slice() {
                    [first, second, rest @ ..] => n::Pattern::Tuple {
                        first,
                        second,
                        rest: self.slice(rest),
                    },
                    _ => return self.unsupported(span, "tuples with fewer than two elements"),
                }
            }
            A::Pair { .. } => {
                return self
                    .unsupported(span, "Pair patterns (a builtin pair is not a Nash tuple)");
            }
            A::Constructor {
                name,
                arguments,
                module,
                spread_location,
                ..
            } => {
                if spread_location.is_some() || arguments.iter().any(|arg| arg.label.is_some()) {
                    return self.unsupported(span, "labeled or spread constructor patterns");
                }
                let args = arguments
                    .iter()
                    .map(|arg| self.pattern_inner(&arg.value))
                    .collect::<Result<Vec<_>>>()?;
                let (module, name) = match module {
                    Some(Namespace::Module(module)) => (Some(self.text(module)), self.text(name)),
                    Some(Namespace::Type(..)) => {
                        return self.unsupported(span, "type-qualified constructor patterns");
                    }
                    None if !self.globals.contains(name.as_str()) => {
                        if let Some((module, original)) = self.imported.get(name.as_str()) {
                            (Some(*module), *original)
                        } else if name == "True" || name == "False" {
                            (Some("Builtin"), self.text(name))
                        } else if name == "Void" && args.is_empty() {
                            return Ok(self.at(span, n::Pattern::Unit));
                        } else {
                            (None, self.text(name))
                        }
                    }
                    None => (None, self.text(name)),
                };
                let region = self.spans.region(span);
                let args = self.slice(&args);
                match module {
                    Some(module) => n::Pattern::CtorQual {
                        region,
                        module,
                        name,
                        args,
                    },
                    None => n::Pattern::Ctor { region, name, args },
                }
            }
        };
        Ok(self.at(span, value))
    }
}
