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
    use crate::{Source, render_plain};

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
        let module = &can.module;
        nash_solve::run(&bump, &mut uf, module, &can.tables)
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
        assert!(report.context.is_some_and(|region| region.start.line == 3));
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
