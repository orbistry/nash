//! Expression syntax reports, adapted from Elm's Reporting/Error/Syntax.hs.

use super::{pattern, problem, to_space_report, type_, wide};
use crate::code::Next;
use crate::{Doc, Report, Source};
use nash_parse::{Col, Row, error::*};

#[derive(Clone, Copy)]
#[allow(clippy::enum_variant_names)] // Preserve Elm's context vocabulary.
pub(crate) enum Context<'c> {
    InNode(Node, Row, Col, &'c Context<'c>),
    InDef(&'c str, Row, Col),
    InDestruct(Row, Col),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Node {
    Record,
    Parens,
    List,
    Func,
    Cond,
    Then,
    Else,
    Case,
    Branch,
    Do,
    Macro,
    Keyword(&'static str),
}
fn get_def_name(context: Context<'_>) -> Option<&str> {
    match context {
        Context::InDef(name, _, _) => Some(name),
        Context::InDestruct(_, _) => None,
        Context::InNode(_, _, _, outer) => get_def_name(*outer),
    }
}
fn is_within(node: Node, context: Context<'_>) -> bool {
    matches!(context, Context::InNode(actual, _, _, _) if node == actual)
}
fn context_start(context: Context<'_>) -> (Row, Col, String) {
    match context {
        Context::InDef(name, r, c) => (r, c, format!("the `{name}` definition")),
        Context::InDestruct(r, c) => (r, c, "a definition".into()),
        Context::InNode(node, r, c, _) => (
            r,
            c,
            match node {
                Node::Record => "a record",
                Node::Parens => "some parentheses",
                Node::List => "a list",
                Node::Func => "an anonymous function",
                Node::Cond | Node::Then | Node::Else => "an `if` expression",
                Node::Case | Node::Branch => "a `case` expression",
                Node::Do => "a `do` block",
                Node::Macro => "a macro invocation",
                Node::Keyword(keyword) => {
                    return (
                        r,
                        c,
                        format!(
                            "{} `{keyword}` expression",
                            if keyword == "assert" { "an" } else { "a" }
                        ),
                    );
                }
            }
            .into(),
        ),
    }
}
fn unfinished(title: &str, thing: &str, r: Row, c: Col, sr: Row, sc: Col, hint: &str) -> Report {
    wide(
        problem(
            title,
            r,
            c,
            &format!("I was partway through parsing {thing}, but I got stuck here:"),
            hint,
        ),
        sr,
        sc,
    )
}
fn width(mut report: Report, amount: u16) -> Report {
    report.region.end.column = report.region.start.column.saturating_add(amount);
    report.snippet = crate::Snippet::Region {
        region: report.region,
        highlight: None,
    };
    report
}
fn example(mut report: Report, lines: &[&str], note: &str) -> Report {
    report.after = Doc::stack([
        report.after,
        Doc::indent(4, Doc::vcat(lines.iter().map(|s| Doc::text(*s)))).dullyellow(),
        Doc::reflow(note),
    ]);
    report
}
pub(crate) fn to_expr_report(
    source: &Source<'_>,
    context: Context<'_>,
    expr: &Expr<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    match *expr {
        Expr::Let(e, r, c) => to_let_report(source, context, e, r, c),
        Expr::Case(e, r, c) => to_case_report(source, context, e, r, c),
        Expr::If(e, r, c) => to_if_report(source, context, e, r, c),
        Expr::List(e, r, c) => to_list_report(source, context, e, r, c),
        Expr::Record(e, r, c) => to_record_report(source, context, e, r, c),
        Expr::Tuple(e, r, c) => to_tuple_report(source, context, e, r, c),
        Expr::Func(e, r, c) => to_func_report(source, context, e, r, c),
        Expr::Do(e, r, c) => to_do_report(source, context, e, r, c),
        Expr::Macro(e, r, c) => to_macro_report(source, context, e, r, c),
        Expr::Assert(e, r, c) => to_keyword_report(source, context, "assert", e, r, c),
        Expr::Fail(e, r, c) => to_keyword_report(source, context, "fail", e, r, c),
        Expr::Todo(e, r, c) => to_keyword_report(source, context, "todo", e, r, c),
        Expr::Trace(e, r, c) => to_keyword_report(source, context, "trace", e, r, c),
        Expr::Comptime(e, r, c) => to_keyword_report(source, context, "comptime", e, r, c),
        Expr::Dot(r, c) => problem(
            "EXPECTING RECORD ACCESSOR",
            r,
            c,
            "I was expecting to see a record accessor here:",
            "Something like .name or .price that accesses a value from a record.",
        ),
        Expr::Access(r, c) => problem(
            "EXPECTING RECORD ACCESSOR",
            r,
            c,
            "I am trying to parse a record accessor here:",
            "Something like .name or .price that accesses a value from a record. Record field names must start with a lower case letter!",
        ),
        Expr::OperatorRight(op, r, c) => {
            let hint = match op { "+"|"-"|"*"|"/"|"^"=>format!("I was expecting to see an expression next. Something like 42 or 1000 that makes sense with a {op} sign."),"&&"|"||"=>"I was expecting to see an expression next. Something like True or False that makes sense with boolean logic.".into(),"|>"=>"I was expecting to see a function next.".into(),"<|"=>"I was expecting to see an argument next.".into(),_=>"I was expecting to see an expression next.".into() };
            wide(
                problem(
                    "MISSING EXPRESSION",
                    r,
                    c,
                    &format!(
                        "I just saw a {op} {}, so I am getting stuck here:",
                        if matches!(op, "+" | "-" | "*" | "/" | "^") {
                            "sign"
                        } else {
                            "operator"
                        }
                    ),
                    &hint,
                ),
                sr,
                sc,
            )
        }
        Expr::IndentOperatorRight(op, r, c) => wide(
            problem(
                "MISSING EXPRESSION",
                r,
                c,
                &format!("I was expecting to see an expression after this {op} operator:"),
                &format!(
                    "You can just put anything for now, like 42 or \"hello\". Once there is something there, I can probably give a more specific hint! I may be getting confused by your indentation? The easiest way to make sure this is not an indentation problem is to put the expression on the right of the {op} operator on the same line."
                ),
            ),
            sr,
            sc,
        ),
        Expr::OperatorReserved(ref op, r, c) => operator_in_context(source, context, op, r, c),
        Expr::Start(r, c) => {
            let (r0, c0, thing) = context_start(context);
            unfinished(
                "MISSING EXPRESSION",
                &thing,
                r,
                c,
                r0,
                c0,
                "I was expecting to see an expression like 42 or \"hello\". Once there is something there, I can probably give a more specific hint! This can also happen if I run into reserved words like `let` or `as` unexpectedly, or operators in unexpected spots.",
            )
        }
        Expr::String(ref e, r, c) => to_string_report(source, e, r, c),
        Expr::Bytes(ref e, r, c) => to_bytes_report(source, e, r, c),
        Expr::Number(ref e, r, c) => to_number_report(source, e, r, c),
        Expr::Space(ref e, r, c) => to_space_report(source, e, r, c),
    }
}
pub(crate) fn to_string_report(source: &Source<'_>, e: &StringError, r: Row, c: Col) -> Report {
    let report = match e {
        StringError::EndlessSingle => problem(
            "ENDLESS STRING",
            r,
            c,
            "I got to the end of the line without seeing the closing double quote:",
            "Strings look like \"this\" with double quotes on each end. Is the closing double quote missing in your code? For a string that spans multiple lines, use triple double quotes on each end.",
        ),
        StringError::EndlessMulti => width(
            problem(
                "ENDLESS STRING",
                r,
                c,
                "I cannot find the end of this multi-line string:",
                "Add a \"\"\" somewhere after this to end the string.",
            ),
            3,
        ),
        StringError::Escape(e) => return to_escape_report(source, e, r, c),
    };
    example(
        report,
        &[
            "\"\"\"",
            "# Multi-line Strings",
            "",
            "- start with triple double quotes",
            "- write whatever you want",
            "- no need to escape newlines or double quotes",
            "- end with triple double quotes",
            "\"\"\"",
        ],
        "Here is a valid multi-line string for reference.",
    )
}
fn to_escape_report(_source: &Source<'_>, e: &Escape, r: Row, c: Col) -> Report {
    match *e {
        Escape::Unknown => width(
            problem(
                "UNKNOWN ESCAPE",
                r,
                c,
                "Backslashes always start escaped characters, but I do not recognize this one:",
                r#"Valid escape characters include \n, \r, \t, \", \', \\, and \u{003D}. Do you want one of those instead? Maybe you need \\ to escape a backslash?"#,
            ),
            2,
        ),
        Escape::BadUnicodeFormat(w) => width(
            problem(
                "BAD UNICODE ESCAPE",
                r,
                c,
                "I ran into an invalid Unicode escape:",
                r"Valid Unicode escapes include \u{0041}, \u{03BB}, and \u{1F60A}. Notice that the code point is always surrounded by curly braces. Maybe you are missing the opening or closing curly brace?",
            ),
            w,
        ),
        Escape::BadUnicodeCode(w) => width(
            problem(
                "BAD UNICODE ESCAPE",
                r,
                c,
                "This is not a valid code point:",
                "The valid Unicode scalar values are between 0 and 10FFFF inclusive, excluding the surrogate range D800 through DFFF.",
            ),
            w,
        ),
        Escape::BadUnicodeLength {
            code,
            expected,
            actual,
        } => width(
            problem(
                "BAD UNICODE ESCAPE",
                r,
                c,
                "This code point has the wrong number of digits:",
                &format!(
                    "I expected {expected} digits, but found {actual}. Unicode escapes need between four and six hexadecimal digits. Add leading zeros if there are too few, or trim leading zeros if there are too many."
                ),
            ),
            code,
        ),
    }
}
pub(crate) fn to_number_report(_source: &Source<'_>, e: &Number, r: Row, c: Col) -> Report {
    match e {
        Number::End => problem(
            "WEIRD NUMBER",
            r,
            c,
            "I thought I was reading a number, but I ran into some weird stuff here:",
            "I recognize integers like 42 and 0x002B. Is there a way to write it like one of those? Nash has no floating point numbers.",
        ),
        Number::Dot(n) => problem(
            "WEIRD NUMBER",
            r,
            c,
            "Numbers cannot end with a dot like this:",
            &format!("Switching to {n} will work though! Nash has no floating point numbers."),
        ),
        Number::HexDigit => problem(
            "WEIRD HEXADECIMAL",
            r,
            c,
            "I thought I was reading a hexadecimal number until I got here:",
            "Valid hexadecimal digits include 0123456789abcdefABCDEF, so I can only recognize things like 0x2B, 0x002B, or 0x00ffb3.",
        ),
        Number::NoLeadingZero => problem(
            "LEADING ZEROS",
            r,
            c,
            "I do not accept numbers with leading zeros:",
            "Just delete the leading zeros and it should work! Some languages use a leading zero to specify octal numbers. Nash avoids this ambiguity.",
        ),
    }
}
pub(crate) fn to_bytes_report(_source: &Source<'_>, e: &Bytes, r: Row, c: Col) -> Report {
    match e {
        Bytes::Endless => problem(
            "ENDLESS BYTE STRING",
            r,
            c,
            "I cannot find the end of this byte string:",
            "Add a closing double quote to end the byte string.",
        ),
        Bytes::OddLength => problem(
            "INCOMPLETE BYTE",
            r,
            c,
            "This byte string has an odd number of hexadecimal digits:",
            "Each byte needs two hexadecimal digits. Add the missing digit or remove the extra one.",
        ),
        Bytes::BadHexDigit(bad_col) => wide(
            problem(
                "BAD BYTE STRING",
                r,
                *bad_col,
                "I ran into an invalid hexadecimal digit in this byte string:",
                "Use pairs of digits from 0123456789abcdefABCDEF, one pair for each byte.",
            ),
            r,
            c,
        ),
    }
}
pub(crate) fn to_operator_report(_source: &Source<'_>, e: &BadOperator, r: Row, c: Col) -> Report {
    let (title, before, after, w) = match e {
        BadOperator::Dot => (
            "UNEXPECTED SYMBOL",
            "I was not expecting this dot:",
            "Dots are for record access, so they cannot float around on their own. Maybe there is some extra whitespace?",
            1,
        ),
        BadOperator::Pipe => (
            "UNEXPECTED SYMBOL",
            "I was not expecting this vertical bar:",
            "Vertical bars appear in custom type declarations and record updates. Maybe you want || instead?",
            1,
        ),
        BadOperator::Arrow => (
            "UNEXPECTED ARROW",
            "I was not expecting this arrow:",
            "Arrows belong in `case` branches, anonymous functions, and function types. Maybe an earlier expression is unfinished?",
            2,
        ),
        BadOperator::Equals => (
            "UNEXPECTED EQUALS",
            "I was not expecting this equals sign:",
            "An equals sign defines a value. To compare two values, use == instead.",
            1,
        ),
        BadOperator::HasType => (
            "UNEXPECTED COLON",
            "I was not expecting this colon:",
            "Colons appear in type annotations. A type annotation must appear directly above its definition.",
            1,
        ),
        BadOperator::FatArrow => (
            "UNEXPECTED ARROW",
            "I was not expecting this fat arrow:",
            "Use -> for a `case` branch or an anonymous function. The => arrow belongs in trait constraints.",
            2,
        ),
        BadOperator::LeftArrow => (
            "UNEXPECTED ARROW",
            "I was not expecting this left arrow:",
            "The <- arrow binds the result of an action inside a `do` block.",
            2,
        ),
    };
    width(problem(title, r, c, before, after), w)
}
fn operator_in_context(
    source: &Source<'_>,
    context: Context<'_>,
    e: &BadOperator,
    r: Row,
    c: Col,
) -> Report {
    let mut report = to_operator_report(source, e, r, c);
    if matches!(e, BadOperator::Arrow)
        && (is_within(Node::Case, context) || is_within(Node::Branch, context))
    {
        report.before = Doc::reflow(
            "I am parsing a `case` expression right now, but this arrow is confusing me:",
        );
        report.after = Doc::reflow(if is_within(Node::Case, context) {
            "Maybe the `of` keyword is missing on a previous line?"
        } else {
            "Maybe this branch is not indented enough? Each pattern must line up with the other patterns."
        });
    } else if matches!(e, BadOperator::Equals) && is_within(Node::Record, context) {
        report.after = Doc::stack([
            Doc::reflow("Maybe you want == instead? To check if two values are equal?"),
            Doc::to_simple_note(
                "Records look like { x = 3, y = 4 } with the equals sign right after the field name. So maybe you forgot a comma?",
            ),
        ]);
    } else if matches!(e, BadOperator::Equals)
        && let Some(name) = get_def_name(context)
    {
        report.after = Doc::reflow(&format!(
            "Maybe you want == instead? To check if two values are equal? I may be getting confused by your indentation. I think I am still parsing the `{name}` definition. Is this supposed to be part of a definition after that? If so, the problem may be a bit before the equals sign. I need all definitions to be indented exactly the same amount, so the problem may be that this new definition has too many spaces in front of it."
        ));
    }
    report
}
fn to_if_report(source: &Source<'_>, ctx: Context<'_>, e: &If<'_>, sr: Row, sc: Col) -> Report {
    let (r, c, hint) = match *e {
        If::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        If::Condition(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Cond, sr, sc, &ctx), e, r, c);
        }
        If::ThenBranch(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Then, sr, sc, &ctx), e, r, c);
        }
        If::ElseBranch(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Else, sr, sc, &ctx), e, r, c);
        }
        If::Then(r, c) => (r, c, "I was expecting to see the `then` keyword next."),
        If::Else(r, c) => (
            r,
            c,
            "I was expecting to see the `else` keyword next. All `if` expressions need an `else` branch.",
        ),
        If::ElseBranchStart(r, c) => (
            r,
            c,
            "I was expecting to see an expression next. Maybe the `else` branch is not filled in yet?",
        ),
        If::IndentCondition(r, c) => (
            r,
            c,
            "I was expecting to see a condition next. If it is already present, it may not be indented enough for me to recognize it.",
        ),
        If::IndentThen(r, c) => (
            r,
            c,
            "I was expecting to see the `then` keyword next. It may need more indentation.",
        ),
        If::IndentThenBranch(r, c) => (
            r,
            c,
            "I was expecting to see an expression next. If the `then` branch is already present, it may not be indented enough for me to recognize it.",
        ),
        If::IndentElseBranch(r, c) => (
            r,
            c,
            "I was expecting to see an expression next. If the `else` branch is already present, it may not be indented enough for me to recognize it.",
        ),
        If::IndentElse(r, c) => {
            if let Some((row, col)) = source.next_line_starts_with_keyword("else", r) {
                return wide(
                    width(
                        problem(
                            "WEIRD ELSE BRANCH",
                            row,
                            col,
                            "I was partway through an `if` expression when I got stuck here:",
                            "I think this `else` keyword needs to be indented more. Try adding some spaces before it!",
                        ),
                        4,
                    ),
                    sr,
                    sc,
                );
            }
            (
                r,
                c,
                "I was expecting to see an `else` branch after this. All `if` expressions need both branches. Check the indentation if the branch is already present.",
            )
        }
    };
    wide(
        problem(
            "UNFINISHED IF",
            r,
            c,
            "I was expecting to see more of this `if` expression, but I got stuck here:",
            hint,
        ),
        sr,
        sc,
    )
}
fn case_note(report: Report) -> Report {
    example(
        report,
        &[
            "case maybeWidth of",
            "  Some width ->",
            "    width + 200",
            "",
            "  None ->",
            "    400",
        ],
        "Notice the indentation. Each pattern is aligned, and each branch is indented a bit more than the corresponding pattern. That is important!",
    )
}
fn to_case_report(source: &Source<'_>, ctx: Context<'_>, e: &Case<'_>, sr: Row, sc: Col) -> Report {
    let (r,c,hint)=match *e {
        Case::Space(ref e,r,c)=>return to_space_report(source,e,r,c),
        Case::Pattern(e,r,c)=>return pattern::to_pattern_report(source,pattern::PContext::Case,e,r,c),
        Case::Expr(e,r,c)=>return to_expr_report(source,Context::InNode(Node::Case,sr,sc,&ctx),e,r,c),
        Case::Branch(e,r,c)=>return to_expr_report(source,Context::InNode(Node::Branch,sr,sc,&ctx),e,r,c),
        Case::Of(r,c)|Case::IndentOf(r,c)=>(r,c,"I was expecting to see the `of` keyword next.".to_owned()),
        Case::Arrow(r,c)=> {
            let (title,hint)=match source.what_is_next(r,c) {
                Next::Keyword(k)=>("RESERVED WORD",format!("It looks like you are trying to use `{k}` in one of your patterns, but it is a reserved word. Try using a different name?")),
                Next::Operator(":")=>("UNEXPECTED OPERATOR","I am seeing : but maybe you want :: instead?".into()),
                Next::Operator("=")=>("UNEXPECTED OPERATOR","I am seeing = but maybe you want -> instead?".into()),
                _=>("MISSING ARROW","I was expecting to see an arrow next.".into()),
            };
            return case_note(unfinished(title,"a `case` expression",r,c,sr,sc,&hint));
        }
        Case::IndentExpr(r,c)=>(r,c,"I was expecting to see an expression next.".into()),
        Case::IndentPattern(r,c)=>(r,c,"I was expecting to see a pattern next.".into()),
        Case::IndentArrow(r,c)=>(r,c,"I was expecting to see an arrow next. It may need more indentation.".into()),
        Case::IndentBranch(r,c)=>(r,c,"I was expecting to see an expression next. What should I do when I run into this particular pattern?".into()),
        Case::PatternAlignment(indent,r,c)=>(r,c,format!("I suspect this is a pattern that is not indented far enough? ({indent} spaces)")),
    };
    case_note(unfinished(
        "UNFINISHED CASE",
        "a `case` expression",
        r,
        c,
        sr,
        sc,
        &hint,
    ))
}
fn record_note(report: Report) -> Report {
    example(
        report,
        &["{ name = \"Nash\"", "  , age = 1", "  }"],
        "Notice that each line starts with some indentation. Usually two or four spaces.",
    )
}
fn to_record_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    e: &Record<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let (r, c, title, hint) = match *e {
        Record::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Record::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Record, sr, sc, &ctx), e, r, c);
        }
        Record::Open(r, c) | Record::Field(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => {
                return wide(
                    width(
                        problem(
                            "RESERVED WORD",
                            r,
                            c,
                            "I am partway through parsing a record, but I got stuck on this field name:",
                            &format!(
                                "It looks like you are trying to use `{k}` as a field name, but that is a reserved word. Try using a different name!"
                            ),
                        ),
                        k.len() as u16,
                    ),
                    sr,
                    sc,
                );
            }
            Next::Other(Some(',')) => (
                r,
                c,
                "EXTRA COMMA",
                "I am seeing two commas in a row. This is the second one! Just delete one of the commas and you should be all set!",
            ),
            Next::Close(_, '}') => (
                r,
                c,
                "EXTRA COMMA",
                "Trailing commas are not allowed in records. Try deleting the comma that appears before this closing curly brace.",
            ),
            _ => (
                r,
                c,
                "PROBLEM IN RECORD",
                "I was expecting to see a record field next. Record field names must start with a lower case letter.",
            ),
        },
        Record::End(r, c) => (
            r,
            c,
            "PROBLEM IN RECORD",
            "I was expecting to see a comma or a closing curly brace next.",
        ),
        Record::Equals(r, c) => (
            r,
            c,
            "PROBLEM IN RECORD",
            "I just saw a record field, so I was expecting to see an equals sign next.",
        ),
        Record::IndentOpen(r, c) => (
            r,
            c,
            "UNFINISHED RECORD",
            "I just saw the opening curly brace of a record. I was expecting a field name or a closing curly brace next. Try adding more indentation.",
        ),
        Record::IndentEnd(r, c) => {
            if let Some((row, col)) = source.next_line_starts_with_close_curly(r) {
                return record_note(unfinished(
                    "NEED MORE INDENTATION",
                    "a record",
                    row,
                    col,
                    sr,
                    sc,
                    "I need this curly brace to be indented more. Try adding some spaces before it!",
                ));
            }
            if matches!(source.what_is_next(r, c), Next::Close(_, '}')) {
                (
                    r,
                    c,
                    "NEED MORE INDENTATION",
                    "I need this curly brace to be indented more. Try adding some spaces before it!",
                )
            } else {
                (
                    r,
                    c,
                    "UNFINISHED RECORD",
                    "I was expecting a comma or a closing curly brace next. Try adding more indentation.",
                )
            }
        }
        Record::IndentField(r, c) => (
            r,
            c,
            "UNFINISHED RECORD",
            "Trailing commas are not allowed in records, so the fix may be to delete that last comma? Or maybe you were in the middle of defining an additional field?",
        ),
        Record::IndentEquals(r, c) => (
            r,
            c,
            "UNFINISHED RECORD",
            "I just saw a record field, so I was expecting to see an equals sign next. Try adding more indentation.",
        ),
        Record::IndentExpr(r, c) => (
            r,
            c,
            "UNFINISHED RECORD",
            "I was expecting to run into an expression next. If it is already present, it may need more indentation.",
        ),
    };
    record_note(unfinished(title, "a record", r, c, sr, sc, hint))
}
fn to_tuple_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    e: &Tuple<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let (r, c, title, hint) = match *e {
        Tuple::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Tuple::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Parens, sr, sc, &ctx), e, r, c);
        }
        Tuple::OperatorReserved(ref e, r, c) => return to_operator_report(source, e, r, c),
        Tuple::End(r, c) => (
            r,
            c,
            "UNFINISHED PARENTHESES",
            "I was expecting to see a closing parenthesis next. Try adding a ) to see if that helps?",
        ),
        Tuple::OperatorClose(r, c) => (
            r,
            c,
            "UNFINISHED OPERATOR FUNCTION",
            "I was expecting a closing parenthesis here. Try adding a ) to see if that helps! Operators in parentheses, like (+), can be used as functions.",
        ),
        Tuple::IndentExpr1(r, c) => (
            r,
            c,
            "UNFINISHED PARENTHESES",
            "I just saw an open parenthesis, so I was expecting to see an expression next. It may need more indentation.",
        ),
        Tuple::IndentExprN(r, c) => (
            r,
            c,
            "UNFINISHED TUPLE",
            "I just saw a comma, so I was expecting to see an expression next. It may need more indentation.",
        ),
        Tuple::IndentEnd(r, c) => (
            r,
            c,
            "UNFINISHED PARENTHESES",
            "I was expecting to see a closing parenthesis next. Try adding a ) or adding more indentation to the existing one.",
        ),
    };
    unfinished(title, "some parentheses", r, c, sr, sc, hint)
}
fn to_list_report(source: &Source<'_>, ctx: Context<'_>, e: &List<'_>, sr: Row, sc: Col) -> Report {
    let (r, c, hint) = match *e {
        List::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        List::Expr(e, r, c) => {
            if let Expr::Start(row, col) = *e {
                (
                    row,
                    col,
                    "Trailing commas are not allowed in lists, so the fix may be to delete the comma?",
                )
            } else {
                return to_expr_report(source, Context::InNode(Node::List, sr, sc, &ctx), e, r, c);
            }
        }
        List::Open(r, c) => (
            r,
            c,
            "I was expecting an expression or a closing square bracket next.",
        ),
        List::End(r, c) => (
            r,
            c,
            "I was expecting a comma or a closing square bracket next.",
        ),
        List::IndentOpen(r, c) => (
            r,
            c,
            "I cannot find the end of this list. Try adding a ] or indenting the list entries more.",
        ),
        List::IndentEnd(r, c) => (
            r,
            c,
            "I cannot find the end of this list. Try adding a ] or indenting the closing bracket more.",
        ),
        List::IndentExpr(r, c) => (
            r,
            c,
            "I was expecting to see another list entry after this comma. Trailing commas are not allowed in lists, so the fix may be to delete the comma?",
        ),
    };
    example(
        unfinished("UNFINISHED LIST", "a list", r, c, sr, sc, hint),
        &["[ 1", "  , 2", "  ]"],
        "Notice that each line starts with some indentation. Usually two or four spaces.",
    )
}
fn to_func_report(source: &Source<'_>, ctx: Context<'_>, e: &Func<'_>, sr: Row, sc: Col) -> Report {
    let (r, c, title, hint) = match *e {
        Func::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Func::Arg(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Arg, e, r, c);
        }
        Func::Body(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Func, sr, sc, &ctx), e, r, c);
        }
        Func::Arrow(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => {
                return wide(
                    width(
                        problem(
                            "RESERVED WORD",
                            r,
                            c,
                            "I was parsing an anonymous function, but I got stuck here:",
                            &format!(
                                "It looks like you are trying to use `{k}` as an argument, but it is a reserved word in this language. Try using a different argument name!"
                            ),
                        ),
                        k.len() as u16,
                    ),
                    sr,
                    sc,
                );
            }
            _ => (
                r,
                c,
                "UNFINISHED ANONYMOUS FUNCTION",
                "I was expecting to see an arrow next. The syntax for anonymous functions is \\name -> name.",
            ),
        },
        Func::IndentArg(r, c) => (
            r,
            c,
            "MISSING ARGUMENT",
            "I just saw the beginning of an anonymous function, so I was expecting to see an argument next. It may need more indentation.",
        ),
        Func::IndentArrow(r, c) => (
            r,
            c,
            "UNFINISHED ANONYMOUS FUNCTION",
            "I was expecting to see an arrow next. It may need more indentation.",
        ),
        Func::IndentBody(r, c) => (
            r,
            c,
            "UNFINISHED ANONYMOUS FUNCTION",
            "I was expecting to see an expression after the arrow. It may need more indentation.",
        ),
    };
    unfinished(title, "an anonymous function", r, c, sr, sc, hint)
}
fn to_let_report(source: &Source<'_>, ctx: Context<'_>, e: &Let<'_>, sr: Row, sc: Col) -> Report {
    let (r,c,hint)=match *e {
        Let::Space(ref e,r,c)=>return to_space_report(source,e,r,c),
        Let::Def(name,e,r,c)=>return to_let_def_report(source,name,e,r,c),
        Let::Destruct(e,r,c)=>return to_let_destruct_report(source,e,r,c),
        Let::Body(e,r,c)=>return to_expr_report(source,ctx,e,r,c),
        Let::In(r,c)|Let::DefAlignment(_,r,c)=>return unfinished("LET PROBLEM", "a `let` expression", r,c,sr,sc,"Based on the indentation, I was expecting to see the `in` keyword next. Is there a typo? This can also happen if you are trying to define another value within the `let` but it is not indented enough. Make sure each definition has exactly the same amount of spaces before it. They should line up exactly!"),
        Let::IndentIn(r,c)=>(r,c,"I was expecting to see the `in` keyword next. Or maybe more of that expression?".into()),
        Let::DefName(r,c)=>match source.what_is_next(r,c) {
            Next::Keyword(k)=>return wide(width(problem("RESERVED WORD",r,c,"I was partway through parsing a `let` expression, but I got stuck here:",&format!("It looks like you are trying to use `{k}` as a variable name, but it is a reserved word! Try using a different name instead.")),k.len() as u16),sr,sc),
            _=>(r,c,"I was expecting the name of a definition next.".to_owned()),
        },
        Let::IndentDef(r,c)=>(r,c,"I was expecting a value to be defined here. It may need more indentation.".into()),
        Let::IndentBody(r,c)=>(r,c,"I was expecting an expression next. Tell me what should happen with the value you just defined!".into()),
    };
    example(
        unfinished("UNFINISHED LET", "a `let` expression", r, c, sr, sc, &hint),
        &[
            "let",
            "    fullName =",
            "        first ++ \" \" ++ last",
            "in",
            "fullName",
        ],
        "The definition is indented more than the `let` keyword, and its value is indented a bit more than that. That is important!",
    )
}
pub(crate) fn to_let_def_report(
    source: &Source<'_>,
    name: &str,
    e: &Def<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let (r,c,title,hint)=match *e {
        Def::Space(ref e,r,c)=>return to_space_report(source,e,r,c),
        Def::Type(e,r,c)=>return type_::to_type_report(source,type_::TContext::Annotation(name),e,r,c),
        Def::Arg(e,r,c)=>return pattern::to_pattern_report(source,pattern::PContext::Arg,e,r,c),
        Def::Body(e,r,c)=>return to_expr_report(source,Context::InDef(name,sr,sc),e,r,c),
        Def::NameRepeat(r,c)=>(r,c,"EXPECTING DEFINITION",format!("I just saw the type annotation for `{name}` so I was expecting to see its definition here. Type annotations always appear directly above the relevant definition, without anything else in between.")),
        Def::NameMatch(actual,r,c)=>return wide(width(problem("NAME MISMATCH",r,c,&format!("I just saw a type annotation for `{name}`, but it is followed by a definition for `{actual}`:"),"These names do not match! Is there a typo?"),actual.len() as u16),sr,sc).with_suggestions(vec![name.to_owned()]),
        Def::Equals(r,c)=>match source.what_is_next(r,c) {
            Next::Keyword(k)=>return wide(width(problem("RESERVED WORD",r,c,&format!("The name `{k}` is reserved, so it cannot be used as an argument:"),"Try renaming it to something else."),k.len() as u16),sr,sc),
            Next::Operator("->")=>(r,c,"MISSING COLON?","I was not expecting to see an arrow here. Maybe this is a type annotation missing its colon?".into()),
            _=>(r,c,"PROBLEM IN DEFINITION","I was expecting to see an argument or an equals sign next.".into()),
        },
        Def::IndentEquals(r,c)=>(r,c,"UNFINISHED DEFINITION","I was expecting to see an argument or an equals sign next. It may need more indentation.".into()),
        Def::IndentType(r,c)=>(r,c,"UNFINISHED DEFINITION","I just saw a colon, so I am expecting to see a type next. It may need more indentation.".into()),
        Def::IndentBody(r,c)=>(r,c,"UNFINISHED DEFINITION","I was expecting to see an expression next. What is it equal to?".into()),
        Def::Alignment(indent,r,c)=>(r,c,"PROBLEM IN DEFINITION",format!("I just saw a type annotation indented {indent} spaces, so I was expecting to see the corresponding definition next with the exact same amount of indentation.")),
    };
    example(
        wide(
            problem(
                title,
                r,
                c,
                &format!("I got stuck while parsing the `{name}` definition:"),
                &hint,
            ),
            sr,
            sc,
        ),
        &[
            "greet : string -> string",
            "greet name =",
            "    \"Hello \" ++ name",
        ],
        "The top line is an optional type annotation. It works as compiler-verified documentation and often improves error messages!",
    )
}
fn to_let_destruct_report(source: &Source<'_>, e: &Destruct<'_>, sr: Row, sc: Col) -> Report {
    let (r, c, hint) = match *e {
        Destruct::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Destruct::Pattern(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Let, e, r, c);
        }
        Destruct::Body(e, r, c) => {
            return to_expr_report(source, Context::InDestruct(sr, sc), e, r, c);
        }
        Destruct::Equals(r, c) => (
            r,
            c,
            if matches!(source.what_is_next(r, c), Next::Operator(":")) {
                "I was expecting to see an equals sign next, followed by an expression telling me what to compute. Destructuring definitions cannot have type annotations. Put the annotation on a named value instead."
            } else {
                "I was expecting to see an equals sign next, followed by an expression telling me what to compute."
            },
        ),
        Destruct::IndentEquals(r, c) => (
            r,
            c,
            "I was expecting to see an equals sign next, followed by an expression telling me what to compute. It may need more indentation.",
        ),
        Destruct::IndentBody(r, c) => (
            r,
            c,
            "I was expecting to see an expression next. What is it equal to?",
        ),
    };
    unfinished(
        "UNFINISHED DEFINITION",
        "this definition",
        r,
        c,
        sr,
        sc,
        hint,
    )
}
fn to_keyword_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    keyword: &'static str,
    e: &Keyword<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let (r, c, hint) = match *e {
        Keyword::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Keyword::Body(e, r, c) | Keyword::Message(e, r, c) => {
            return to_expr_report(
                source,
                Context::InNode(Node::Keyword(keyword), sr, sc, &ctx),
                e,
                r,
                c,
            );
        }
        Keyword::IndentBody(r, c) => (
            r,
            c,
            "I was expecting to see an expression next. It may need more indentation.",
        ),
        Keyword::IndentMessage(r, c) => (
            r,
            c,
            "I was expecting to see a message expression next. It may need more indentation.",
        ),
    };
    unfinished(
        "UNFINISHED EXPRESSION",
        &format!(
            "{} `{keyword}` expression",
            if keyword == "assert" { "an" } else { "a" }
        ),
        r,
        c,
        sr,
        sc,
        hint,
    )
}
pub(crate) fn to_do_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    e: &Do<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let (r,c,hint)=match *e {
        Do::Space(ref e,r,c)=>return to_space_report(source,e,r,c),
        Do::Let(e,r,c)=>return to_let_report(source,ctx,e,r,c),
        Do::Pattern(e,r,c)=>return pattern::to_pattern_report(source,pattern::PContext::Let,e,r,c),
        Do::Expr(e,r,c)=>return to_expr_report(source,Context::InNode(Node::Do,sr,sc,&ctx),e,r,c),
        Do::Arrow(r,c)=>(r,c,"I was expecting to see <- after this binding pattern.".into()),
        Do::LastNotExpr(r,c)=>(r,c,"A `do` block must end with an expression. Add the final expression after this binding.".into()),
        Do::IndentStmt(r,c)=>(r,c,"I was expecting an indented statement after `do`.".into()),
        Do::IndentArrow(r,c)=>(r,c,"I was expecting to see <- after this binding pattern. It may need more indentation.".into()),
        Do::IndentExpr(r,c)=>(r,c,"I was expecting an expression after <-. It may need more indentation.".into()),
        Do::Alignment(indent,r,c)=>(r,c,format!("Statements in this `do` block must line up with {indent} spaces of indentation.")),
    };
    unfinished("UNFINISHED DO", "a `do` block", r, c, sr, sc, &hint)
}
fn to_macro_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    e: &Macro<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let (r, c, hint) = match *e {
        Macro::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Macro::Arg(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Macro, sr, sc, &ctx), e, r, c);
        }
        Macro::Open(r, c) => (
            r,
            c,
            "I was expecting an opening parenthesis after the macro's ! marker.",
        ),
        Macro::End(r, c) => (
            r,
            c,
            "I was expecting a comma or a closing parenthesis after this macro argument.",
        ),
        Macro::IndentArg(r, c) => (
            r,
            c,
            "I was expecting a macro argument next. It may need more indentation.",
        ),
        Macro::IndentEnd(r, c) => (
            r,
            c,
            "I was expecting a closing parenthesis. It may need more indentation.",
        ),
    };
    unfinished("UNFINISHED MACRO", "a macro invocation", r, c, sr, sc, hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(error: Expr<'_>, text: &str) -> String {
        let report = to_expr_report(
            &Source::new(text),
            Context::InDef("value", 1, 1),
            &error,
            1,
            9,
        );
        format!(
            "{}\n{:?}\n\n{}\n\n{}",
            report.title,
            report.region,
            report.before.render(80, false),
            report.after.render(80, false)
        )
    }
    macro_rules! report_test {
        ($name:ident, $error:expr, $text:expr) => {
            #[test]
            fn $name() {
                insta::assert_snapshot!(snapshot($error, $text));
            }
        };
    }
    report_test!(expr_start_bad, Expr::Start(1, 9), "value = ");
    report_test!(expr_dot_without_name, Expr::Dot(1, 9), "value = ");
    report_test!(expr_access_upper, Expr::Access(1, 9), "value = ");
    report_test!(
        operator_reserved_arrow,
        Expr::OperatorReserved(BadOperator::Arrow, 1, 9),
        "value = "
    );
    report_test!(
        string_endless_single,
        Expr::String(StringError::EndlessSingle, 1, 9),
        "value = "
    );
    report_test!(
        string_endless_multi,
        Expr::String(StringError::EndlessMulti, 1, 9),
        "value = "
    );
    report_test!(
        escape_unknown,
        Expr::String(StringError::Escape(Escape::Unknown), 1, 9),
        "value = "
    );
    report_test!(
        escape_bad_unicode,
        Expr::String(StringError::Escape(Escape::BadUnicodeFormat(3)), 1, 9),
        "value = "
    );
    report_test!(
        escape_short_unicode,
        Expr::String(
            StringError::Escape(Escape::BadUnicodeLength {
                code: 5,
                expected: 4,
                actual: 1
            }),
            1,
            9
        ),
        "value = "
    );
    report_test!(
        number_hex_digit,
        Expr::Number(Number::HexDigit, 1, 9),
        "value = "
    );
    report_test!(
        number_no_leading_zero,
        Expr::Number(Number::NoLeadingZero, 1, 9),
        "value = "
    );
    report_test!(number_bad_end, Expr::Number(Number::End, 1, 9), "value = ");
    report_test!(let_missing_in, Expr::Let(&Let::In(1, 9), 1, 9), "value = ");
    report_test!(
        let_def_alignment,
        Expr::Let(&Let::DefAlignment(4, 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        let_def_indent_body,
        Expr::Let(&Let::Def("x", &Def::IndentBody(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        let_destruct_missing_equals,
        Expr::Let(&Let::Destruct(&Destruct::Equals(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_missing_of,
        Expr::Case(&Case::Of(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_missing_arrow,
        Expr::Case(&Case::Arrow(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_pattern_alignment,
        Expr::Case(&Case::PatternAlignment(4, 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_indent_branch,
        Expr::Case(&Case::IndentBranch(1, 9), 1, 9),
        "value = "
    );
    report_test!(if_missing_then, Expr::If(&If::Then(1, 9), 1, 9), "value = ");
    report_test!(
        if_else_branch_start,
        Expr::If(&If::ElseBranchStart(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_missing_end,
        Expr::Record(&Record::End(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_field_bad,
        Expr::Record(&Record::Field(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_indent_end,
        Expr::Record(&Record::IndentEnd(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        tuple_missing_end,
        Expr::Tuple(&Tuple::End(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        tuple_operator_close,
        Expr::Tuple(&Tuple::OperatorClose(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        list_missing_end,
        Expr::List(&List::End(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        list_indent_expr,
        Expr::List(&List::IndentExpr(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        func_missing_arrow,
        Expr::Func(&Func::Arrow(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        func_indent_body,
        Expr::Func(&Func::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        weird_end_in_def_context,
        Expr::OperatorReserved(BadOperator::Equals, 1, 9),
        "value = "
    );
    report_test!(
        case_colon_instead_of_cons,
        Expr::Case(&Case::Arrow(1, 9), 1, 9),
        "value = :"
    );
    report_test!(
        case_equals_instead_of_arrow,
        Expr::Case(&Case::Arrow(1, 9), 1, 9),
        "value = ="
    );
    #[test]
    fn missing_operand() {
        insta::assert_snapshot!(snapshot(Expr::OperatorRight("+", 1, 13), "value = 1 + "));
    }
    #[test]
    fn missing_else() {
        insta::assert_snapshot!(snapshot(
            Expr::If(&If::Else(1, 24), 1, 9),
            "value = if True then 42"
        ));
    }
    #[test]
    fn case_wrong_arrow() {
        insta::assert_snapshot!(snapshot(
            Expr::Case(&Case::Arrow(1, 30), 1, 9),
            "value = case x of Some width ="
        ));
    }
    #[test]
    fn record_reserved_field() {
        insta::assert_snapshot!(snapshot(
            Expr::Record(&Record::Open(1, 11), 1, 9),
            "value = { if = 1 }"
        ));
    }
    #[test]
    fn list_trailing_comma() {
        insta::assert_snapshot!(snapshot(
            Expr::List(&List::Expr(&Expr::Start(1, 13), 1, 13), 1, 9),
            "value = [1, ]"
        ));
    }
    #[test]
    fn do_requires_result() {
        insta::assert_snapshot!(snapshot(
            Expr::Do(&Do::LastNotExpr(2, 5), 1, 9),
            "value = do\n    let x = 1"
        ));
    }
    #[test]
    fn macro_missing_close() {
        insta::assert_snapshot!(snapshot(
            Expr::Macro(&Macro::End(1, 15), 1, 9),
            "value = foo!(1"
        ));
    }
    #[test]
    fn integer_dot() {
        insta::assert_snapshot!(snapshot(
            Expr::Number(Number::Dot(42), 1, 11),
            "value = 42."
        ));
    }
    #[test]
    fn unicode_escape() {
        insta::assert_snapshot!(snapshot(
            Expr::String(StringError::Escape(Escape::BadUnicodeCode(8)), 1, 10),
            "value = \"\\u{D800}\""
        ));
    }
    #[test]
    fn bytes_odd() {
        insta::assert_snapshot!(snapshot(
            Expr::Bytes(Bytes::OddLength, 1, 12),
            "value = #\"a\""
        ));
    }
    report_test!(
        if_indent_condition,
        Expr::If(&If::IndentCondition(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        if_indent_then,
        Expr::If(&If::IndentThen(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        if_indent_then_branch,
        Expr::If(&If::IndentThenBranch(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        if_indent_else_branch,
        Expr::If(&If::IndentElseBranch(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        if_indent_else,
        Expr::If(&If::IndentElse(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_indent_expr,
        Expr::Case(&Case::IndentExpr(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_indent_pattern,
        Expr::Case(&Case::IndentPattern(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_indent_arrow,
        Expr::Case(&Case::IndentArrow(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_indent_open,
        Expr::Record(&Record::IndentOpen(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_indent_field,
        Expr::Record(&Record::IndentField(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_equals,
        Expr::Record(&Record::Equals(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_indent_equals,
        Expr::Record(&Record::IndentEquals(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_indent_expr,
        Expr::Record(&Record::IndentExpr(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        tuple_indent_expr1,
        Expr::Tuple(&Tuple::IndentExpr1(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        tuple_indent_expr_n,
        Expr::Tuple(&Tuple::IndentExprN(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        tuple_indent_end,
        Expr::Tuple(&Tuple::IndentEnd(1, 9), 1, 9),
        "value = "
    );
    report_test!(list_open, Expr::List(&List::Open(1, 9), 1, 9), "value = ");
    report_test!(
        list_indent_open,
        Expr::List(&List::IndentOpen(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        list_indent_end,
        Expr::List(&List::IndentEnd(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        func_indent_arg,
        Expr::Func(&Func::IndentArg(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        func_indent_arrow,
        Expr::Func(&Func::IndentArrow(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        let_def_name,
        Expr::Let(&Let::DefName(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        let_indent_def,
        Expr::Let(&Let::IndentDef(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        let_indent_body,
        Expr::Let(&Let::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        macro_open,
        Expr::Macro(&Macro::Open(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        macro_indent_arg,
        Expr::Macro(&Macro::IndentArg(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        macro_indent_end,
        Expr::Macro(&Macro::IndentEnd(1, 9), 1, 9),
        "value = "
    );
    report_test!(do_arrow, Expr::Do(&Do::Arrow(1, 9), 1, 9), "value = ");
    report_test!(
        do_indent_stmt,
        Expr::Do(&Do::IndentStmt(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        do_indent_arrow,
        Expr::Do(&Do::IndentArrow(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        do_indent_expr,
        Expr::Do(&Do::IndentExpr(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        def_name_repeat,
        Expr::Let(&Let::Def("x", &Def::NameRepeat(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        def_equals,
        Expr::Let(&Let::Def("x", &Def::Equals(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        def_indent_equals,
        Expr::Let(&Let::Def("x", &Def::IndentEquals(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        def_indent_type,
        Expr::Let(&Let::Def("x", &Def::IndentType(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        operator_dot,
        Expr::OperatorReserved(BadOperator::Dot, 1, 9),
        "value = "
    );
    report_test!(
        operator_pipe,
        Expr::OperatorReserved(BadOperator::Pipe, 1, 9),
        "value = "
    );
    report_test!(
        operator_has_type,
        Expr::OperatorReserved(BadOperator::HasType, 1, 9),
        "value = "
    );
    report_test!(
        operator_fat_arrow,
        Expr::OperatorReserved(BadOperator::FatArrow, 1, 9),
        "value = "
    );
    report_test!(
        operator_left_arrow,
        Expr::OperatorReserved(BadOperator::LeftArrow, 1, 9),
        "value = "
    );
    report_test!(
        assert_indentbody,
        Expr::Assert(&Keyword::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        assert_indentmessage,
        Expr::Assert(&Keyword::IndentMessage(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        fail_indentbody,
        Expr::Fail(&Keyword::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        fail_indentmessage,
        Expr::Fail(&Keyword::IndentMessage(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        todo_indentbody,
        Expr::Todo(&Keyword::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        todo_indentmessage,
        Expr::Todo(&Keyword::IndentMessage(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        trace_indentbody,
        Expr::Trace(&Keyword::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        trace_indentmessage,
        Expr::Trace(&Keyword::IndentMessage(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        comptime_indentbody,
        Expr::Comptime(&Keyword::IndentBody(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        comptime_indentmessage,
        Expr::Comptime(&Keyword::IndentMessage(1, 9), 1, 9),
        "value = "
    );
    report_test!(
        record_double_comma,
        Expr::Record(&Record::Field(1, 9), 1, 9),
        "value = ,"
    );
    report_test!(
        record_trailing_comma,
        Expr::Record(&Record::Field(1, 9), 1, 9),
        "value = }"
    );
    report_test!(
        record_close_indentation,
        Expr::Record(&Record::IndentEnd(1, 9), 1, 9),
        "value = }"
    );
    report_test!(
        let_reserved_name,
        Expr::Let(&Let::DefName(1, 9), 1, 9),
        "value = if"
    );
    report_test!(
        func_reserved_arg,
        Expr::Func(&Func::Arrow(1, 9), 1, 9),
        "value = if"
    );
    report_test!(
        case_reserved_pattern,
        Expr::Case(&Case::Arrow(1, 9), 1, 9),
        "value = if"
    );
    report_test!(
        def_reserved_arg,
        Expr::Let(&Let::Def("x", &Def::Equals(1, 9), 1, 9), 1, 9),
        "value = if"
    );
    report_test!(
        def_missing_colon,
        Expr::Let(&Let::Def("x", &Def::Equals(1, 9), 1, 9), 1, 9),
        "value = ->"
    );
    report_test!(
        def_name_mismatch,
        Expr::Let(&Let::Def("x", &Def::NameMatch("y", 1, 9), 1, 9), 1, 9),
        "value = y"
    );
    report_test!(
        bytes_bad_hex,
        Expr::Bytes(Bytes::BadHexDigit(12), 1, 12),
        "value = #\"ag\""
    );
    report_test!(
        bytes_endless,
        Expr::Bytes(Bytes::Endless, 1, 12),
        "value = #\"aa"
    );
    report_test!(
        do_alignment,
        Expr::Do(&Do::Alignment(4, 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        def_alignment,
        Expr::Let(&Let::Def("x", &Def::Alignment(4, 1, 9), 1, 9), 1, 9),
        "value = "
    );
    macro_rules! parsed_test {
        ($name:ident, $input:expr) => {
            #[test]
            fn $name() {
                let input = $input;
                let bump = bumpalo::Bump::new();
                let error = nash_parse::Parser::new(&bump, input.as_bytes())
                    .module()
                    .expect_err("expected syntax error");
                let source = Source::new(input);
                let report = super::super::to_report(&source, &Error::ParseError(&error));
                insta::assert_snapshot!(crate::render_plain(&report, &source, "Main.nash"));
            }
        };
    }
    parsed_test!(parsed_if_missing_else, "value = if True then 42");
    parsed_test!(
        parsed_case_wrong_arrow,
        "value = case x of\n    Some width = width"
    );
    parsed_test!(parsed_record_reserved, "value = { if = 1 }");
    parsed_test!(parsed_list_trailing_comma, "value = [1, ]");
    parsed_test!(parsed_do_last_binding, "value = do\n    x <- action");
    parsed_test!(parsed_macro_close, "value = foo!(1");
    parsed_test!(parsed_bytes_bad_hex, "value = #\"ag\"");
    parsed_test!(parsed_unicode_short, "value = \"\\u{1}\"");
    parsed_test!(parsed_let_missing_in, "value = let x = 1");
    parsed_test!(parsed_lambda_missing_body, "value = \\x ->");
    report_test!(operand_boolean, Expr::OperatorRight("&&", 1, 9), "value = ");
    report_test!(operand_pipe, Expr::OperatorRight("|>", 1, 9), "value = ");
    report_test!(
        operand_reverse_pipe,
        Expr::OperatorRight("<|", 1, 9),
        "value = "
    );
    report_test!(
        operand_custom_operator,
        Expr::OperatorRight("++", 1, 9),
        "value = "
    );
    report_test!(
        destruct_type_annotation,
        Expr::Let(&Let::Destruct(&Destruct::Equals(1, 9), 1, 9), 1, 9),
        "value = :"
    );
    report_test!(
        destruct_indent_equals,
        Expr::Let(&Let::Destruct(&Destruct::IndentEquals(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        destruct_indent_body,
        Expr::Let(&Let::Destruct(&Destruct::IndentBody(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        case_subject_arrow,
        Expr::Case(
            &Case::Expr(&Expr::OperatorReserved(BadOperator::Arrow, 1, 9), 1, 9),
            1,
            9
        ),
        "value = ->"
    );
    report_test!(
        case_branch_arrow,
        Expr::Case(
            &Case::Branch(&Expr::OperatorReserved(BadOperator::Arrow, 1, 9), 1, 9),
            1,
            9
        ),
        "value = ->"
    );
    report_test!(
        if_misindented_else,
        Expr::If(&If::IndentElse(1, 25), 1, 9),
        "value = if True then 1\nelse 2"
    );
    report_test!(
        macro_missing_arg,
        Expr::Macro(&Macro::Arg(&Expr::Start(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        trace_missing_body,
        Expr::Trace(&Keyword::Body(&Expr::Start(1, 9), 1, 9), 1, 9),
        "value = "
    );
    report_test!(
        operator_indent_right,
        Expr::IndentOperatorRight("+", 1, 9),
        "value = "
    );
    report_test!(
        record_close_next_line,
        Expr::Record(&Record::IndentEnd(1, 19), 1, 9),
        "value = { name = 1\n}"
    );
    report_test!(
        record_unexpected_equals,
        Expr::Record(
            &Record::Expr(&Expr::OperatorReserved(BadOperator::Equals, 1, 9), 1, 9),
            1,
            9
        ),
        "value = ="
    );
}
