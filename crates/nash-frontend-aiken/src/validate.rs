pub(crate) mod validators;

use crate::{profile, spans::Spans};
use aiken_lang::ast::{Definition, Span, UntypedModule};
use nash_frontend::{FrontendDiagnostic, FrontendFailure, ModuleRole, SourceInput};

pub(crate) fn unsupported(spans: &Spans, span: Span, feature: &str) -> FrontendFailure {
    FrontendDiagnostic::error(
        "NAF2201",
        "Unsupported Aiken feature",
        format!("NashV1 does not support {feature}."),
        Some(spans.region(span)),
    )
    .into()
}

pub(crate) fn module(
    input: SourceInput<'_, '_>,
    ast: &UntypedModule,
    spans: &Spans,
) -> Result<(), FrontendFailure> {
    let mut validators = ast.definitions.iter().filter_map(|def| match def {
        Definition::Validator(validator) => Some(validator),
        _ => None,
    });
    if let Some(validator) = validators.next() {
        if input.role == Some(ModuleRole::Library) || validators.next().is_some() {
            return Err(validators::invalid(
                spans,
                validator.location,
                "A validator source must contain exactly one validator and cannot have library role metadata.",
            ));
        }
        if ast.definitions.iter().any(|def| match def {
            Definition::Fn(fun) => fun.name == "main",
            Definition::ModuleConstant(value) => value.name == "main",
            _ => false,
        }) {
            return Err(validators::invalid(
                spans,
                validator.location,
                "The name main is reserved for the lowered validator entry point.",
            ));
        }
        for def in &ast.definitions {
            if let Definition::Use(import) = def {
                for item in &import.unqualified.1 {
                    if matches!(item.variable_name(), "Data" | "Int" | "ByteArray" | "Bool") {
                        return Err(validators::invalid(
                            spans,
                            item.location,
                            "Validator boundary primitive names cannot be shadowed by imports; use a qualified import instead.",
                        ));
                    }
                }
            }
        }
        validators::validate(spans, validator)?;
    } else if input.role == Some(ModuleRole::Validator) {
        return Err(validators::invalid(
            spans,
            Span { start: 0, end: 0 },
            "Validator role metadata requires a validator declaration.",
        ));
    }
    if matches!(
        input.expected_module.as_str().split('.').next(),
        Some("env" | "config")
    ) {
        return Err(unsupported(
            spans,
            Span {
                start: 0,
                end: input.source.len(),
            },
            "environment or configuration modules",
        ));
    }
    let mut type_names = std::collections::HashSet::new();
    let mut imports = std::collections::HashSet::new();
    let mut unqualified = std::collections::HashSet::new();
    for def in &ast.definitions {
        match def {
            Definition::Test(_) => {
                return Err(unsupported(spans, def.location(), "test declarations"));
            }
            Definition::Benchmark(_) => {
                return Err(unsupported(spans, def.location(), "benchmark declarations"));
            }
            Definition::DataType(data) => {
                if let Some(decorator) = data.decorators.first().or_else(|| {
                    data.constructors
                        .iter()
                        .find_map(|ctor| ctor.decorators.first())
                }) {
                    return Err(unsupported(
                        spans,
                        decorator.location,
                        "custom data encoding decorators",
                    ));
                }
                if profile::primitive(&data.name).is_some()
                    || !type_names.insert(profile::user_type(&data.name))
                {
                    return Err(unsupported(
                        spans,
                        data.location,
                        "colliding or primitive-shadowing type declarations",
                    ));
                }
                for ctor in &data.constructors {
                    let labeled = ctor
                        .arguments
                        .iter()
                        .filter(|arg| arg.label.is_some())
                        .count();
                    if labeled != 0 && labeled != ctor.arguments.len() {
                        return Err(unsupported(
                            spans,
                            ctor.location,
                            "mixed labeled and positional constructor fields",
                        ));
                    }
                }
            }
            Definition::TypeAlias(alias) => {
                if profile::primitive(&alias.alias).is_some()
                    || !type_names.insert(profile::user_type(&alias.alias))
                {
                    return Err(unsupported(
                        spans,
                        alias.location,
                        "colliding or primitive-shadowing type declarations",
                    ));
                }
            }
            Definition::Use(import) => {
                let qualifier = import
                    .as_name
                    .as_deref()
                    .or_else(|| import.module.last().map(String::as_str))
                    .unwrap_or("");
                if !imports.insert(qualifier) {
                    return Err(unsupported(
                        spans,
                        import.location,
                        "duplicate module qualifiers",
                    ));
                }
                for item in &import.unqualified.1 {
                    if profile::is_builtin(&import.module)
                        && profile::builtin_value(&item.name).is_none()
                    {
                        return Err(unsupported(
                            spans,
                            item.location,
                            &format!("builtin `{}`", item.name),
                        ));
                    }
                    if !unqualified.insert(item.variable_name()) {
                        return Err(unsupported(
                            spans,
                            item.location,
                            "duplicate unqualified import names",
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
