use super::*;
use crate::render_plain;

fn parse_error_report(input: &str) -> String {
    let bump = bumpalo::Bump::new();
    let error = nash_parse::Parser::new(&bump, input.as_bytes())
        .module()
        .expect_err("expected parse error");
    render_plain(
        &to_report(&Source::new(input), &Error::ParseError(&error)),
        &Source::new(input),
        "src/Main.nash",
    )
}

macro_rules! syntax_snapshot {
    ($name:ident, $input:expr) => {
        #[test]
        fn $name() {
            let input = $input;
            insta::with_settings!({ description => format!("Code:\n\n{input}"), omit_expression => true }, {
                insta::assert_snapshot!(parse_error_report(input));
            });
        }
    };
}

syntax_snapshot!(module_problem, "module");
syntax_snapshot!(module_name_lowercase, "module main exposing (..)");
syntax_snapshot!(exposing_missing_paren, "module Main exposing ..");
syntax_snapshot!(exposing_value_bad, "module Main exposing (1)");
syntax_snapshot!(import_missing_name, "import");
syntax_snapshot!(import_bad_alias, "import Cardano.Tx as tx");
syntax_snapshot!(
    import_exposing_list_missing_paren,
    "import Cardano.Tx exposing x"
);
syntax_snapshot!(space_has_tab, "x =\t1");
syntax_snapshot!(space_endless_comment, "{- unfinished");
syntax_snapshot!(weird_end_reserved_word, "module Main exposing (..)\n\nif");
syntax_snapshot!(weird_end_close_paren, "module Main exposing (..)\n\n)");
syntax_snapshot!(weird_end_operator, "module Main exposing (..)\n\n+");
syntax_snapshot!(fresh_line_after_decl, "x = 1 y = 2");
syntax_snapshot!(type_alias_missing_equals, "type alias account");
syntax_snapshot!(type_alias_bad_body, "type alias account = 42");
syntax_snapshot!(custom_type_missing_variant, "type option 'a =");
syntax_snapshot!(decl_def_missing_equals, "f x");
syntax_snapshot!(decl_def_name_match, "f : int\ng = 1");
syntax_snapshot!(decl_def_indent_body, "f =\n1");
syntax_snapshot!(pattern_alias_missing_name, "f (x as) = x");
syntax_snapshot!(pattern_wildcard_not_var, "f _foo = 1");
syntax_snapshot!(pattern_record_missing_end, "f {x = 1");
syntax_snapshot!(pattern_tuple_missing_end, "f (x = 1");
syntax_snapshot!(pattern_list_missing_end, "f [x = 1");
syntax_snapshot!(type_start_bad_in_annotation, "f : 42\nf = 1");
syntax_snapshot!(type_record_missing_colon, "f : { x int }\nf = 1");
syntax_snapshot!(type_tuple_missing_end, "f : (int, int\nf = 1");

#[test]
fn module_name_missing() {
    insta::assert_snapshot!(render_plain(
        &to_report(&Source::new(""), &Error::ModuleNameUnspecified("Main")),
        &Source::new(""),
        "src/Main.nash"
    ));
}

#[test]
fn module_name_mismatch() {
    let input = "module Other exposing (..)";
    let error = Error::ModuleNameMismatch {
        expected: "Main",
        actual: "Other",
        row: 1,
        col: 8,
    };
    insta::assert_snapshot!(render_plain(
        &to_report(&Source::new(input), &error),
        &Source::new(input),
        "src/Main.nash"
    ));
}

macro_rules! report_branch {
    ($name:ident, $input:expr, $source:ident, $report:expr) => {
        #[test]
        fn $name() {
            let $source = Source::new($input);
            let report = $report;
            insta::with_settings!({ description => $input, omit_expression => true }, {
                insta::assert_snapshot!(render_plain(&report, &$source, "src/Main.nash"));
            });
        }
    };
}
use nash_parse::error::{
    CustomType, DeclDef, Exposing, Module, PList, PRecord, PTuple, Pattern, TRecord, TTuple, Type,
    TypeAlias,
};
report_branch!(
    weird_end_semicolon,
    ";",
    s,
    module::to_weird_end_report(&s, 1, 1)
);
report_branch!(
    weird_end_comma,
    ",",
    s,
    module::to_weird_end_report(&s, 1, 1)
);
report_branch!(
    weird_end_backtick,
    "`",
    s,
    module::to_weird_end_report(&s, 1, 1)
);
report_branch!(
    weird_end_uppercase,
    "Thing",
    s,
    module::to_weird_end_report(&s, 1, 1)
);
report_branch!(
    weird_end_lowercase,
    "thing",
    s,
    module::to_weird_end_report(&s, 1, 1)
);
report_branch!(
    weird_end_empty,
    "",
    s,
    module::to_weird_end_report(&s, 1, 1)
);
report_branch!(
    decl_start_uppercase,
    "Thing",
    s,
    decl::to_decl_start_report(&s, 1, 1)
);
report_branch!(
    decl_start_import,
    "import",
    s,
    decl::to_decl_start_report(&s, 1, 1)
);
report_branch!(
    decl_start_case,
    "case",
    s,
    decl::to_decl_start_report(&s, 1, 1)
);
report_branch!(decl_start_if, "if", s, decl::to_decl_start_report(&s, 1, 1));
report_branch!(
    fresh_line_keyword,
    "module",
    s,
    module::to_parse_error_report(&s, &Module::FreshLine(1, 1))
);
report_branch!(
    exposing_reserved_word,
    "if",
    s,
    module::to_exposing_report(&s, &Exposing::Value(1, 1), 1, 1)
);
report_branch!(
    exposing_bare_operator,
    "+",
    s,
    module::to_exposing_report(&s, &Exposing::Value(1, 1), 1, 1)
);
report_branch!(
    alias_reserved_parameter,
    "if",
    s,
    decl::to_type_alias_report(&s, &TypeAlias::Equals(1, 1), 1, 1)
);
report_branch!(
    custom_type_reserved_parameter,
    "if",
    s,
    decl::to_custom_type_report(&s, &CustomType::Equals(1, 1), 1, 1)
);
report_branch!(
    definition_reserved_argument,
    "if",
    s,
    decl::to_decl_def_report(&s, "f", &DeclDef::Equals(1, 1), 1, 1)
);
report_branch!(
    definition_missing_colon,
    "->",
    s,
    decl::to_decl_def_report(&s, "f", &DeclDef::Equals(1, 1), 1, 1)
);
report_branch!(
    definition_unexpected_operator,
    "+",
    s,
    decl::to_decl_def_report(&s, "f", &DeclDef::Equals(1, 1), 1, 1)
);
report_branch!(
    pattern_start_in_case,
    "if",
    s,
    pattern::to_pattern_report(&s, pattern::PContext::Case, &Pattern::Start(1, 1), 1, 1)
);
report_branch!(
    pattern_start_in_arg,
    "if",
    s,
    pattern::to_pattern_report(&s, pattern::PContext::Arg, &Pattern::Start(1, 1), 1, 1)
);
report_branch!(
    pattern_start_in_let,
    "if",
    s,
    pattern::to_pattern_report(&s, pattern::PContext::Let, &Pattern::Start(1, 1), 1, 1)
);
report_branch!(
    pattern_negative_number,
    "-1",
    s,
    pattern::to_pattern_report(&s, pattern::PContext::Arg, &Pattern::Start(1, 1), 1, 1)
);
report_branch!(
    pattern_reserved_record_field,
    "if",
    s,
    pattern::to_p_record_report(&s, &PRecord::Field(1, 1), 1, 1)
);
report_branch!(
    pattern_reserved_tuple_open,
    "if",
    s,
    pattern::to_p_tuple_report(&s, pattern::PContext::Arg, &PTuple::Open(1, 1), 1, 1)
);
report_branch!(
    pattern_reserved_tuple_end,
    "if",
    s,
    pattern::to_p_tuple_report(&s, pattern::PContext::Arg, &PTuple::End(1, 1), 1, 1)
);
report_branch!(
    pattern_stray_bracket,
    "]",
    s,
    pattern::to_p_tuple_report(&s, pattern::PContext::Arg, &PTuple::End(1, 1), 1, 1)
);
report_branch!(
    pattern_reserved_list_open,
    "if",
    s,
    pattern::to_p_list_report(&s, pattern::PContext::Arg, &PList::Open(1, 1), 1, 1)
);
report_branch!(
    pattern_underscore_only_name,
    "___",
    s,
    pattern::to_pattern_report(
        &s,
        pattern::PContext::Arg,
        &Pattern::WildcardNotVar("___", 3, 1, 1),
        1,
        1
    )
);
report_branch!(
    pattern_underscore_uppercase_name,
    "_Thing",
    s,
    pattern::to_pattern_report(
        &s,
        pattern::PContext::Arg,
        &Pattern::WildcardNotVar("_Thing", 6, 1, 1),
        1,
        1
    )
);
report_branch!(
    type_reserved_word,
    "if",
    s,
    type_::to_type_report(
        &s,
        type_::TContext::Annotation("f"),
        &Type::Start(1, 1),
        1,
        1
    )
);
report_branch!(
    type_start_in_custom_type,
    "42",
    s,
    type_::to_type_report(&s, type_::TContext::CustomType, &Type::Start(1, 1), 1, 1)
);
report_branch!(
    type_start_in_alias,
    "42",
    s,
    type_::to_type_report(&s, type_::TContext::TypeAlias, &Type::Start(1, 1), 1, 1)
);
report_branch!(
    type_indent_in_custom_type,
    "42",
    s,
    type_::to_type_report(
        &s,
        type_::TContext::CustomType,
        &Type::IndentStart(1, 1),
        1,
        1
    )
);
report_branch!(
    type_indent_in_alias,
    "42",
    s,
    type_::to_type_report(
        &s,
        type_::TContext::TypeAlias,
        &Type::IndentStart(1, 1),
        1,
        1
    )
);
report_branch!(
    type_record_reserved_open,
    "if",
    s,
    type_::to_t_record_report(
        &s,
        type_::TContext::Annotation("f"),
        &TRecord::Open(1, 1),
        1,
        1
    )
);
report_branch!(
    type_record_reserved_field,
    "if",
    s,
    type_::to_t_record_report(
        &s,
        type_::TContext::Annotation("f"),
        &TRecord::Field(1, 1),
        1,
        1
    )
);
report_branch!(
    type_record_double_comma,
    ",",
    s,
    type_::to_t_record_report(
        &s,
        type_::TContext::Annotation("f"),
        &TRecord::Field(1, 1),
        1,
        1
    )
);
report_branch!(
    type_record_trailing_comma,
    "}",
    s,
    type_::to_t_record_report(
        &s,
        type_::TContext::Annotation("f"),
        &TRecord::Field(1, 1),
        1,
        1
    )
);
report_branch!(
    type_record_underindented_close,
    "f : { x : int\n}",
    s,
    type_::to_t_record_report(
        &s,
        type_::TContext::Annotation("f"),
        &TRecord::IndentEnd(1, 14),
        1,
        5
    )
);
report_branch!(
    type_tuple_reserved_open,
    "if",
    s,
    type_::to_t_tuple_report(
        &s,
        type_::TContext::Annotation("f"),
        &TTuple::Open(1, 1),
        1,
        1
    )
);
