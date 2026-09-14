use super::{Spans, unsupported};
use aiken_lang::ast::{
    Annotation, ArgBy, ArgName, Span, UntypedArg, UntypedFunction, UntypedValidator,
};
use nash_frontend::{FrontendDiagnostic, FrontendFailure};

pub(crate) fn invalid(spans: &Spans, span: Span, message: &str) -> FrontendFailure {
    FrontendDiagnostic::error(
        "NAF2301",
        "Unsupported Aiken validator",
        message,
        Some(spans.region(span)),
    )
    .into()
}

pub(crate) fn primitive(annotation: Option<&Annotation>, name: &str) -> bool {
    matches!(annotation, Some(Annotation::Constructor { module: None, name: actual, arguments, .. }) if actual == name && arguments.is_empty())
}

fn raw_data(spans: &Spans, argument: &UntypedArg) -> Result<(), FrontendFailure> {
    if argument.annotation.is_some() && !primitive(argument.annotation.as_ref(), "Data") {
        return Err(invalid(
            spans,
            argument.location,
            "This validator boundary requires raw Data (for example, transaction: Data). Custom boundary layouts are not supported.",
        ));
    }
    Ok(())
}

fn names<'a>(
    spans: &Spans,
    arguments: impl Iterator<Item = &'a UntypedArg>,
) -> Result<(), FrontendFailure> {
    let mut names = std::collections::HashSet::new();
    for arg in arguments {
        match &arg.by {
            ArgBy::ByName(ArgName::Named { name, label, .. }) if name == label => {
                if !names.insert(name) {
                    return Err(invalid(
                        spans,
                        arg.location,
                        "Validator parameters and handler arguments must have distinct names.",
                    ));
                }
            }
            ArgBy::ByName(ArgName::Discarded { name, label, .. }) if name == label => {}
            _ => {
                return Err(unsupported(
                    spans,
                    arg.location,
                    "validator argument patterns or renamed labels",
                ));
            }
        }
    }
    Ok(())
}

fn boolean(spans: &Spans, function: &UntypedFunction) -> Result<(), FrontendFailure> {
    if !primitive(function.return_annotation.as_ref(), "Bool") {
        return Err(invalid(
            spans,
            function.location,
            "Validator handlers must return Bool; False fails the script.",
        ));
    }
    Ok(())
}

pub(crate) fn validate(spans: &Spans, validator: &UntypedValidator) -> Result<(), FrontendFailure> {
    for param in &validator.params {
        raw_data(spans, param)?;
    }
    if validator.handlers.len() > 1 {
        return Err(invalid(
            spans,
            validator.location,
            "NashV1 supports one mint handler with optional else, or an else-only validator.",
        ));
    }
    if let Some(handler) = validator.handlers.first() {
        if handler.name != "mint" || handler.arguments.len() != 3 {
            return Err(invalid(
                spans,
                handler.location,
                "Supported handler: mint(redeemer: Int, policy: ByteArray, transaction: Data). Other purposes and handler arities are not supported.",
            ));
        }
        boolean(spans, handler)?;
        names(spans, validator.params.iter().chain(&handler.arguments))?;
        let redeemer = &handler.arguments[0];
        if !["Data", "Int", "ByteArray"]
            .iter()
            .any(|name| primitive(redeemer.annotation.as_ref(), name))
        {
            return Err(invalid(
                spans,
                redeemer.location,
                "Annotate the mint redeemer as Data, Int or ByteArray; other boundary conversions are not supported.",
            ));
        }
        if !primitive(redeemer.annotation.as_ref(), "Data")
            && matches!(redeemer.by, ArgBy::ByName(ArgName::Discarded { .. }))
        {
            return Err(invalid(
                spans,
                redeemer.location,
                "Bind a decoded redeemer by name, or use _: Data to discard it without decoding.",
            ));
        }
        // The official mint pattern decodes its policy field to ByteArray even
        // though the internal prelude constructor advertises Data.
        let policy = &handler.arguments[1];
        if !primitive(policy.annotation.as_ref(), "ByteArray") {
            return Err(invalid(
                spans,
                policy.location,
                "Annotate the mint policy as ByteArray, matching the official runtime boundary.",
            ));
        }
        raw_data(spans, &handler.arguments[2])?;
    }
    let fallback = &validator.fallback;
    if fallback.arguments.len() != 1 {
        return Err(invalid(
            spans,
            fallback.location,
            "The else handler takes exactly one raw Data context argument.",
        ));
    }
    raw_data(spans, &fallback.arguments[0])?;
    names(spans, validator.params.iter().chain(&fallback.arguments))?;
    boolean(spans, fallback)
}
