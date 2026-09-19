//! `Reporting/Error/Pattern.hs`.
use crate::doc::int_to_ordinal;
use crate::{Doc, Report};

use nash_nitpick::render::{RenderContext, pattern_to_string};
use nash_nitpick::{Context, Error, Pattern};

pub fn to_report(error: &Error<'_>) -> Report {
    match error {
        Error::Redundant {
            case_region,
            pattern_region,
            index,
        } => Report::snippet(
            "REDUNDANT PATTERN",
            *pattern_region,
            None,
            Doc::text(format!(
                "The {} pattern is unreachable.",
                int_to_ordinal(*index)
            )),
            Doc::text("Remove it; earlier patterns cover every matching value."),
        )
        .with_region(*case_region)
        .with_code("nash::pattern::redundant"),
        Error::Incomplete {
            region,
            context,
            unhandled,
        } => {
            let (title, message, hint) = match context {
                Context::BadArg => (
                    "UNSAFE PATTERN",
                    "Argument pattern is not exhaustive.",
                    "Use a case expression in the function body to handle the missing patterns.",
                ),
                Context::BadDestruct => (
                    "UNSAFE PATTERN",
                    "Binding pattern is not exhaustive.",
                    "Use a case expression to handle the missing patterns.",
                ),
                Context::BadCase => (
                    "MISSING PATTERNS",
                    "Case expression is not exhaustive.",
                    "Add the missing branches; use `todo` for unfinished bodies.",
                ),
            };
            Report::snippet(
                title,
                *region,
                None,
                Doc::text(message),
                Doc::stack([
                    Doc::text("Missing patterns:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::text(hint),
                ]),
            )
            .with_code("nash::pattern::incomplete")
        }
    }
}

fn unhandled_patterns_to_doc_block(unhandled: &[Pattern<'_>]) -> Doc {
    Doc::indent(
        4,
        Doc::vcat(
            unhandled
                .iter()
                .map(|p| Doc::text(pattern_to_string(RenderContext::Unambiguous, *p))),
        )
        .dullyellow(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_witnesses_escape_source_text() {
        use nash_nitpick::Literal;
        let patterns = [
            Pattern::Anything,
            Pattern::Literal(Literal::Int(-7)),
            Pattern::Literal(Literal::Str("a\n\"b")),
            Pattern::Literal(Literal::Bytes(&[0, 255])),
        ];
        let doc = unhandled_patterns_to_doc_block(&patterns);
        insta::assert_snapshot!(doc.render(80, false));
        assert!(doc.chunks(80).iter().any(|c| matches!(c,crate::doc::Chunk::Styled{style,..} if style.color == Some(crate::doc::Color{base:crate::doc::BaseColor::Yellow,vivid:false}))));
    }
}
