use super::{Doc, Report, Source, expr, problem, to_space_report, wide};
use crate::code::{Next, to_keyword_region, to_wider_region};
use nash_parse::error::{PList, PRecord, PTuple, Pattern};
use nash_parse::{Col, Row};
#[derive(Clone, Copy)]
pub(crate) enum PContext {
    Case,
    Arg,
    Let,
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
            Next::Keyword(k) => reserved(
                r,
                c,
                k,
                &format!(
                    "It looks like you are trying to use `{k}` {}:",
                    match context {
                        PContext::Arg => "as an argument",
                        PContext::Case | PContext::Let => "in this pattern",
                    }
                ),
            ),
            Next::Operator("-") => problem(
                "UNEXPECTED SYMBOL",
                r,
                c,
                "I ran into a minus sign unexpectedly in this pattern:",
                "It is not possible to pattern match on negative numbers at this time. Try using an `if` expression for that sort of thing for now.",
            ),
            _ => problem(
                "PROBLEM IN PATTERN",
                r,
                c,
                "I wanted to parse a pattern next, but I got stuck here:",
                "I am not sure why I am getting stuck exactly. I just know that I want a pattern next. Something as simple as maybeHeight or result would work!",
            ),
        },
        Pattern::Alias(r, c) | Pattern::IndentAlias(r, c) => problem(
            "UNFINISHED PATTERN",
            r,
            c,
            "I was expecting to see a variable name after the `as` keyword:",
            "The `as` keyword lets you write patterns like ((x,y) as point) so you can refer to individual parts of the tuple with x and y or you refer to the whole thing with point. So I was expecting to see a variable name after the `as` keyword here. Sometimes people just want to use `as` as a variable name though. Try using a different name in that case!",
        ),
        Pattern::WildcardNotVar(name, width, r, c) => {
            let stripped = name.trim_start_matches('_');
            let example = stripped
                .chars()
                .next()
                .map(|ch| ch.to_lowercase().to_string() + &stripped[ch.len_utf8()..])
                .unwrap_or_else(|| "x or age".into());
            Report::snippet(
                "UNEXPECTED NAME",
                to_wider_region(r, c, usize::try_from(width).unwrap_or(1)),
                None,
                Doc::reflow("Variable names cannot start with underscores like this:"),
                Doc::reflow(&format!(
                    "You can either have an underscore like _ to ignore the value, or you can have a name like {example} to use the matched value."
                )),
            )
        }
        Pattern::IndentStart(r, c) => indent_note(
            problem(
                "UNFINISHED PATTERN",
                r,
                c,
                "I wanted to parse a pattern next, but I got stuck here:",
                "I am not sure why I am getting stuck exactly. I just know that I want a pattern next. Something as simple as maybeHeight or result would work!",
            ),
            "I can get confused by indentation. If you think there is a pattern next, maybe it needs to be indented a bit more?",
        ),
    };
    wide(report, sr, sc)
}
fn reserved(r: Row, c: Col, k: &str, before: &str) -> Report {
    Report::snippet(
        "RESERVED WORD",
        to_keyword_region(r, c, k),
        None,
        Doc::reflow(before),
        Doc::reflow("This is a reserved word! Try using some other name?"),
    )
}
fn indent_note(mut report: Report, note: &str) -> Report {
    report.after = Doc::stack([report.after, Doc::to_simple_note(note)]);
    report
}
pub(super) fn to_p_record_report(source: &Source<'_>, error: &PRecord, sr: Row, sc: Col) -> Report {
    let report = match *error {
        PRecord::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        PRecord::Open(r, c) | PRecord::IndentOpen(r, c) | PRecord::IndentField(r, c) => {
            to_unfinish_record_pattern_report(r, c, "I was expecting to see a field name next.")
        }
        PRecord::End(r, c) | PRecord::IndentEnd(r, c) => to_unfinish_record_pattern_report(
            r,
            c,
            "I was expecting to see a closing curly brace next. Try adding a } here?",
        ),
        PRecord::Field(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow(&format!(
                    "I was not expecting to see `{k}` as a record field name:"
                )),
                Doc::reflow(
                    "This is a reserved word, not available for variable names. Try another name!",
                ),
            ),
            _ => {
                to_unfinish_record_pattern_report(r, c, "I was expecting to see a field name next.")
            }
        },
    };
    wide(report, sr, sc)
}
fn to_unfinish_record_pattern_report(r: Row, c: Col, message: &str) -> Report {
    let mut report = problem(
        "UNFINISHED RECORD PATTERN",
        r,
        c,
        "I was partway through parsing a record pattern, but I got stuck here:",
        message,
    );
    report.after = Doc::stack([
        report.after,
        Doc::to_simple_hint(
            "A record pattern looks like {x,y} or {name,age} where you list the field names you want to access.",
        ),
    ]);
    report
}
pub(super) fn to_p_tuple_report(
    source: &Source<'_>,
    context: PContext,
    error: &PTuple<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        PTuple::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        PTuple::Expr(e, r, c) => return to_pattern_report(source, context, e, r, c),
        PTuple::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => reserved(
                r,
                c,
                k,
                &format!("It looks like you are trying to use `{k}` as a variable name:"),
            ),
            _ => problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I just saw an open parenthesis, but I got stuck here:",
                "I was expecting to see a pattern next. Maybe it will end up being something like (x,y) or (name, _)?",
            ),
        },
        PTuple::End(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow("I ran into a reserved word in this pattern:"),
                Doc::reflow(&format!(
                    "The `{k}` keyword is reserved. Try using a different name instead!"
                )),
            ),
            Next::Operator(op) => Report::snippet(
                "UNEXPECTED SYMBOL",
                to_keyword_region(r, c, op),
                None,
                Doc::reflow(&format!(
                    "I ran into the {op} symbol unexpectedly in this pattern:"
                )),
                Doc::reflow(
                    "Only the :: symbol works in patterns. It is useful if you are pattern matching on lists, trying to get the first element off the front. Did you want that instead?",
                ),
            ),
            Next::Close(term, ch) => problem(
                &format!("STRAY {}", term.to_uppercase()),
                r,
                c,
                &format!("I ran into an unexpected {term} in this pattern:"),
                &format!(
                    "This {ch} does not match up with an earlier open {term}. Try deleting it?"
                ),
            ),
            _ => problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I was partway through parsing a pattern, but I got stuck here:",
                "I was expecting a closing parenthesis next, so try adding a ) to see if that helps?",
            ),
        },
        PTuple::IndentEnd(r, c) => indent_note(
            problem(
                "UNFINISHED PARENTHESES",
                r,
                c,
                "I was expecting a closing parenthesis next:",
                "Try adding a ) to see if that helps?",
            ),
            "I can get confused by indentation in cases like this, so maybe you have a closing parenthesis but it is not indented enough?",
        ),
        PTuple::IndentExpr1(r, c) => problem(
            "UNFINISHED PARENTHESES",
            r,
            c,
            "I just saw an open parenthesis, but then I got stuck here:",
            "I was expecting to see a pattern next. Maybe it will end up being something like (x,y) or (name, _)?",
        ),
        PTuple::IndentExprN(r, c) => indent_note(
            problem(
                "UNFINISHED TUPLE PATTERN",
                r,
                c,
                "I am partway through parsing a tuple pattern, but I got stuck here:",
                "I was expecting to see a pattern next. I am expecting the final result to be something like (x,y) or (name, _).",
            ),
            "I can get confused by indentation in cases like this, so the problem may be that the next part is not indented enough?",
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
        PList::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        PList::Expr(e, r, c) => return to_pattern_report(source, context, e, r, c),
        PList::Open(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => reserved(
                r,
                c,
                k,
                &format!("It looks like you are trying to use `{k}` to name an element of a list:"),
            ),
            _ => problem(
                "UNFINISHED LIST PATTERN",
                r,
                c,
                "I just saw an open square bracket, but then I got stuck here:",
                "Try adding a ] to see if that helps?",
            ),
        },
        PList::End(r, c) => problem(
            "UNFINISHED LIST PATTERN",
            r,
            c,
            "I was expecting a closing square bracket to end this list pattern:",
            "Try adding a ] to see if that helps?",
        ),
        PList::IndentOpen(r, c) => indent_note(
            problem(
                "UNFINISHED LIST PATTERN",
                r,
                c,
                "I just saw an open square bracket, but then I got stuck here:",
                "Try adding a ] to see if that helps?",
            ),
            "I can get confused by indentation in cases like this, so maybe there is something next, but it is not indented enough?",
        ),
        PList::IndentEnd(r, c) => indent_note(
            problem(
                "UNFINISHED LIST PATTERN",
                r,
                c,
                "I was expecting a closing square bracket to end this list pattern:",
                "Try adding a ] to see if that helps?",
            ),
            "I can get confused by indentation in cases like this, so maybe you have a closing square bracket but it is not indented enough?",
        ),
        PList::IndentExpr(r, c) => indent_note(
            problem(
                "UNFINISHED LIST PATTERN",
                r,
                c,
                "I am partway through parsing a list pattern, but I got stuck here:",
                "I was expecting to see another pattern next. Maybe a variable name.",
            ),
            "I can get confused by indentation in cases like this, so maybe there is more to this pattern but it is not indented enough?",
        ),
    };
    wide(report, sr, sc)
}
