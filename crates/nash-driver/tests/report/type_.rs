//! Type errors produced by the real solver, rendered by `nash-report`.

fn source_settings(source: &str) -> insta::Settings {
    let bump = bumpalo::Bump::new();
    nash_parse::Parser::new(&bump, source)
        .module()
        .expect("renderer fixture must be valid Nash syntax");
    let mut settings = insta::Settings::clone_current();
    settings.set_description(source);
    settings.set_omit_expression(true);
    settings
}

fn type_error_reports(input: &str) -> String {
    let bump = bumpalo::Bump::new();
    let source = bump.alloc_str(input);
    let module = nash_parse::Parser::new(&bump, source)
        .module()
        .expect("parse fixture");
    let localizer = nash_report::Localizer::from_module(&module, &[]);
    let canonical = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
        .expect("canonicalize fixture");
    let mut uf = nash_constrain::UnionFind::new();
    let module = &canonical.module;
    let errors = nash_solve::run(&bump, &mut uf, module, &canonical.tables)
        .expect_err("fixture must type-fail");
    assert!(!errors.is_empty());
    errors
        .iter()
        .map(|error| {
            nash_report::render_plain(
                &nash_report::type_::to_report(&localizer, error),
                &nash_report::Source::new(source),
                "Main.nash",
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn source_pipeline_annotation_body() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf : A\nf = B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_call_argument() {
    let source =
        "module Main exposing (..)\ntype A = A\ntype B = B\nf : A -> A\nf x = x\ng = f B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_if_branches() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf condition = if condition then A else B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_case_branches() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf a =\n    case a of\n        A -> A\n        _ -> B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_list_entries() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf = [A, B]\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}

#[test]
fn source_pipeline_if_condition() {
    let source = "module Main exposing (..)\ntype A = A\nf = if A then A else A\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_call_second_argument() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf : A -> A -> A\nf x y = x\ng = f A B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_pattern_typed_arg() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf : B -> B\nf A = B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_pattern_ctor_arg() {
    let source =
        "module Main exposing (..)\ntype A = A\ntype B = B\ntype Box = Box A\nf (Box B) = A\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_record_update_type() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\ntype alias Person = { age : A }\nf : Person -> Person\nf p = { p | age = B }\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_record_access() {
    let source = "module Main exposing (..)\ntype A = A\ntype alias Person = { age : A }\nf : Person -> A\nf p = p.aeg\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_infinite_type() {
    let source = "module Main exposing (..)\nf x = x x\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_missing_impl() {
    let source = "module Main exposing (..)\ntype A = A\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nf : A -> Builtin.bool\nf a = eq a a\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_missing_constraint() {
    let source = "module Main exposing (..)\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nf : 'a -> Builtin.bool\nf a = eq a a\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
