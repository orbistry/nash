//! Syntax reports ported from Elm's `Reporting/Error/Syntax.hs`.
//! Nash-only grammar (traits, representations, tests and do blocks) has its own reports.
mod decl;
mod expr;
mod module;
mod pattern;
#[cfg(test)]
mod tests;
mod type_;

use crate::code::{Source, to_region, to_wider_region};
use crate::{Doc, Report, Snippet};
use nash_parse::error::{Error, Space};
use nash_parse::{Col, Row};
use nash_region::{Position, Region};

pub fn to_report(source: &Source<'_>, error: &Error<'_>) -> Report {
    match error {
        Error::ModuleNameUnspecified(name) => Report {
            title: "MODULE NAME MISSING".into(), severity: crate::Severity::Error,
            region: to_region(1, 1), snippet: Snippet::None,
            before: Doc::stack([
                Doc::reflow("I need the module name to be declared at the top of this file, like this:"),
                Doc::indent(4, Doc::hsep([Doc::text("module").cyan(), Doc::text(*name), Doc::text("exposing").cyan(), Doc::text("(..)")])),
                Doc::reflow("Try adding that as the first line of your file!"),
            ]),
            after: Doc::to_simple_note("It is best to replace (..) with an explicit list of types and functions you want to expose. When you know a value is only used within this module, you can refactor without worrying about uses elsewhere. Limiting exposed values can also speed up compilation because I can skip a bunch of work if I see that the exposed API has not changed."),
            suggestions: Vec::new(),
        },
        Error::ModuleNameMismatch { expected, actual, row, col } => Report::snippet(
            "MODULE NAME MISMATCH", to_wider_region(*row, *col, actual.len()), None,
            Doc::text("It looks like this module name is out of sync:"),
            Doc::stack([
                Doc::reflow(&format!("I need it to match the file path, so I was expecting to see `{expected}` here. Make the following change, and you should be all set!")),
                Doc::indent(4, Doc::cat([Doc::text(*actual).dullyellow(), Doc::text(" -> "), Doc::text(*expected).green()])),
                Doc::to_simple_note("I require that module names correspond to file paths. This makes it much easier to explore unfamiliar codebases! So if you want to keep the current module name, try renaming the file instead."),
            ]),
        ).with_suggestions(vec![expected.to_string()]),
        Error::ParseError(error) => module::to_parse_error_report(source, error),
    }
}

pub(crate) fn problem(title: &str, row: Row, col: Col, before: &str, after: &str) -> Report {
    Report::snippet(
        title,
        to_region(row, col),
        None,
        Doc::reflow(before),
        Doc::reflow(after),
    )
}

pub(crate) fn wide(mut report: Report, row: Row, col: Col) -> Report {
    let highlight = report.region;
    let start = Position::new(row, col).min(highlight.start);
    report.snippet = Snippet::Region {
        region: Region::new(start, highlight.end),
        highlight: Some(highlight),
    };
    report
}

pub(crate) fn to_space_report(_source: &Source<'_>, space: &Space, row: Row, col: Col) -> Report {
    match space {
        Space::TooDeep => problem(
            "EXCESSIVE NESTING",
            row,
            col,
            "This expression, pattern, or type is nested too deeply.",
            "Split it into smaller definitions or simplify its nesting.",
        ),
        Space::HasTab => problem(
            "NO TABS",
            row,
            col,
            "I ran into a tab, but tabs are not allowed in Nash files.",
            "Replace the tab with spaces.",
        ),
        Space::EndlessMultiComment => Report::snippet(
            "ENDLESS COMMENT",
            to_wider_region(row, col, 2),
            None,
            Doc::reflow("I cannot find the end of this multi-line comment:"),
            Doc::stack([
                Doc::reflow("Add a -} somewhere after this to end the comment."),
                Doc::to_simple_hint(
                    "Multi-line comments can be nested in Nash, so {- {- -} -} is a comment that happens to contain another comment. Like parentheses and curly braces, the start and end markers must always be balanced. Maybe that is the problem?",
                ),
            ]),
        ),
    }
}

#[cfg(test)]
mod variants;
