//! Pattern coverage errors produced by the real pipeline, rendered by `nash-report`.

use nash_report::{Report, Source, render_plain};

fn reports(input: &str) -> Vec<Report> {
    let bump = bumpalo::Bump::new();
    let text = bump.alloc_str(input);
    let module = nash_parse::Parser::new(&bump, text)
        .module()
        .expect("parse");
    let interfaces =
        std::collections::BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(&bump))]);
    let can = nash_can::canonicalize(
        &bump,
        nash_can::Context {
            package: None,
            interfaces: Some(&interfaces),
        },
        &module,
    )
    .expect("canonicalize");
    let mut uf = nash_constrain::UnionFind::new();
    let module = &can.module;
    nash_solve::run(&bump, &mut uf, module, &can.tables).expect("solve before checking coverage");
    nash_nitpick::check(&bump, &can.module)
        .expect_err("expected pattern errors")
        .iter()
        .map(nash_report::pattern::to_report)
        .collect()
}
fn render(input: &str) -> String {
    let reports = reports(input);
    assert_eq!(reports.len(), 1);
    render_plain(&reports[0], &Source::new(input), "src/Main.nash")
}
macro_rules! assert_pattern_snapshot {
    ($input:expr) => {{
        let input = $input;
        insta::with_settings!({ description => format!("Code:\n\n{input}"), omit_expression => true }, {
            insta::assert_snapshot!(render(input));
        });
    }};
}
#[test]
fn missing_patterns_data() {
    assert_pattern_snapshot!(indoc::indoc! {r#"
        module Main exposing (..)
        import Builtin exposing (Data(..))
        tag d =
            case d of
                Constr _ _ -> ()
                List _ -> ()
    "#});
}
#[test]
fn missing_patterns_nested_list() {
    assert_pattern_snapshot!(indoc::indoc! {r#"
        module Main exposing (..)
        first xs =
            case xs of
                [] -> 0
                [] :: _ -> 1
    "#});
}
#[test]
fn unsafe_arg() {
    assert_pattern_snapshot!("module Main exposing (..)\nf [x] = x\n");
}
#[test]
fn unsafe_destruct() {
    assert_pattern_snapshot!(indoc::indoc! {r#"
        module Main exposing (..)
        f xs =
            let
                [x] = xs
            in
            x
    "#});
}
#[test]
fn redundant_pattern() {
    let input = indoc::indoc! {r#"
        module Main exposing (..)
        f xs =
            case xs of
                _ -> 0
                [] -> 1
    "#};
    let reports = reports(input);
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(report.region.start.line, 5);
    assert!(report.context.is_some_and(|region| region.start.line == 3));
    insta::with_settings!({ description => format!("Code:\n\n{input}"), omit_expression => true }, {
        insta::assert_snapshot!(render_plain(report, &Source::new(input), "src/Main.nash"));
    });
}
