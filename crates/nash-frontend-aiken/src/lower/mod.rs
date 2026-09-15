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
    prelude_qualifiers: HashSet<&'s str>,
    imported_builtin_arities: HashMap<&'s str, usize>,
    globals: HashSet<&'s str>,
    local_types: HashSet<&'s str>,
    validator_handlers: HashMap<&'s str, HashMap<&'s str, &'a str>>,
    pub(crate) entry_points: Vec<nash_frontend::SourceEntryPoint<'a>>,
    extra_values: Vec<&'a Located<n::Value<'a>>>,
    locals: Vec<(String, &'a str)>,
    serial: usize,
}

impl<'a, 's> Lower<'a, 's> {
    pub(crate) fn new(arena: &'a Bump, spans: &'s Spans, ast: &'s UntypedModule) -> Self {
        let mut imported = HashMap::new();
        let mut qualifiers = HashSet::new();
        let mut builtin_qualifiers = HashSet::new();
        let mut prelude_qualifiers = HashSet::new();
        let mut imported_builtin_arities = HashMap::new();
        let mut globals = HashSet::new();
        let mut local_types = HashSet::new();
        let mut validator_handlers = HashMap::new();
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
                    if profile::is_prelude(&import.module) {
                        prelude_qualifiers.insert(qualifier);
                    }
                    let qualifier: &str = arena.alloc_str(qualifier);
                    for item in &import.unqualified.1 {
                        if profile::is_builtin(&import.module) {
                            if let Some(name) = profile::builtin_value(&item.name) {
                                imported.insert(item.variable_name(), (qualifier, name));
                                if let Some(arity) = profile::builtin_arity(&item.name) {
                                    imported_builtin_arities.insert(item.variable_name(), arity);
                                }
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
                    local_types.insert(data.name.as_str());
                    globals.extend(data.constructors.iter().map(|ctor| ctor.name.as_str()));
                }
                Definition::TypeAlias(alias) => {
                    local_types.insert(alias.alias.as_str());
                }
                Definition::Validator(validator) => {
                    validator_handlers.insert(
                        validator.name.as_str(),
                        validator
                            .handlers
                            .iter()
                            .chain(std::iter::once(&validator.fallback))
                            .map(|handler| {
                                (
                                    handler.name.as_str(),
                                    &*arena.alloc_str(&format!(
                                        "$validator.{}.{}",
                                        validator.name, handler.name
                                    )),
                                )
                            })
                            .collect(),
                    );
                }
                Definition::Test(_) | Definition::Benchmark(_) => {}
            }
        }
        Self {
            arena,
            spans,
            imported,
            qualifiers,
            builtin_qualifiers,
            prelude_qualifiers,
            imported_builtin_arities,
            globals,
            local_types,
            validator_handlers,
            entry_points: Vec::new(),
            extra_values: Vec::new(),
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
    fn validator_handler(&self, span: Span, qualifier: &str, name: &str) -> Option<Expr<'a>> {
        if self.local(qualifier).is_some()
            || self.globals.contains(qualifier)
            || self.qualifiers.contains(qualifier)
        {
            return None;
        }
        self.validator_handlers
            .get(qualifier)?
            .get(name)
            .map(|name| self.var(span, name))
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
    fn conversion_ascribe(
        &self,
        span: Span,
        body: Expr<'a>,
        typ: Type<'a>,
        site: n::ConversionSite,
    ) -> Expr<'a> {
        self.at(
            span,
            n::Expr::Convert {
                kind: n::ConversionKind::Ascription(site),
                typ,
                value: body,
            },
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
                if self.prelude_qualifiers.contains(*module)
                    && let Some(value) = self.prelude_reference(span, module, original)
                {
                    return value;
                }
                let value = self.at(
                    span,
                    n::Expr::VarQual {
                        kind: Self::kind(original),
                        module,
                        name: original,
                    },
                );
                return if let Some(arity) = self.imported_builtin_arities.get(name) {
                    self.grouped_builtin(span, value, *arity)
                } else {
                    value
                };
            }
            if let Some(value) = self.prelude_reference(span, "Builtin", name) {
                return value;
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
    fn prelude_reference(&self, span: Span, module: &'a str, name: &str) -> Option<Expr<'a>> {
        if name == "Void" {
            return Some(self.at(span, n::Expr::Unit));
        }
        if let Some((_, constructor)) = profile::prelude_constructor(name) {
            return Some(self.at(
                span,
                n::Expr::VarQual {
                    kind: n::VarType::CapVar,
                    module,
                    name: constructor,
                },
            ));
        }
        let value = profile::prelude_value(name)?;
        let arity = profile::prelude_arity(name)?;
        let reference = self.at(
            span,
            n::Expr::VarQual {
                kind: n::VarType::LowVar,
                module,
                name: value,
            },
        );
        Some(self.grouped_builtin(span, reference, arity))
    }
    fn integer(&self, span: Span, value: &str) -> Result<n::Constant<'a>> {
        if let Ok(value) = value.parse::<i128>() {
            return Ok(n::Constant::Int(value));
        }
        let value = value.parse::<num_bigint::BigInt>().map_err(|_| {
            nash_frontend::FrontendFailure::from(nash_frontend::FrontendDiagnostic::error(
                "NAF2101",
                "Invalid Aiken integer",
                "The integer could not be represented as decimal digits.",
                Some(self.spans.region(span)),
            ))
        })?;
        Ok(n::Constant::BigInt(self.text(&value.to_string())))
    }
}

impl<'a> Lower<'a, '_> {
    fn module_docs(&self, ast: &UntypedModule, span: Span) -> &'a n::Docs<'a> {
        let comment = |text: &str, location: Span| {
            let position = self.spans.region(location).start;
            self.alloc(n::Comment(self.alloc(n::Snippet {
                data: self.text(text).as_bytes(),
                off_row: position.line,
                off_col: position.column,
            })))
        };
        let mut comments = Vec::new();
        for definition in &ast.definitions {
            let (name, doc) = match definition {
                Definition::Fn(value) => (&value.name, value.doc.as_deref()),
                Definition::Test(value) | Definition::Benchmark(value) => {
                    (&value.name, value.doc.as_deref())
                }
                Definition::ModuleConstant(value) => (&value.name, value.doc.as_deref()),
                Definition::DataType(value) => (&value.name, value.doc.as_deref()),
                Definition::TypeAlias(value) => (&value.alias, value.doc.as_deref()),
                Definition::Validator(value) => (&value.name, value.doc.as_deref()),
                Definition::Use(_) => continue,
            };
            if let Some(doc) = doc {
                comments.push(self.alloc((self.text(name), comment(doc, definition.location()))));
            }
        }
        if ast.docs.is_empty() && comments.is_empty() {
            return self.alloc(n::Docs::NoDocs(self.spans.region(span)));
        }
        self.alloc(n::Docs::YesDocs {
            overview: comment(&ast.docs.join("\n"), span),
            comments: self.slice(&comments),
        })
    }
}
