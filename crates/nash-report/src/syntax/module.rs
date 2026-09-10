use super::{Doc, Report, Source, closing, decl, expr, pattern, problem, to_space_report, wide};
use crate::code::{Next, to_keyword_region};
use nash_parse::error::{Exposing, Module, Test, Tests};
use nash_parse::{Col, Row};

pub(crate) fn to_parse_error_report(source: &Source<'_>, error: &Module<'_>) -> Report {
    match *error {
        Module::Space(ref e, r, c) => to_space_report(source, e, r, c),
        Module::BadEnd(r, 1) => decl::to_decl_start_report(source, r, 1),
        Module::BadEnd(r, c) => to_weird_end_report(source, r, c),
        Module::Problem(r, c) => problem(
            "UNFINISHED MODULE DECLARATION",
            r,
            c,
            "Incomplete module declaration.",
            "Write `module Name exposing (..)`.",
        ),
        Module::Name(r, c) => problem(
            "EXPECTED MODULE NAME",
            r,
            c,
            "Expected an uppercase module name.",
            "Use a name such as `Main` or `Cardano.Tx`.",
        ),
        Module::Validator(r, c) => problem(
            "UNFINISHED VALIDATOR",
            r,
            c,
            "Expected a module declaration after `validator`.",
            "Write `validator module Name exposing (main)`.",
        ),
        Module::Exposing(e, r, c) | Module::ImportExposingList(e, r, c) => {
            to_exposing_report(source, e, r, c)
        }
        Module::FreshLine(r, c) => problem(
            "DECLARATION PLACEMENT",
            r,
            c,
            "Top-level declarations must start on separate lines.",
            "Start the declaration in column 1 on a new line.",
        ),
        Module::ImportName(r, c) | Module::ImportIndentName(r, c) => problem(
            "EXPECTED IMPORT NAME",
            r,
            c,
            "Expected a module name after `import`.",
            "Use an uppercase module name.",
        ),
        Module::ImportAlias(r, c) | Module::ImportIndentAlias(r, c) => problem(
            "EXPECTED IMPORT ALIAS",
            r,
            c,
            "Expected an alias after `as`.",
            "Use an uppercase alias, as in `import Cardano.Tx as Tx`.",
        ),
        Module::ImportIndentExposingList(r, c) => problem(
            "EXPECTED EXPOSING LIST",
            r,
            c,
            "Expected an exposing list.",
            "Put exposed names inside parentheses.",
        ),
        Module::ImportStart(r, c)
        | Module::ImportAs(r, c)
        | Module::ImportExposing(r, c)
        | Module::ImportEnd(r, c) => problem(
            "UNFINISHED IMPORT",
            r,
            c,
            "Invalid import declaration.",
            "Write `import Module`, optionally followed by `as Alias` and `exposing (names)`.",
        ),
        Module::Infix(r, c) => problem(
            "INVALID INFIX",
            r,
            c,
            "Invalid infix declaration.",
            "Write `infix left 5 (++) = append`, with precedence from 0 to 9.",
        ),
        Module::Declarations(e, _, _) => decl::to_declarations_report(source, e),
        Module::Tests(e, r, c) => to_tests_report(source, e, r, c),
    }
}

pub(crate) fn to_weird_end_report(source: &Source<'_>, r: Row, c: Col) -> Report {
    match source.what_is_next(r, c) {
        Next::Close(_, ch) => problem(
            "STRAY DELIMITER",
            r,
            c,
            &format!("Unexpected closing `{ch}`."),
            "Remove the unmatched delimiter.",
        ),
        Next::Keyword(keyword) => Report::snippet(
            "RESERVED WORD",
            to_keyword_region(r, c, keyword),
            None,
            Doc::text(format!("Unexpected keyword `{keyword}`.")),
            Doc::text("Check the preceding expression and declaration indentation."),
        ),
        Next::Other(Some(';')) => problem(
            "UNEXPECTED SEMICOLON",
            r,
            c,
            "Semicolons do not separate declarations.",
            "Start a new line without indentation.",
        ),
        Next::Other(Some(',')) => problem(
            "UNEXPECTED COMMA",
            r,
            c,
            "Unexpected comma outside a collection.",
            "Remove it, or enclose the elements in a list, tuple, or record.",
        ),
        Next::Other(Some('`')) => problem(
            "UNEXPECTED BACKTICK",
            r,
            c,
            "Backtick function calls are not supported.",
            "Call the function by name before its arguments.",
        ),
        Next::Operator(op) => problem(
            "UNEXPECTED OPERATOR",
            r,
            c,
            &format!("Unexpected operator `({op})`."),
            "Put the operator in an expression, or indent this line to continue the preceding expression.",
        ),
        _ => problem(
            "UNEXPECTED INPUT",
            r,
            c,
            "Unexpected input after the declaration.",
            "Start the next declaration on a new line in column 1.",
        ),
    }
}

pub(super) fn to_exposing_report(
    source: &Source<'_>,
    error: &Exposing,
    sr: Row,
    sc: Col,
) -> Report {
    let report = match *error {
        Exposing::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Exposing::Start(r, c) => problem(
            "MISSING EXPOSING PARENTHESIS",
            r,
            c,
            "Expected `(` after `exposing`.",
            "Enclose the exposed names in parentheses.",
        ),
        Exposing::Value(r, c) | Exposing::IndentValue(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, keyword),
                None,
                Doc::text(format!("Cannot expose reserved word `{keyword}`.")),
                Doc::text("Expose a declared value, type, or operator."),
            ),
            Next::Operator(op) => problem(
                "EXPECTED EXPOSED NAME",
                r,
                c,
                "Exposed operators need parentheses.",
                &format!("Write `({op})`."),
            ),
            _ => problem(
                "EXPECTED EXPOSED NAME",
                r,
                c,
                "Expected a value, type, or operator to expose.",
                "Separate names with commas, or use `..` to expose everything.",
            ),
        },
        Exposing::Operator(r, c) => problem(
            "EXPECTED OPERATOR",
            r,
            c,
            "Expected an operator inside these parentheses.",
            "Write an operator such as `(+)`.",
        ),
        Exposing::OperatorReserved(ref error, r, c) => {
            return expr::to_operator_report(source, error, r, c);
        }
        Exposing::OperatorRightParen(opening, r, c) | Exposing::TypePrivacyEnd(opening, r, c) => {
            return closing(source, r, c, opening.line, opening.column, ')');
        }
        Exposing::End(r, c) | Exposing::IndentEnd(r, c) => {
            return closing(source, r, c, sr, sc, ')');
        }
        Exposing::TypePrivacy(r, c) => problem(
            "INVALID TYPE EXPOSING",
            r,
            c,
            "Invalid constructor exposure.",
            "Write `Type(..)` to expose all constructors, or omit `(..)` to keep them private.",
        ),
        Exposing::TypeName(r, c) => problem(
            "MISSING TYPE NAME",
            r,
            c,
            "Expected a lowercase type name after `type`.",
            "Write `type name` or `type name(..)`.",
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_tests_report(source: &Source<'_>, error: &Tests<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
        Tests::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Tests::Import(e, _, _) => return to_parse_error_report(source, e),
        Tests::Test(e, r, c) => return to_test_report(source, e, r, c),
        Tests::Start(r, c) | Tests::IndentStart(r, c) => problem(
            "EXPECTED TEST",
            r,
            c,
            "Expected a test or property declaration.",
            "Indent `test` or `prop` beneath `tests`; put test imports first.",
        ),
        Tests::Alignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Test declarations must align.",
            &format!("Start each declaration in column {indent}."),
        ),
    };
    wide(report, sr, sc)
}

pub(super) fn to_test_report(source: &Source<'_>, error: &Test<'_>, sr: Row, sc: Col) -> Report {
    let report = match *error {
        Test::Space(ref e, r, c) => return to_space_report(source, e, r, c),
        Test::Name(ref e, r, c) => return expr::to_string_report(source, e, r, c),
        Test::WithinNumber(ref e, r, c) => return expr::to_number_report(source, e, r, c),
        Test::Body(e, r, c) => {
            return expr::to_do_report(source, expr::Context::InDestruct(sr, sc), e, r, c);
        }
        Test::Pattern(e, r, c) => {
            return pattern::to_pattern_report(source, pattern::PContext::Let, e, r, c);
        }
        Test::Fuzzer(e, r, c) => {
            return expr::to_expr_report(source, expr::Context::InDestruct(sr, sc), e, r, c);
        }
        Test::NameStart(r, c) | Test::IndentName(r, c) => problem(
            "MISSING TEST NAME",
            r,
            c,
            "Expected a quoted test name.",
            "Give the test a name in double quotes.",
        ),
        Test::OnceOnUnitTest(r, c) => problem(
            "UNEXPECTED ONCE",
            r,
            c,
            "Unit tests already run once.",
            "Remove `once`, or use a property declaration for generated inputs.",
        ),
        Test::WithinOpen(r, c) => problem(
            "MISSING BUDGET PARENTHESIS",
            r,
            c,
            "Expected `(` after `within`.",
            "Put budget limits inside parentheses.",
        ),
        Test::WithinKind(r, c) => problem(
            "UNKNOWN TEST BUDGET",
            r,
            c,
            "Expected a budget kind.",
            "Use `cpu` or `mem`, followed by an integer limit.",
        ),
        Test::WithinDuplicate(r, c) => problem(
            "DUPLICATE TEST BUDGET",
            r,
            c,
            "Budget kind is specified more than once.",
            "Keep one limit for each budget kind.",
        ),
        Test::WithinEnd(opening, r, c) => closing(source, r, c, opening.line, opening.column, ')'),
        Test::Equals(r, c) | Test::IndentEquals(r, c) => problem(
            "MISSING TEST EQUALS",
            r,
            c,
            "Expected `=` before the test body.",
            "Add `=` after the test name and modifiers.",
        ),
        Test::Do(r, c) | Test::IndentBody(r, c) => problem(
            "MISSING TEST BODY",
            r,
            c,
            "Expected a do block for the test body.",
            "Write `do` and indent its statements.",
        ),
        Test::Let(r, c) | Test::IndentBinder(r, c) => problem(
            "MISSING PROPERTY BINDER",
            r,
            c,
            "Expected generated inputs for the property.",
            "Write `let pattern via fuzzer` before `in`.",
        ),
        Test::Via(r, c) => problem(
            "MISSING VIA",
            r,
            c,
            "Expected `via` after the input pattern.",
            "Add `via` followed by the fuzzer expression.",
        ),
        Test::In(r, c) | Test::IndentIn(r, c) => problem(
            "MISSING IN",
            r,
            c,
            "Expected `in` after the generated inputs.",
            "Add `in` before the property body.",
        ),
        Test::BinderAlignment(indent, r, c) => problem(
            "INDENTATION",
            r,
            c,
            "Property input bindings must align.",
            &format!("Start each binding in column {indent}."),
        ),
    };
    wide(report, sr, sc)
}
