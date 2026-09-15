pub(crate) mod validators;

use crate::{profile, spans::Spans};
use aiken_lang::ast::{Definition, Span, UntypedModule};
use nash_frontend::{FrontendDiagnostic, FrontendFailure, SourceInput};

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
    _input: SourceInput<'_, '_>,
    ast: &UntypedModule,
    spans: &Spans,
) -> Result<(), FrontendFailure> {
    let mut validator_names = std::collections::HashSet::new();
    for definition in &ast.definitions {
        if let Definition::Validator(validator) = definition {
            if !validator_names.insert(validator.name.as_str()) {
                return Err(validators::invalid(
                    spans,
                    validator.location,
                    "A validator name may be declared only once in a module.",
                ));
            }
            validators::validate(spans, validator)?;
        }
    }
    let mut type_names = std::collections::HashSet::new();
    let mut imports = std::collections::HashSet::new();
    let mut unqualified = std::collections::HashSet::new();
    for def in &ast.definitions {
        match def {
            Definition::Test(_) | Definition::Benchmark(_) => {}
            Definition::DataType(data) => {
                data_layout(spans, data)?;
                if !type_names.insert(&data.name) {
                    return Err(unsupported(
                        spans,
                        data.location,
                        "duplicate type declarations",
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
                if !type_names.insert(&alias.alias) {
                    return Err(unsupported(
                        spans,
                        alias.location,
                        "duplicate type declarations",
                    ));
                }
            }
            Definition::Use(import) => {
                let qualifier = import
                    .as_name
                    .as_deref()
                    .or_else(|| import.module.last().map(String::as_str))
                    .unwrap_or("");
                if validator_names.contains(qualifier) {
                    return Err(validators::invalid(
                        spans,
                        import.location,
                        "A validator name conflicts with this module qualifier.",
                    ));
                }
                if !imports.insert(qualifier) {
                    return Err(unsupported(
                        spans,
                        import.location,
                        "duplicate module qualifiers",
                    ));
                }
                for item in &import.unqualified.1 {
                    if profile::is_prelude(&import.module)
                        && profile::primitive(&item.name).is_none()
                        && profile::prelude_constructor(&item.name).is_none()
                        && profile::prelude_value(&item.name).is_none()
                        && !matches!(item.name.as_str(), "Pairs" | "Fuzzer" | "Sampler")
                    {
                        return Err(FrontendDiagnostic::error(
                            "NAF3003",
                            "Unknown Aiken prelude member",
                            format!("The Aiken 1.1.23 prelude does not export `{}`.", item.name),
                            Some(spans.region(item.location)),
                        )
                        .into());
                    }
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
            Definition::Fn(_) | Definition::ModuleConstant(_) | Definition::Validator(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn layout_error(spans: &Spans, span: Span, message: &str) -> FrontendFailure {
    FrontendDiagnostic::error(
        "NAF2401",
        "Invalid Aiken data layout",
        message,
        Some(spans.region(span)),
    )
    .into()
}

pub(crate) fn data_layout(
    spans: &Spans,
    data: &aiken_lang::ast::UntypedDataType,
) -> Result<(nash_source::DataEncoding, Vec<u64>), FrontendFailure> {
    use aiken_lang::ast::DecoratorKind;
    use nash_source::DataEncoding;
    let invalid = |span, message| layout_error(spans, span, message);
    if data.constructors.is_empty() || data.constructors.len() > u16::MAX as usize {
        return Err(invalid(
            data.location,
            "The constructor count cannot be represented.",
        ));
    }
    if data.decorators.len() > 1 {
        return Err(invalid(
            data.decorators[1].location,
            "Conflicting type decorators.",
        ));
    }
    if data.constructors.len() != 1 && !data.decorators.is_empty() {
        return Err(invalid(
            data.decorators[0].location,
            "Type decorators require a single-constructor type.",
        ));
    }
    let mut encoding = DataEncoding::Constr;
    let mut record_tag = None;
    if let Some(decorator) = data.decorators.first() {
        match decorator.kind {
            DecoratorKind::List => encoding = DataEncoding::List,
            DecoratorKind::Tag { value, .. } => record_tag = Some(value as u64),
        }
    }
    let mut tags = Vec::with_capacity(data.constructors.len());
    for (index, ctor) in data.constructors.iter().enumerate() {
        if ctor.arguments.len() > u16::MAX as usize {
            return Err(invalid(
                ctor.location,
                "The constructor field count cannot be represented.",
            ));
        }
        if ctor.decorators.len() > 1 {
            return Err(invalid(
                ctor.decorators[1].location,
                "Conflicting constructor decorators.",
            ));
        }
        let tag = match ctor.decorators.first() {
            Some(decorator) => match decorator.kind {
                DecoratorKind::Tag { value, .. } => value as u64,
                DecoratorKind::List => {
                    return Err(invalid(
                        decorator.location,
                        "@list applies to a single-constructor type, not a constructor.",
                    ));
                }
            },
            None => record_tag.unwrap_or(index as u64),
        };
        if tags.contains(&tag) {
            return Err(invalid(
                ctor.location,
                "Constructor tags must be distinct, including default tags.",
            ));
        }
        tags.push(tag);
    }
    if data.opaque
        && data.constructors.len() == 1
        && data.constructors[0].arguments.len() == 1
        && encoding != DataEncoding::List
    {
        encoding = DataEncoding::Transparent;
    }
    Ok((encoding, tags))
}
