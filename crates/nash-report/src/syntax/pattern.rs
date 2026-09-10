use super::{Doc, Report, Source, closing, expr, problem, to_space_report, wide};
use crate::code::{Next, to_keyword_region, to_wider_region};
use nash_parse::error::{PList, PRecord, PTuple, Pattern};
use nash_parse::{Col, Row};

#[derive(Clone, Copy)]
pub(crate) enum PContext {
    Case,
    Arg,
    Let,
}

fn reserved(row: Row, col: Col, keyword: &str) -> Report {
    Report::snippet(
        "RESERVED WORD",
        to_keyword_region(row, col, keyword),
        None,
        Doc::text(format!(
            "Reserved word `{keyword}` cannot be a pattern variable."
        )),
        Doc::text("Choose another name."),
    )
}

pub(crate) fn to_pattern_report(
    source: &Source<'_>,
    context: PContext,
    error: &Pattern<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Pattern::Record(e, r, c) => return to_p_record_report(source, e, r, c),
        Pattern::Tuple(e, r, c) => return to_p_tuple_report(source, context, e, r, c),
        Pattern::List(e, r, c) => return to_p_list_report(source, context, e, r, c),
        Pattern::String(ref e, r, c) => return expr::to_string_report(source, e, r, c),
        Pattern::Bytes(ref e, r, c) => return expr::to_bytes_report(source, e, r, c),
        Pattern::Number(ref e, r, c) => return expr::to_number_report(source, e, r, c),
        Pattern::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Pattern::Start(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword),
            Next::Operator("-") => problem(
                "UNEXPECTED SYMBOL",
                r,
                c,
                "Negative literal patterns are not supported.",
                "Use an if expression to test the value.",
            ),
            _ => problem(
                "EXPECTED PATTERN",
                r,
                c,
                match context {
                    PContext::Case => "Expected a case pattern.",
                    PContext::Arg => "Expected an argument pattern.",
                    PContext::Let => "Expected a binding pattern.",
                },
                "Use a variable, constructor pattern, or `_`.",
            ),
        },
        Pattern::Alias(r, c) | Pattern::IndentAlias(r, c) => problem(
            "UNFINISHED PATTERN",
            r,
            c,
            "Expected a variable after `as`.",
            "Name the whole matched value, as in `(pattern as name)`.",
        ),
        Pattern::WildcardNotVar(name, width, r, c) => Report::snippet(
            "UNEXPECTED NAME",
            to_wider_region(r, c, usize::try_from(width).unwrap_or(1)),
            None,
            Doc::text(format!("Variable `{name}` starts with an underscore.")),
            Doc::text("Use `_` to ignore the value, or a name without a leading underscore."),
        ),
        Pattern::IndentStart(r, c) => problem(
            "EXPECTED PATTERN",
            r,
            c,
            "Expected an indented pattern.",
            "Indent the pattern to continue this expression.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_p_record_report(source: &Source<'_>, error: &PRecord, sr: Row, sc: Col) -> Report {
    let report = match *error {
        PRecord::Open(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != '}') =>
        {
            return closing(source, r, c, sr, sc, '}');
        }
        PRecord::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        PRecord::End(r, c) | PRecord::IndentEnd(r, c) => return closing(source, r, c, sr, sc, '}'),
        PRecord::Open(r, c) | PRecord::Field(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword),
            _ => problem(
                "EXPECTED FIELD",
                r,
                c,
                "Expected a field name in this record pattern.",
                "List the required field names, separated by commas.",
            ),
        },
        PRecord::IndentOpen(r, c) | PRecord::IndentField(r, c) => problem(
            "EXPECTED FIELD",
            r,
            c,
            "Expected an indented record field.",
            "Indent the field inside the braces.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_p_tuple_report(
    source: &Source<'_>,
    context: PContext,
    error: &PTuple<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        PTuple::Open(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != ')') =>
        {
            return closing(source, r, c, sr, sc, ')');
        }
        PTuple::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        PTuple::Expr(e, r, c) => return to_pattern_report(source, context, e, r, c),
        PTuple::End(r, c) | PTuple::IndentEnd(r, c) => return closing(source, r, c, sr, sc, ')'),
        PTuple::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword),
            _ => problem(
                "EXPECTED PATTERN",
                r,
                c,
                "Expected a pattern after `(`.",
                "Add a pattern, or close an empty tuple with `)`.",
            ),
        },
        PTuple::IndentExpr1(r, c) | PTuple::IndentExprN(r, c) => problem(
            "EXPECTED PATTERN",
            r,
            c,
            "Expected an indented tuple pattern.",
            "Add a pattern inside the parentheses; separate elements with commas.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_p_list_report(
    source: &Source<'_>,
    context: PContext,
    error: &PList<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        PList::Open(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != ']') =>
        {
            return closing(source, r, c, sr, sc, ']');
        }
        PList::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        PList::Expr(e, r, c) => return to_pattern_report(source, context, e, r, c),
        PList::End(r, c) | PList::IndentEnd(r, c) => return closing(source, r, c, sr, sc, ']'),
        PList::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => reserved(r, c, keyword),
            _ => problem(
                "EXPECTED PATTERN",
                r,
                c,
                "Expected a pattern or `]`.",
                "Add a list element, or close an empty list with `]`.",
            ),
        },
        PList::IndentOpen(r, c) | PList::IndentExpr(r, c) => problem(
            "EXPECTED PATTERN",
            r,
            c,
            "Expected an indented list pattern.",
            "Add a pattern inside the brackets; separate elements with commas.",
        ),
    };
    wide(report, sr, sc)
}
