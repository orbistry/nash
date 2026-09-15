use super::Spans;
use aiken_lang::ast::{ArgBy, ArgName, Span, UntypedArg, UntypedValidator};
use nash_frontend::{FrontendDiagnostic, FrontendFailure};

pub(crate) fn invalid(spans: &Spans, span: Span, message: &str) -> FrontendFailure {
    FrontendDiagnostic::error(
        "NAF2301",
        "Invalid Aiken validator",
        message,
        Some(spans.region(span)),
    )
    .into()
}

fn names<'a>(
    spans: &Spans,
    arguments: impl Iterator<Item = &'a UntypedArg>,
) -> Result<(), FrontendFailure> {
    let mut names = std::collections::HashSet::new();
    let mut labels = std::collections::HashSet::new();
    for (index, argument) in arguments.enumerate() {
        if !labels.insert(argument.arg_name(index).get_label()) {
            return Err(invalid(
                spans,
                argument.location,
                "Validator argument labels must be distinct, including discarded argument labels.",
            ));
        }
        if let ArgBy::ByName(ArgName::Named { name, .. }) = &argument.by
            && !names.insert(name)
        {
            return Err(invalid(
                spans,
                argument.location,
                "Validator parameters and handler arguments must have distinct names.",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate(spans: &Spans, validator: &UntypedValidator) -> Result<(), FrontendFailure> {
    let mut handlers = std::collections::HashSet::new();
    for handler in &validator.handlers {
        let Some((_, arity, _)) = purpose(&handler.name) else {
            return Err(invalid(
                spans,
                handler.location,
                "Unknown validator purpose.",
            ));
        };
        if handler.arguments.len() != arity || !handlers.insert(&handler.name) {
            return Err(invalid(
                spans,
                handler.location,
                "A purpose may have one handler, with four arguments for spend and three for other purposes.",
            ));
        }
        names(spans, validator.params.iter().chain(&handler.arguments))?;
    }
    let fallback = &validator.fallback;
    if handlers.len() == 6 && *fallback != UntypedValidator::default_fallback(fallback.location) {
        return Err(invalid(
            spans,
            fallback.location,
            "An explicit else handler is redundant when all six purposes are handled.",
        ));
    }
    if fallback.arguments.len() != 1 {
        return Err(invalid(
            spans,
            fallback.location,
            "The else handler takes exactly one context argument.",
        ));
    }
    names(spans, validator.params.iter().chain(&fallback.arguments))
}

/// V3 purpose tag, handler arity, and purpose argument's field index.
pub(crate) fn purpose(name: &str) -> Option<(u64, usize, usize)> {
    Some(match name {
        "mint" => (0, 3, 0),
        "spend" => (1, 4, 0),
        "withdraw" => (2, 3, 0),
        "publish" => (3, 3, 1),
        "vote" => (4, 3, 0),
        "propose" => (5, 3, 1),
        _ => return None,
    })
}
