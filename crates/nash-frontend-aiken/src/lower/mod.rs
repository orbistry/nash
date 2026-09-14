mod declarations;
mod expressions;
mod patterns;
mod types;
mod validators;

use crate::{profile, spans::Spans, validate};
use aiken_lang::ast::{Definition, Span, UntypedModule};
use bumpalo::Bump;
use nash_frontend::FrontendFailure;
use nash_region::Located;
use nash_source as n;
use std::collections::{HashMap, HashSet};

type Expr<'a> = &'a Located<n::Expr<'a>>;
type Pattern<'a> = &'a Located<n::Pattern<'a>>;
type Type<'a> = &'a Located<n::Type<'a>>;
type Result<T, E = FrontendFailure> = std::result::Result<T, E>;

pub(crate) struct Lower<'a, 's> {
    arena: &'a Bump,
    spans: &'s Spans,
    imported: HashMap<&'s str, (&'a str, &'a str)>,
    qualifiers: HashSet<&'s str>,
    builtin_qualifiers: HashSet<&'s str>,
    globals: HashSet<&'s str>,
    validator_name: Option<&'s str>,
    locals: Vec<(String, &'a str)>,
    serial: usize,
}

impl<'a, 's> Lower<'a, 's> {
    pub(crate) fn new(arena: &'a Bump, spans: &'s Spans, ast: &'s UntypedModule) -> Self {
        let mut imported = HashMap::new();
        let mut qualifiers = HashSet::new();
        let mut builtin_qualifiers = HashSet::new();
        let mut globals = HashSet::new();
        let mut validator_name = None;
        for def in &ast.definitions {
            match def {
                Definition::Use(import) => {
                    let qualifier = import
                        .as_name
                        .as_deref()
                        .or_else(|| import.module.last().map(String::as_str))
                        .unwrap_or("");
                    qualifiers.insert(qualifier);
                    if profile::is_builtin(&import.module) {
                        builtin_qualifiers.insert(qualifier);
                    }
                    let qualifier: &str = arena.alloc_str(qualifier);
                    for item in &import.unqualified.1 {
                        if profile::is_builtin(&import.module) {
                            if let Some(name) = profile::builtin_value(&item.name) {
                                imported.insert(item.variable_name(), (qualifier, name));
                            }
                        } else {
                            imported.insert(
                                item.variable_name(),
                                (qualifier, &*arena.alloc_str(&item.name)),
                            );
                        }
                    }
                }
                Definition::Fn(fun) => {
                    globals.insert(fun.name.as_str());
                }
                Definition::ModuleConstant(value) => {
                    globals.insert(value.name.as_str());
                }
                Definition::DataType(data) => {
                    globals.extend(data.constructors.iter().map(|ctor| ctor.name.as_str()));
                }
                Definition::Validator(validator) => {
                    validator_name = Some(validator.name.as_str());
                }
                _ => {}
            }
        }
        Self {
            arena,
            spans,
            imported,
            qualifiers,
            builtin_qualifiers,
            globals,
            validator_name,
            locals: vec![],
            serial: 0,
        }
    }

    fn alloc<T>(&self, value: T) -> &'a T {
        self.arena.alloc(value)
    }
    fn text(&self, text: &str) -> &'a str {
        self.arena.alloc_str(text)
    }
    fn slice<T: Copy>(&self, values: &[T]) -> &'a [T] {
        self.arena.alloc_slice_copy(values)
    }
    fn at<T>(&self, span: Span, value: T) -> &'a Located<T> {
        self.alloc(Located::at(self.spans.region(span), value))
    }
    fn unsupported<T>(&self, span: Span, feature: &str) -> Result<T> {
        Err(validate::unsupported(self.spans, span, feature))
    }
    fn fresh(&mut self) -> &'a str {
        let name = self.text(&format!("$aiken{}", self.serial));
        self.serial += 1;
        name
    }
    fn bind(&mut self, name: &str) -> &'a str {
        let fresh = self.fresh();
        self.locals.push((name.to_owned(), fresh));
        fresh
    }
    fn local(&self, name: &str) -> Option<&'a str> {
        self.locals
            .iter()
            .rev()
            .find_map(|(original, local)| (original == name).then_some(*local))
    }
    fn var(&self, span: Span, name: &'a str) -> Expr<'a> {
        self.at(
            span,
            n::Expr::Var {
                kind: n::VarType::LowVar,
                name,
            },
        )
    }
    fn builtin(&self, span: Span, name: &'static str) -> Expr<'a> {
        self.at(
            span,
            n::Expr::VarQual {
                kind: n::VarType::LowVar,
                module: "Builtin",
                name,
            },
        )
    }
    fn call(&self, span: Span, function: Expr<'a>, arguments: &[Expr<'a>]) -> Expr<'a> {
        self.at(
            span,
            n::Expr::Call {
                function,
                arguments: self.slice(arguments),
            },
        )
    }
    fn bool(&self, span: Span, value: bool) -> Expr<'a> {
        self.at(
            span,
            n::Expr::VarQual {
                kind: n::VarType::CapVar,
                module: "Builtin",
                name: if value { "True" } else { "False" },
            },
        )
    }
    fn if_(&self, span: Span, condition: Expr<'a>, yes: Expr<'a>, no: Expr<'a>) -> Expr<'a> {
        self.at(
            span,
            n::Expr::If {
                branches: self.slice(&[self.alloc(n::IfBranch {
                    condition,
                    then_branch: yes,
                })]),
                final_else: no,
            },
        )
    }
    fn not(&self, span: Span, value: Expr<'a>) -> Expr<'a> {
        self.if_(span, value, self.bool(span, false), self.bool(span, true))
    }
    fn apply_pattern(
        &self,
        span: Span,
        pattern: Pattern<'a>,
        value: Expr<'a>,
        body: Expr<'a>,
    ) -> Expr<'a> {
        self.call(
            span,
            self.at(
                span,
                n::Expr::Lambda {
                    parameters: self.slice(&[pattern]),
                    body,
                },
            ),
            &[value],
        )
    }
    fn ascribe(&mut self, span: Span, body: Expr<'a>, typ: Type<'a>) -> Expr<'a> {
        let name = self.fresh();
        let def = self.at(
            span,
            n::Def::Define {
                name: self.at(span, name),
                args: &[],
                body,
                annotation: Some(self.alloc(n::Annotation {
                    constraints: &[],
                    typ,
                })),
            },
        );
        self.at(
            span,
            n::Expr::Let {
                defs: self.slice(&[def]),
                body: self.var(span, name),
            },
        )
    }
    fn kind(name: &str) -> n::VarType {
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            n::VarType::CapVar
        } else {
            n::VarType::LowVar
        }
    }
    fn value_name(&self, span: Span, name: &str) -> Expr<'a> {
        if let Some(local) = self.local(name) {
            return self.var(span, local);
        }
        if !self.globals.contains(name) {
            if let Some((module, original)) = self.imported.get(name) {
                return self.at(
                    span,
                    n::Expr::VarQual {
                        kind: Self::kind(original),
                        module,
                        name: original,
                    },
                );
            }
            match name {
                "True" => return self.bool(span, true),
                "False" => return self.bool(span, false),
                "Void" => return self.at(span, n::Expr::Unit),
                _ => {}
            }
        }
        self.at(
            span,
            n::Expr::Var {
                kind: Self::kind(name),
                name: self.text(name),
            },
        )
    }
    fn integer(&self, span: Span, value: &str) -> Result<i128> {
        value.parse::<i128>().map_err(|_| {
            nash_frontend::FrontendDiagnostic::error(
                "NAF2101",
                "Aiken integer out of range",
                "Integer literals must fit Nash's temporary signed 128-bit source representation.",
                Some(self.spans.region(span)),
            )
            .into()
        })
    }
}
