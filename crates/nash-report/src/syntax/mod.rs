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
use crate::{Doc, Report};
use nash_parse::error::{Error, Space};
use nash_parse::{Col, Row};
use nash_region::{Position, Region};

pub fn to_report(source: &Source<'_>, error: &Error<'_>) -> Report {
    match error {
        Error::ModuleNameUnspecified(name) => Report::snippet(
            "MODULE NAME MISSING",
            to_region(1, 1),
            None,
            Doc::text("Missing module declaration."),
            Doc::text(format!(
                "Add `module {name} exposing (..)` at the start of the file."
            )),
        )
        .without_source()
        .with_code("nash::syntax::missing_module_name"),
        Error::ModuleNameMismatch {
            expected,
            actual,
            row,
            col,
        } => Report::snippet(
            "MODULE NAME MISMATCH",
            to_wider_region(*row, *col, actual.len()),
            None,
            Doc::text(format!(
                "Module name must be `{expected}`, found `{actual}`."
            )),
            Doc::text("Make the module name match its file path."),
        )
        .with_code("nash::syntax::module_name_mismatch")
        .with_suggestions(vec![expected.to_string()]),
        Error::ParseError(error) => {
            let mut report = module::to_parse_error_report(source, error);
            if report.code == "nash::diagnostic" {
                report.code = "nash::syntax";
            }
            report
        }
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
    .with_code("nash::syntax")
}

pub(crate) fn wide(mut report: Report, row: Row, col: Col) -> Report {
    let highlight = report.region;
    let start = Position::new(row, col).min(highlight.start);
    report.context = Some(Region::new(start, highlight.end));
    report
}

/// Delimiter coordinates come from the parser's enclosing context, never a
/// backwards punctuation search that could select a comment or string literal.
pub(super) fn closing(
    source: &Source<'_>,
    row: Row,
    col: Col,
    open_row: Row,
    open_col: Col,
    delimiter: char,
) -> Report {
    let opening = match delimiter {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        _ => unreachable!("collection delimiter"),
    };
    // Indentation errors retain the end of the preceding token. Inspect only
    // whitespace after that boundary; comments and strings are never searched.
    let mut position = Position::new(row, col);
    for ch in source.text()[source.offset(position)..].chars() {
        if !ch.is_whitespace() {
            break;
        }
        if ch == '\n' {
            position = Position::new(position.line + 1, 1);
        } else {
            position.column += ch.len_utf8();
        }
    }
    let found = source.what_is_next(position.line, position.column);
    let (position, message, hint, label, code) = match found {
        crate::code::Next::Close(_, found) if found == delimiter => (
            position,
            format!("Closing `{delimiter}` is underindented."),
            format!("Indent `{delimiter}` to continue this construct."),
            "underindented closing delimiter".into(),
            "nash::syntax::indentation",
        ),
        crate::code::Next::Close(_, found) => (
            position,
            format!("Expected `{delimiter}`, found `{found}`."),
            format!("Replace `{found}` with `{delimiter}`."),
            format!("expected `{delimiter}`"),
            "nash::syntax::unclosed_delimiter",
        ),
        _ => (
            Position::new(row, col),
            format!("Expected `{delimiter}` to close `{opening}`."),
            format!("Add `{delimiter}` here."),
            format!("expected `{delimiter}`"),
            "nash::syntax::unclosed_delimiter",
        ),
    };
    let mut report = problem(
        "UNCLOSED DELIMITER",
        position.line,
        position.column,
        &message,
        &hint,
    )
    .with_code(code);
    report.primary_label = Some(label);
    if source
        .line(open_row)
        .and_then(|line| line.get(open_col.saturating_sub(1)..))
        .is_some_and(|rest| rest.starts_with(opening))
    {
        report.labels.push(crate::Label {
            region: to_wider_region(open_row, open_col, 1),
            text: "opened here".into(),
        });
    }
    report
}

fn unclosed_literal(
    title: &str,
    row: Row,
    col: Col,
    opening: Position,
    token: &str,
    closing: &str,
) -> Report {
    let mut report = problem(
        title,
        row,
        col,
        &format!("Expected `{closing}` to close `{token}`."),
        &format!("Add `{closing}` here."),
    )
    .with_code("nash::syntax::unclosed_delimiter");
    report.primary_label = Some(format!("expected `{closing}`"));
    report.labels.push(crate::Label {
        region: to_wider_region(opening.line, opening.column, token.len()),
        text: "opened here".into(),
    });
    report
}

pub(crate) fn to_space_report(_source: &Source<'_>, space: &Space, row: Row, col: Col) -> Report {
    match space {
        Space::TooDeep => problem(
            "EXCESSIVE NESTING",
            row,
            col,
            "Nesting limit exceeded.",
            "Split this expression, pattern, or type into smaller definitions.",
        ),
        Space::HasTab => problem(
            "NO TABS",
            row,
            col,
            "Tabs are not allowed.",
            "Replace the tab with spaces.",
        ),
        Space::EndlessMultiComment(opening) => {
            unclosed_literal("ENDLESS COMMENT", row, col, *opening, "{-", "-}")
        }
    }
}

#[cfg(test)]
mod variants;
