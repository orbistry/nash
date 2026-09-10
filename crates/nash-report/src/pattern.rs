//! `Reporting/Error/Pattern.hs`.
use crate::doc::int_to_ordinal;
use crate::{Doc, Report};

use nash_nitpick::render::{RenderContext, pattern_to_string};
use nash_nitpick::{Context, Error, Pattern};

pub fn to_report(error: &Error<'_>) -> Report {
    match error {
        Error::Redundant { case_region, pattern_region, index } => Report::snippet(
            "REDUNDANT PATTERN",
            *pattern_region,
            Some(*pattern_region),
            Doc::reflow(&format!("The {} pattern is redundant:", int_to_ordinal(*index))),
            Doc::reflow("Any value with this shape will be handled by a previous pattern, so it should be removed."),
        )
        .with_region(*case_region),

        Error::Incomplete { region, context, unhandled } => match context {
            Context::BadArg => Report::snippet(
                "UNSAFE PATTERN",
                *region,
                None,
                Doc::text("This pattern does not cover all possibilities:"),
                Doc::stack([
                    Doc::text("Other possibilities include:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::reflow(
                        "I would have to crash if I saw one of those! So rather than pattern matching in \
                         function arguments, put a `case` in the function body to account for all possibilities.",
                    ),
                ]),
            ),
            Context::BadDestruct => Report::snippet(
                "UNSAFE PATTERN",
                *region,
                None,
                Doc::text("This pattern does not cover all possible values:"),
                Doc::stack([
                    Doc::text("Other possibilities include:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::reflow(
                        "I would have to crash if I saw one of those! You can use `let` to deconstruct values \
                         only if there is ONE possibility. Switch to a `case` expression to account for all \
                         possibilities.",
                    ),
                    Doc::to_simple_hint(
                        "Are you calling a function that definitely returns values with a very specific shape? \
                         Try making the return type of that function more specific!",
                    ),
                ]),
            ),
            Context::BadCase => Report::snippet(
                "MISSING PATTERNS",
                *region,
                None,
                Doc::text("This `case` does not have branches for all possibilities:"),
                Doc::stack([
                    Doc::text("Missing possibilities include:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::reflow("I would have to crash if I saw one of those. Add branches for them!"),
                    Doc::link(
                        "Hint",
                        "If you want to write the code for each branch later, use `todo` as a placeholder. Read",
                        "missing-patterns",
                        "for more guidance on this workflow.",
                    ),
                ]),
            ),
        },
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
    use crate::{Snippet, Source, render_plain};

    fn reports(input: &str) -> Vec<Report> {
        let bump = bumpalo::Bump::new();
        let text = bump.alloc_str(input);
        let module = nash_parse::Parser::new(&bump, text)
            .module()
            .expect("parse");
        let interfaces = std::collections::BTreeMap::from([(
            "Builtin",
            nash_can::kinds::builtin_interface(&bump),
        )]);
        let can = nash_can::canonicalize(
            &bump,
            nash_can::Context {
                package: None,
                interfaces: Some(&interfaces),
            },
            &module,
        )
        .expect("canonicalize");
        let mut uf = nash_constrain::UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &can.module);
        nash_solve::run(&bump, &mut uf, &constraint, &can.tables)
            .expect("solve before checking coverage");
        nash_nitpick::check(&bump, &can.module)
            .expect_err("expected pattern errors")
            .iter()
            .map(to_report)
            .collect()
    }
    fn render(input: &str) -> String {
        let reports = reports(input);
        assert_eq!(reports.len(), 1);
        render_plain(&reports[0], &Source::new(input), "src/Main.nash")
    }
    #[test]
    fn missing_patterns_data() {
        insta::assert_snapshot!(render(indoc::indoc! {r#"
            module Main exposing (..)
            import Builtin exposing (Data(..))
            tag d =
                case d of
                    Constr _ _ -> ()
                    List _ -> ()
        "#}));
    }
    #[test]
    fn missing_patterns_nested_list() {
        insta::assert_snapshot!(render(indoc::indoc! {r#"
            module Main exposing (..)
            first xs =
                case xs of
                    [] -> 0
                    [] :: _ -> 1
        "#}));
    }
    #[test]
    fn unsafe_arg() {
        insta::assert_snapshot!(render("module Main exposing (..)\nf [x] = x\n"));
    }
    #[test]
    fn unsafe_destruct() {
        insta::assert_snapshot!(render(indoc::indoc! {r#"
            module Main exposing (..)
            f xs =
                let
                    [x] = xs
                in
                x
        "#}));
    }
    #[test]
    fn redundant_pattern() {
        let input = indoc::indoc! {r#"
            module Main exposing (..)
            f xs =
                case xs of
                    _ -> 0
                    [] -> 1
        "#};
        let reports = reports(input);
        assert_eq!(reports.len(), 1);
        let report = &reports[0];
        assert_eq!(report.region.start.line, 5);
        assert!(
            matches!(report.snippet, Snippet::Region{region,highlight:Some(h)} if region.start.line == 3 && h == report.region)
        );
        insta::assert_snapshot!(render_plain(report, &Source::new(input), "src/Main.nash"));
    }
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
