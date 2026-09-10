//! Expression syntax diagnostics.
use super::{closing, pattern, problem, to_space_report, type_, wide};
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
fn width(mut report: Report, amount: usize) -> Report {
    report.region.end.column = report.region.start.column.saturating_add(amount);
    report.context = Some(report.region);
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
            "EXPECTED ACCESSOR",
            r,
            c,
            "Expected a record accessor.",
            "Write a dot followed by a field name, such as `.name`.",
        ),
        Expr::Access(r, c) => problem(
            "EXPECTED FIELD",
            r,
            c,
            "Expected a field name after `.`.",
            "Use a lowercase field name.",
        ),
        Expr::OperatorRight(op, r, c) | Expr::IndentOperatorRight(op, r, c) => wide(
            problem(
                "MISSING EXPRESSION",
                r,
                c,
                &format!("Expected an operand after `({op})`."),
                "Add an expression on the same line or indent it on the next line.",
            ),
            sr,
            sc,
        ),
        Expr::OperatorReserved(ref op, r, c) => operator_in_context(source, context, op, r, c),
        Expr::Start(r, c) => {
            if let Context::InNode(node, open_row, open_col, _) = context {
                let delimiter = match node {
                    Node::List => Some(']'),
                    Node::Parens => Some(')'),
                    Node::Record => Some('}'),
                    _ => None,
                };
                if let (Some(delimiter), Next::Close(_, found)) =
                    (delimiter, source.what_is_next(r, c))
                {
                    if found != delimiter {
                        return closing(source, r, c, open_row, open_col, delimiter);
                    }
                    return problem(
                        "MISSING EXPRESSION",
                        r,
                        c,
                        "Expected an expression before the closing delimiter.",
                        "Add the missing expression, or remove a trailing comma.",
                    );
                }
            }
            let (row, col, place) = context_start(context);
            wide(
                problem(
                    "MISSING EXPRESSION",
                    r,
                    c,
                    &format!("Expected an expression in {place}."),
                    "Add an expression or `todo` for an unfinished body.",
                ),
                row,
                col,
            )
        }
        Expr::String(ref e, r, c) => to_string_report(source, e, r, c),
        Expr::Bytes(ref e, r, c) => to_bytes_report(source, e, r, c),
        Expr::Number(ref e, r, c) => to_number_report(source, e, r, c),
        Expr::Space(ref e, r, c) => to_space_report(source, e, r, c),
    }
}

pub(crate) fn to_string_report(source: &Source<'_>, error: &StringError, r: Row, c: Col) -> Report {
    match error {
        StringError::EndlessSingle(opening) => {
            super::unclosed_literal("ENDLESS STRING", r, c, *opening, "\"", "\"")
        }
        StringError::EndlessMulti(opening) => {
            super::unclosed_literal("ENDLESS STRING", r, c, *opening, "\"\"\"", "\"\"\"")
        }
        StringError::Escape(error) => to_escape_report(source, error, r, c),
    }
}
fn to_escape_report(_source: &Source<'_>, error: &Escape, r: Row, c: Col) -> Report {
    match *error {
        Escape::Unknown => width(
            problem(
                "UNKNOWN ESCAPE",
                r,
                c,
                "Unknown escape sequence.",
                r#"Use \n, \r, \t, \", \', \\, or \u{0041}."#,
            ),
            2,
        ),
        Escape::BadUnicodeFormat(length) => width(
            problem(
                "BAD UNICODE ESCAPE",
                r,
                c,
                "Malformed Unicode escape.",
                r"Use \u{0041}: four to six hexadecimal digits inside braces.",
            ),
            length,
        ),
        Escape::BadUnicodeCode(length) => width(
            problem(
                "BAD UNICODE ESCAPE",
                r,
                c,
                "Invalid Unicode scalar value.",
                "Use 0000–10FFFF, excluding D800–DFFF.",
            ),
            length,
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
                &format!("Unicode escape needs {expected} digits, found {actual}."),
                "Use four to six hexadecimal digits; adjust leading zeros.",
            ),
            code,
        ),
    }
}
pub(crate) fn to_number_report(_source: &Source<'_>, error: &Number, r: Row, c: Col) -> Report {
    match error {
        Number::End => problem(
            "INVALID NUMBER",
            r,
            c,
            "Invalid integer literal.",
            "Use decimal or hexadecimal integers; floating point numbers are not supported.",
        ),
        Number::Dot(number) => problem(
            "INVALID NUMBER",
            r,
            c,
            "Floating point numbers are not supported.",
            &format!("Use an integer such as `{number}`."),
        ),
        Number::HexDigit => problem(
            "INVALID HEXADECIMAL",
            r,
            c,
            "Expected a hexadecimal digit.",
            "Use digits from `0123456789abcdefABCDEF`.",
        ),
        Number::NoLeadingZero => problem(
            "LEADING ZEROS",
            r,
            c,
            "Integer has leading zeros.",
            "Remove the leading zeros.",
        ),
    }
}
pub(crate) fn to_bytes_report(_source: &Source<'_>, error: &Bytes, r: Row, c: Col) -> Report {
    match error {
        Bytes::Endless(opening) => {
            super::unclosed_literal("ENDLESS BYTE STRING", r, c, *opening, "#\"", "\"")
        }
        Bytes::OddLength => problem(
            "INCOMPLETE BYTE",
            r,
            c,
            "Byte string has an odd number of hexadecimal digits.",
            "Use two hexadecimal digits per byte.",
        ),
        Bytes::BadHexDigit(bad_col) => wide(
            problem(
                "BAD BYTE STRING",
                r,
                *bad_col,
                "Invalid hexadecimal digit in byte string.",
                "Use digits from `0123456789abcdefABCDEF`.",
            ),
            r,
            c,
        ),
    }
}
pub(crate) fn to_operator_report(
    _source: &Source<'_>,
    error: &BadOperator,
    r: Row,
    c: Col,
) -> Report {
    let (token, hint) = match error {
        BadOperator::Dot => (".", "Use a dot directly before a record field name."),
        BadOperator::Pipe => (
            "|",
            "Use `||` for boolean disjunction; `|` belongs in datatype declarations and record updates.",
        ),
        BadOperator::Arrow => (
            "->",
            "Use `->` in function types, anonymous functions, or case branches.",
        ),
        BadOperator::Equals => (
            "=",
            "Use `==` to compare values; `=` defines a value or record field.",
        ),
        BadOperator::HasType => (
            ":",
            "Use `::` to prepend a list element; `:` introduces a type annotation.",
        ),
        BadOperator::FatArrow => (
            "=>",
            "Use `=>` after type constraints; use `->` for function and case bodies.",
        ),
        BadOperator::LeftArrow => ("<-", "Use `<-` for a binding inside a do block."),
    };
    width(
        problem(
            "UNEXPECTED SYMBOL",
            r,
            c,
            &format!("Unexpected `{token}` in expression."),
            hint,
        ),
        token.len(),
    )
}
fn operator_in_context(
    source: &Source<'_>,
    context: Context<'_>,
    error: &BadOperator,
    r: Row,
    c: Col,
) -> Report {
    let mut report = to_operator_report(source, error, r, c);
    if matches!(error, BadOperator::Arrow)
        && (is_within(Node::Case, context) || is_within(Node::Branch, context))
    {
        report.after = Doc::text(if is_within(Node::Case, context) {
            "Add `of` before the case branches."
        } else {
            "Align this pattern with the other case branches."
        });
    } else if matches!(error, BadOperator::Equals) && is_within(Node::Record, context) {
        report.after = Doc::text("Separate record fields with commas; use `==` for a comparison.");
    } else if matches!(error, BadOperator::Equals)
        && let Some(name) = get_def_name(context)
    {
        report.after = Doc::text(format!(
            "Use `==` for a comparison, or align a new definition with `{name}`."
        ));
    }
    report
}

fn to_if_report(source: &Source<'_>, ctx: Context<'_>, error: &If<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
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
        If::Then(r, c) | If::IndentThen(r, c) => problem(
            "MISSING THEN",
            r,
            c,
            "Expected `then` after the condition.",
            "Add `then` before the first branch.",
        ),
        If::Else(r, c) | If::IndentElse(r, c) => {
            if let Some((row, col)) = source.next_line_starts_with_keyword("else", r) {
                width(
                    problem(
                        "INDENTATION",
                        row,
                        col,
                        "The else branch is not indented enough.",
                        "Indent `else` to continue this if expression.",
                    ),
                    4,
                )
            } else {
                problem(
                    "MISSING ELSE",
                    r,
                    c,
                    "Expected an else branch.",
                    "Add `else` and an expression; every if expression needs both branches.",
                )
            }
        }
        If::ElseBranchStart(r, c) | If::IndentElseBranch(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected an expression after `else`.",
            "Add the second branch, indented inside the if expression.",
        ),
        If::IndentCondition(r, c) => problem(
            "MISSING CONDITION",
            r,
            c,
            "Expected an indented condition after `if`.",
            "Add a boolean expression.",
        ),
        If::IndentThenBranch(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected an expression after `then`.",
            "Add the first branch, indented inside the if expression.",
        ),
    };
    wide(report, sr, sc)
}

fn to_case_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Case<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Case::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Case::Pattern(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Case, e, r, c);
        }
        Case::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Case, sr, sc, &ctx), e, r, c);
        }
        Case::Branch(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Branch, sr, sc, &ctx), e, r, c);
        }
        Case::Of(r, c) | Case::IndentOf(r, c) => problem(
            "MISSING OF",
            r,
            c,
            "Expected `of` after the case value.",
            "Add `of` before the case branches.",
        ),
        Case::Arrow(r, c) | Case::IndentArrow(r, c) => problem(
            "MISSING ARROW",
            r,
            c,
            "Expected `->` after the pattern.",
            "Separate the pattern and branch body with `->`.",
        ),
        Case::IndentExpr(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a value after `case`.",
            "Add an indented expression followed by `of`.",
        ),
        Case::IndentPattern(r, c) => problem(
            "MISSING PATTERN",
            r,
            c,
            "Expected a case pattern.",
            "Indent the pattern beneath `case`.",
        ),
        Case::IndentBranch(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a case branch body.",
            "Indent an expression beneath the pattern.",
        ),
        Case::PatternAlignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Case patterns must align.",
            &format!("Indent this pattern with {indent} spaces."),
        ),
    };
    wide(report, sr, sc)
}

fn to_record_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Record<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Record::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Record::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Record, sr, sc, &ctx), e, r, c);
        }
        Record::End(r, c) | Record::IndentEnd(r, c) => return closing(source, r, c, sr, sc, '}'),
        Record::Open(r, c) | Record::Field(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != '}') =>
        {
            return closing(source, r, c, sr, sc, '}');
        }
        Record::Open(r, c) | Record::Field(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => width(
                problem(
                    "RESERVED WORD",
                    r,
                    c,
                    &format!("Reserved word `{keyword}` cannot be a field name."),
                    "Choose another field name.",
                ),
                keyword.len(),
            ),
            Next::Other(Some(',')) => problem(
                "EXTRA COMMA",
                r,
                c,
                "Extra comma in record.",
                "Remove the repeated comma.",
            ),
            Next::Close(_, '}') => problem(
                "EXTRA COMMA",
                r,
                c,
                "Trailing comma in record.",
                "Remove the comma before `}`.",
            ),
            _ => problem(
                "EXPECTED FIELD",
                r,
                c,
                "Expected a record field.",
                "Write `name = value`; separate fields with commas.",
            ),
        },
        Record::Equals(r, c) | Record::IndentEquals(r, c) => problem(
            "MISSING EQUALS",
            r,
            c,
            "Expected `=` after the field name.",
            "Write `name = value` for a record field.",
        ),
        Record::IndentOpen(r, c) | Record::IndentField(r, c) => problem(
            "EXPECTED FIELD",
            r,
            c,
            "Expected an indented record field.",
            "Indent the field inside the braces.",
        ),
        Record::IndentExpr(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a field value after `=`.",
            "Add an indented expression.",
        ),
    };
    wide(report, sr, sc)
}

fn to_tuple_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Tuple<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Tuple::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Tuple::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Parens, sr, sc, &ctx), e, r, c);
        }
        Tuple::OperatorReserved(ref op, r, c) => return to_operator_report(source, op, r, c),
        Tuple::End(r, c) | Tuple::IndentEnd(r, c) | Tuple::OperatorClose(r, c) => {
            return closing(source, r, c, sr, sc, ')');
        }
        Tuple::IndentExpr1(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != ')') =>
        {
            return closing(source, r, c, sr, sc, ')');
        }
        Tuple::IndentExpr1(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected an expression or `)`.",
            "Add an expression inside the parentheses.",
        ),
        Tuple::IndentExprN(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a tuple element after `,`.",
            "Add an indented expression, or remove a trailing comma.",
        ),
    };
    wide(report, sr, sc)
}

fn to_list_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &List<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        List::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        List::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::List, sr, sc, &ctx), e, r, c);
        }
        List::End(r, c) | List::IndentEnd(r, c) => return closing(source, r, c, sr, sc, ']'),
        List::Open(r, c) | List::IndentOpen(r, c) if matches!(source.what_is_next(r, c), Next::Close(_, found) if found != ']') =>
        {
            return closing(source, r, c, sr, sc, ']');
        }
        List::Open(r, c) | List::IndentOpen(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a list element or `]`.",
            "Add an indented expression, or close an empty list.",
        ),
        List::IndentExpr(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a list element after `,`.",
            "Add an indented expression, or remove a trailing comma.",
        ),
    };
    wide(report, sr, sc)
}

fn to_func_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Func<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Func::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Func::Arg(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Arg, e, r, c);
        }
        Func::Body(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Func, sr, sc, &ctx), e, r, c);
        }
        Func::Arrow(r, c) | Func::IndentArrow(r, c) => problem(
            "MISSING ARROW",
            r,
            c,
            "Expected `->` after the arguments.",
            "Separate the argument patterns and function body with `->`.",
        ),
        Func::IndentArg(r, c) => problem(
            "MISSING PATTERN",
            r,
            c,
            "Expected an argument pattern after `\\`.",
            "Add an indented argument pattern.",
        ),
        Func::IndentBody(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a function body after `->`.",
            "Add an indented expression.",
        ),
    };
    wide(report, sr, sc)
}

fn to_let_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Let<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Let::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Let::Def(name, e, r, c) => return to_let_def_report(source, name, e, r, c),
        Let::Destruct(e, r, c) => return to_let_destruct_report(source, e, r, c),
        Let::Body(e, r, c) => return to_expr_report(source, ctx, e, r, c),
        Let::In(r, c) | Let::IndentIn(r, c) => problem(
            "MISSING IN",
            r,
            c,
            "Expected `in` after the let definitions.",
            "Add `in` before the expression that uses these definitions.",
        ),
        Let::DefAlignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Let definitions must align.",
            &format!("Indent this definition with {indent} spaces."),
        ),
        Let::DefName(r, c) | Let::IndentDef(r, c) => problem(
            "MISSING DEFINITION",
            r,
            c,
            "Expected a definition after `let`.",
            "Add an indented definition.",
        ),
        Let::IndentBody(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a body after `in`.",
            "Add an indented expression.",
        ),
    };
    wide(report, sr, sc)
}

pub(crate) fn to_let_def_report(
    source: &Source<'_>,
    name: &str,
    error: &Def<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Def::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Def::Type(e, r, c) => {
            return type_::to_type_report(source, type_::TContext::Annotation(name), e, r, c);
        }
        Def::Arg(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Arg, e, r, c);
        }
        Def::Body(e, r, c) => return to_expr_report(source, Context::InDef(name, sr, sc), e, r, c),
        Def::NameRepeat(r, c) => problem(
            "MISSING DEFINITION",
            r,
            c,
            &format!("Expected the definition of `{name}` after its annotation."),
            "Repeat the annotated name and add its arguments and body.",
        ),
        Def::NameMatch(found, r, c) => problem(
            "NAME MISMATCH",
            r,
            c,
            &format!("Expected definition `{name}`, found `{found}`."),
            "Use the same name for an annotation and its definition.",
        ),
        Def::Equals(r, c) | Def::IndentEquals(r, c) => problem(
            "MISSING EQUALS",
            r,
            c,
            &format!("Expected `=` in the definition of `{name}`."),
            "Separate the arguments and body with `=`.",
        ),
        Def::IndentType(r, c) => problem(
            "MISSING TYPE",
            r,
            c,
            "Expected a type after `:`.",
            "Add an indented type annotation.",
        ),
        Def::IndentBody(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a body after `=`.",
            "Add an indented expression.",
        ),
        Def::Alignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Definition and annotation must align.",
            &format!("Indent the definition with {indent} spaces."),
        ),
    };
    wide(report, sr, sc)
}

fn to_let_destruct_report(source: &Source<'_>, error: &Destruct<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
        Destruct::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Destruct::Pattern(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Let, e, r, c);
        }
        Destruct::Body(e, r, c) => {
            return to_expr_report(source, Context::InDestruct(sr, sc), e, r, c);
        }
        Destruct::Equals(r, c) | Destruct::IndentEquals(r, c) => problem(
            "MISSING EQUALS",
            r,
            c,
            "Expected `=` after the binding pattern.",
            if matches!(source.what_is_next(r, c), Next::Operator(":")) {
                "Annotate a named definition; destructuring bindings cannot have annotations."
            } else {
                "Add `=` before the value to destructure."
            },
        ),
        Destruct::IndentBody(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected a value after `=`.",
            "Add an indented expression.",
        ),
    };
    wide(report, sr, sc)
}

fn to_keyword_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    keyword: &'static str,
    error: &Keyword<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
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
        Keyword::IndentBody(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            &format!("Expected a body after `{keyword}`."),
            "Add an indented expression.",
        ),
        Keyword::IndentMessage(r, c) => problem(
            "MISSING MESSAGE",
            r,
            c,
            &format!("Expected a message for `{keyword}`."),
            "Add an indented message expression.",
        ),
    };
    wide(report, sr, sc)
}

pub(crate) fn to_do_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Do<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Do::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Do::Let(e, r, c) => return to_let_report(source, ctx, e, r, c),
        Do::Pattern(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Let, e, r, c);
        }
        Do::Expr(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Do, sr, sc, &ctx), e, r, c);
        }
        Do::Arrow(r, c) | Do::IndentArrow(r, c) => problem(
            "MISSING BIND ARROW",
            r,
            c,
            "Expected `<-` after the binding pattern.",
            "Separate the pattern and computation with `<-`.",
        ),
        Do::LastNotExpr(r, c) => problem(
            "MISSING RESULT",
            r,
            c,
            "A do block must end with an expression.",
            "Add a final expression after this binding.",
        ),
        Do::IndentStmt(r, c) => problem(
            "MISSING STATEMENT",
            r,
            c,
            "Expected a statement after `do`.",
            "Add an indented statement.",
        ),
        Do::IndentExpr(r, c) => problem(
            "MISSING EXPRESSION",
            r,
            c,
            "Expected an expression after `<-`.",
            "Add an indented computation.",
        ),
        Do::Alignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Do statements must align.",
            &format!("Indent the statement with {indent} spaces."),
        ),
    };
    wide(report, sr, sc)
}

fn to_macro_report(
    source: &Source<'_>,
    ctx: Context<'_>,
    error: &Macro<'_>,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Macro::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Macro::Arg(e, r, c) => {
            return to_expr_report(source, Context::InNode(Node::Macro, sr, sc, &ctx), e, r, c);
        }
        Macro::End(r, c) | Macro::IndentEnd(r, c) => {
            return closing(source, r, c, sr, sc.saturating_add(1), ')');
        }
        Macro::Open(r, c) => problem(
            "MISSING PARENTHESIS",
            r,
            c,
            "Expected `(` after the macro marker `!`.",
            "Write `name!(arguments)`.",
        ),
        Macro::IndentArg(r, c) => problem(
            "MISSING ARGUMENT",
            r,
            c,
            "Expected a macro argument.",
            "Add an indented expression inside the parentheses.",
        ),
    };
    wide(report, sr, sc)
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
        Expr::String(
            StringError::EndlessSingle(nash_region::Position::new(1, 9)),
            1,
            9
        ),
        "value = "
    );
    report_test!(
        string_endless_multi,
        Expr::String(
            StringError::EndlessMulti(nash_region::Position::new(1, 9)),
            1,
            9
        ),
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
        Expr::Bytes(Bytes::Endless(nash_region::Position::new(1, 9)), 1, 12),
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
                let error = nash_parse::Parser::new(&bump, input)
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
