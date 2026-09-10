use super::{Doc, Report, Source, decl, expr, pattern, problem, to_space_report, wide};
use crate::code::{Next, to_keyword_region};
use nash_parse::error::{BadOperator, Exposing, Module, Test, Tests};
use nash_parse::{Col, Row};

pub(crate) fn to_parse_error_report(source: &Source<'_>, error: &Module<'_>) -> Report {
    match *error {
        Module::Space(ref e, r, c) => to_space_report(source, e, r, c),
        Module::BadEnd(r, 1) => decl::to_decl_start_report(source, r, 1),
        Module::BadEnd(r, c) => to_weird_end_report(source, r, c),
        Module::Problem(r, c) => {
            let mut report = problem(
                "UNFINISHED MODULE DECLARATION",
                r,
                c,
                "I am parsing a `module` declaration, but I got stuck here:",
                "Here are some examples of valid `module` declarations:",
            );
            report.after = Doc::stack([
                report.after,
                examples(&[
                    "module Main exposing (..)",
                    "module Dict exposing (Dict, empty, get)",
                ]),
                Doc::reflow(
                    "I generally recommend using an explicit exposing list. I can skip compiling a bunch of files when the public interface of a module stays the same, so exposing fewer values can help improve compile times!",
                ),
            ]);
            report
        }
        Module::Name(r, c) => {
            let mut report = problem(
                "EXPECTING MODULE NAME",
                r,
                c,
                "I was parsing a `module` declaration until I got stuck here:",
                "I was expecting to see the module name next, like in these examples:",
            );
            report.after = Doc::stack([
                report.after,
                examples(&[
                    "module Dict exposing (..)",
                    "module Option exposing (..)",
                    "module Cardano.Tx exposing (..)",
                    "module Data.Decoder exposing (..)",
                ]),
                Doc::reflow(
                    "Notice that the module names all start with capital letters. That is required!",
                ),
            ]);
            report
        }
        Module::Validator(r, c) => problem(
            "UNFINISHED VALIDATOR",
            r,
            c,
            "I was parsing a validator declaration, but I got stuck here:",
            "A validator module starts with `validator module`, followed by its module name and exposing list. For example: `validator module Vesting exposing (main)`.",
        ),
        Module::Exposing(e, r, c) | Module::ImportExposingList(e, r, c) => {
            to_exposing_report(source, e, r, c)
        }
        Module::FreshLine(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(keyword) => problem(
                "TOO MUCH INDENTATION",
                r,
                c,
                &format!("This `{keyword}` should not have any spaces before it:"),
                &format!("Delete the spaces before `{keyword}` until there are none left!"),
            ),
            _ => problem(
                "SYNTAX PROBLEM",
                r,
                c,
                "I got stuck here:",
                "A top-level declaration must start on a fresh line with no spaces before it. Move this declaration to its own line.",
            ),
        },
        Module::ImportName(r, c) => {
            let mut report = problem(
                "EXPECTING IMPORT NAME",
                r,
                c,
                "I was parsing an `import` until I got stuck here:",
                "I was expecting to see a module name next, like in these examples:",
            );
            report.after = Doc::stack([
                report.after,
                examples(&[
                    "import Dict",
                    "import Option",
                    "import Cardano.Tx as Tx",
                    "import Data.Decoder exposing (..)",
                ]),
                Doc::reflow(
                    "Notice that the module names all start with capital letters. That is required!",
                ),
            ]);
            report
        }
        Module::ImportAlias(r, c) => {
            let mut report = problem(
                "EXPECTING IMPORT ALIAS",
                r,
                c,
                "I was parsing an `import` until I got stuck here:",
                "I was expecting to see an alias next, like in these examples:",
            );
            report.after = Doc::stack([
                report.after,
                examples(&["import Cardano.Tx as Tx", "import Data.Decoder as D"]),
                Doc::reflow(
                    "Notice that the alias always starts with a capital letter. That is required!",
                ),
            ]);
            report
        }
        Module::ImportIndentExposingList(r, c) => {
            let mut report = problem(
                "UNFINISHED IMPORT",
                r,
                c,
                "I was parsing an `import` until I got stuck here:",
                "I was expecting to see the list of exposed values next.",
            );
            report.after = Doc::stack([
                report.after,
                examples(&[
                    "import Data.Decoder exposing (..)",
                    "import Data.Decoder exposing (decode)",
                ]),
                Doc::reflow(
                    "I generally recommend the second style. It is more explicit, making it much easier to figure out where values are coming from in large projects!",
                ),
            ]);
            report
        }
        Module::ImportStart(r, c)
        | Module::ImportAs(r, c)
        | Module::ImportExposing(r, c)
        | Module::ImportEnd(r, c)
        | Module::ImportIndentName(r, c)
        | Module::ImportIndentAlias(r, c) => to_import_report(r, c),
        Module::Infix(r, c) => problem(
            "BAD INFIX",
            r,
            c,
            "Something went wrong in this infix operator declaration:",
            "An infix declaration gives associativity, precedence, an operator in parentheses, and its implementation name. For example: `infix left 6 (+) = add`.",
        ),
        Module::Declarations(e, _, _) => decl::to_declarations_report(source, e),
        Module::Tests(e, r, c) => to_tests_report(source, e, r, c),
    }
}

fn examples(lines: &[&str]) -> Doc {
    Doc::indent(4, Doc::vcat(lines.iter().map(|s| Doc::text(*s))))
}
fn to_import_report(r: Row, c: Col) -> Report {
    let mut report = problem(
        "UNFINISHED IMPORT",
        r,
        c,
        "I am partway through parsing an import, but I got stuck here:",
        "Here are some examples of valid `import` declarations:",
    );
    report.after = Doc::stack([
        report.after,
        examples(&[
            "import Cardano.Tx",
            "import Cardano.Tx as Tx",
            "import Cardano.Tx as Tx exposing (..)",
            "import Data.Decoder exposing (decode)",
        ]),
        Doc::reflow(
            "You are probably trying to import a different module, but try to make it look like one of these examples!",
        ),
    ]);
    report
}

pub(crate) fn to_weird_end_report(source: &Source<'_>, r: Row, c: Col) -> Report {
    match source.what_is_next(r, c) {
        Next::Keyword(k) => Report::snippet(
            "RESERVED WORD",
            to_keyword_region(r, c, k),
            None,
            Doc::reflow("I got stuck on this reserved word:"),
            Doc::reflow(&format!(
                "The name `{k}` is reserved, so try using a different name?"
            )),
        ),
        Next::Operator(op) => Report::snippet(
            "UNEXPECTED SYMBOL",
            to_keyword_region(r, c, op),
            None,
            Doc::reflow("I ran into an unexpected symbol:"),
            Doc::reflow(&format!(
                "I was not expecting to see a {op} here. Try deleting it? Maybe I can give a better hint from there?"
            )),
        ),
        Next::Close(term, ch) => problem(
            &format!("UNEXPECTED {}", term.to_uppercase()),
            r,
            c,
            &format!("I ran into an unexpected {term}:"),
            &format!("This {ch} does not match up with an earlier open {term}. Try deleting it?"),
        ),
        Next::Lower(name) | Next::Upper(name) => Report::snippet(
            "UNEXPECTED NAME",
            to_keyword_region(r, c, name),
            None,
            Doc::reflow("I got stuck on this name:"),
            Doc::reflow(
                "It is confusing me a lot! Normally I can give fairly specific hints, but something is really tripping me up this time.",
            ),
        ),
        Next::Other(Some(';')) => {
            let mut report = problem(
                "UNEXPECTED SEMICOLON",
                r,
                c,
                "I got stuck on this semicolon:",
                "Try removing it?",
            );
            report.after = Doc::stack([
                report.after,
                Doc::to_simple_note(
                    "Some languages require semicolons at the end of each statement. Nash uses indentation to separate declarations and statements in do blocks, so there is no need to use semicolons to separate them.",
                ),
            ]);
            report
        }
        Next::Other(Some(',')) => {
            let mut report = problem(
                "UNEXPECTED COMMA",
                r,
                c,
                "I got stuck on this comma:",
                "I do not think I am parsing a list or tuple right now. Try deleting the comma?",
            );
            report.after = Doc::stack([
                report.after,
                Doc::to_simple_note(
                    "If this is supposed to be part of a list, the problem may be a bit earlier. Perhaps the opening [ is missing? Or perhaps some value in the list has an extra closing ] that is making me think the list ended earlier? The same kinds of things could be going wrong if this is supposed to be a tuple.",
                ),
            ]);
            report
        }
        Next::Other(Some('`')) => {
            let mut report = problem(
                "UNEXPECTED CHARACTER",
                r,
                c,
                "I got stuck on this character:",
                "It is not used for anything in Nash syntax. It is used for multi-line strings in some languages though, so if you want a string that spans multiple lines, you can use Nash's multi-line string syntax like this:",
            );
            report.after = Doc::stack([
                report.after,
                examples(&[
                    "\"\"\"",
                    "# Multi-line Strings",
                    "",
                    "- start with triple double quotes",
                    "- write whatever you want",
                    "- no need to escape newlines or double quotes",
                    "- end with triple double quotes",
                    "\"\"\"",
                ])
                .dullyellow(),
                Doc::reflow(
                    "Otherwise I do not know what is going on! Try removing the character?",
                ),
            ]);
            report
        }
        Next::Other(Some(ch)) => problem(
            "UNEXPECTED CHARACTER",
            r,
            c,
            "I got stuck on this character:",
            &format!("It is not a character I expect here (`{ch}`). Try deleting it?"),
        ),
        Next::Other(None) => problem(
            "UNFINISHED FILE",
            r,
            c,
            "I got to the end of the file, but I was expecting more.",
            "Maybe a declaration or an expression is incomplete?",
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
        Exposing::Start(r, c) => {
            let mut report = problem(
                "PROBLEM IN EXPOSING",
                r,
                c,
                "I want to parse exposed values, but I am getting stuck here:",
                "Exposed values are always surrounded by parentheses. So try adding a ( here?",
            );
            report.after = Doc::stack([
                report.after,
                Doc::to_simple_note("Here are some valid examples of `exposing` for reference:"),
                examples(&[
                    "import Data.Decoder exposing (..)",
                    "import Data.Decoder exposing (decode)",
                ]),
                Doc::reflow(
                    "If you are getting tripped up, you can just expose everything for now. It should get easier to make an explicit exposing list as you see more examples in the wild.",
                ),
            ]);
            report
        }
        Exposing::Value(r, c) => match source.what_is_next(r, c) {
            Next::Keyword(k) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(r, c, k),
                None,
                Doc::reflow("I got stuck on this reserved word:"),
                Doc::reflow(&format!(
                    "It looks like you are trying to expose `{k}` but that is a reserved word. Is there a typo?"
                )),
            ),
            Next::Operator(op) => Report::snippet(
                "UNEXPECTED SYMBOL",
                to_keyword_region(r, c, op),
                None,
                Doc::reflow("I got stuck on this symbol:"),
                Doc::stack([
                    Doc::reflow(
                        "If you are trying to expose an operator, add parentheses around it like this:",
                    ),
                    Doc::indent(
                        4,
                        Doc::cat([
                            Doc::text(op).dullyellow(),
                            Doc::text(" -> "),
                            Doc::text(format!("({op})")).green(),
                        ]),
                    ),
                ]),
            ),
            _ => {
                let mut report = problem(
                    "PROBLEM IN EXPOSING",
                    r,
                    c,
                    "I got stuck while parsing these exposed values:",
                    "I do not have an exact recommendation, so here are some valid examples of `exposing` for reference:",
                );
                report.after = Doc::stack([
                    report.after,
                    examples(&[
                        "import Data.Decoder exposing (..)",
                        "import Basics exposing (type int, type bool(..), (+), not)",
                    ]),
                    Doc::reflow(
                        "These examples show how to expose types, variants, operators, and functions. Everything should be some permutation of these examples, just with different names.",
                    ),
                ]);
                report
            }
        },
        Exposing::Operator(r, c) => problem(
            "PROBLEM IN EXPOSING",
            r,
            c,
            "I just saw an open parenthesis, so I was expecting an operator next:",
            "It is possible to expose operators, so I was expecting to see something like (+) or (|=) or (||) after I saw that open parenthesis.",
        ),
        Exposing::OperatorReserved(ref op, r, c) => problem(
            "RESERVED SYMBOL",
            r,
            c,
            "I cannot expose this as an operator:",
            match op {
                BadOperator::Pipe => "Maybe you want (||) instead?",
                BadOperator::Equals => "Maybe you want (==) instead?",
                BadOperator::HasType => "Maybe you want (::) instead?",
                BadOperator::Dot
                | BadOperator::Arrow
                | BadOperator::FatArrow
                | BadOperator::LeftArrow => {
                    "Try getting rid of this entry? Maybe I can give you a better hint after that?"
                }
            },
        ),
        Exposing::OperatorRightParen(r, c) => problem(
            "PROBLEM IN EXPOSING",
            r,
            c,
            "It looks like you are exposing an operator, but I got stuck here:",
            "I was expecting to see the closing parenthesis immediately after the operator. Try adding a ) right here?",
        ),
        Exposing::TypePrivacy(r, c) => {
            let mut report = problem(
                "PROBLEM EXPOSING CUSTOM TYPE VARIANTS",
                r,
                c,
                "It looks like you are trying to expose the variants of a custom type:",
                "You need to write something like Status(..) or Entity(..) though. It is all or nothing, otherwise `case` expressions could miss a variant and crash!",
            );
            report.after = Doc::stack([
                report.after,
                Doc::to_simple_note(
                    "It is often best to keep the variants hidden! If someone pattern matches on the variants, it is a MAJOR change if any new variants are added. Suddenly their `case` expressions do not cover all variants! So if you do not need people to pattern match, keep the variants hidden and expose functions to construct values of this type. This way you can add new variants as a MINOR change!",
                ),
            ]);
            report
        }
        Exposing::TypeName(r, c) => problem(
            "EXPECTING TYPE NAME",
            r,
            c,
            "I was parsing an exposed type, but I got stuck here:",
            "Write the name of the type after `type`. Use `type name(..)` to expose its constructors as well.",
        ),
        Exposing::End(r, c) => problem(
            "UNFINISHED EXPOSING",
            r,
            c,
            "I was partway through parsing exposed values, but I got stuck here:",
            "Maybe there is a comma missing before this?",
        ),
        Exposing::IndentEnd(r, c) => {
            let mut report = problem(
                "UNFINISHED EXPOSING",
                r,
                c,
                "I was partway through parsing exposed values, but I got stuck here:",
                "I was expecting a closing parenthesis. Try adding a ) right here?",
            );
            report.after = Doc::stack([
                report.after,
                Doc::to_simple_note(
                    "I can get confused when there is not enough indentation, so if you already have a closing parenthesis, it probably just needs some spaces in front of it.",
                ),
            ]);
            report
        }
        Exposing::IndentValue(r, c) => problem(
            "UNFINISHED EXPOSING",
            r,
            c,
            "I was partway through parsing exposed values, but I got stuck here:",
            "I was expecting another value to expose.",
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
            "UNFINISHED TESTS",
            r,
            c,
            "I started parsing a tests section, but I got stuck here:",
            "Add an indented `test` or `prop` declaration. Test imports must come before the declarations.",
        ),
        Tests::Alignment(indent, r, c) => problem(
            "TEST ALIGNMENT",
            r,
            c,
            "This test declaration does not line up with the others:",
            &format!("Indent every declaration in this tests section to column {indent}."),
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
            "I was parsing a test declaration, but I got stuck here:",
            "Give this test a name in double quotes, such as `test \"adds two numbers\"`.",
        ),
        Test::OnceOnUnitTest(r, c) => problem(
            "UNEXPECTED ONCE",
            r,
            c,
            "I found `once` on a unit test:",
            "Unit tests already run once. Remove `once`, or use a property declaration when you need generated inputs.",
        ),
        Test::WithinOpen(r, c) => problem(
            "UNFINISHED TEST BUDGET",
            r,
            c,
            "I saw `within`, but I got stuck here:",
            "Put the budget inside parentheses, with a budget kind and an integer limit.",
        ),
        Test::WithinKind(r, c) => problem(
            "UNKNOWN TEST BUDGET",
            r,
            c,
            "I was expecting a test budget kind here:",
            "Use `cpu` or `mem` followed by an integer limit.",
        ),
        Test::WithinDuplicate(r, c) => problem(
            "DUPLICATE TEST BUDGET",
            r,
            c,
            "This budget kind has already been specified:",
            "Keep one limit for each budget kind in the `within` clause.",
        ),
        Test::WithinEnd(r, c) => problem(
            "UNFINISHED TEST BUDGET",
            r,
            c,
            "I was parsing the `within` clause, but I got stuck here:",
            "Separate budget limits with a comma, and close the clause with ).",
        ),
        Test::Equals(r, c) | Test::IndentEquals(r, c) => problem(
            "MISSING TEST EQUALS",
            r,
            c,
            "I have the test name, but I got stuck here:",
            "Add an = before the test body.",
        ),
        Test::Do(r, c) | Test::IndentBody(r, c) => problem(
            "MISSING TEST BODY",
            r,
            c,
            "I was expecting the test body here:",
            "Start the test body with `do`, then indent its statements on the following lines.",
        ),
        Test::Let(r, c) | Test::IndentBinder(r, c) => problem(
            "MISSING PROPERTY BINDER",
            r,
            c,
            "I was parsing generated inputs for a property, but I got stuck here:",
            "Start the generated inputs with `let`, then write each pattern followed by `via` and its fuzzer.",
        ),
        Test::Via(r, c) => problem(
            "MISSING FUZZER",
            r,
            c,
            "I have the property input pattern, but I got stuck here:",
            "Add `via` followed by the fuzzer expression that generates this input.",
        ),
        Test::In(r, c) | Test::IndentIn(r, c) => problem(
            "MISSING PROPERTY IN",
            r,
            c,
            "I was parsing a property, but I got stuck here:",
            "Add `in` after the generated inputs and before the property body.",
        ),
        Test::BinderAlignment(indent, r, c) => problem(
            "PROPERTY BINDER ALIGNMENT",
            r,
            c,
            "This generated input does not line up with the others:",
            &format!("Indent each generated input to column {indent}."),
        ),
    };
    wide(report, sr, sc)
}
