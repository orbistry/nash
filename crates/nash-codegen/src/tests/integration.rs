use std::collections::BTreeMap;

use super::*;
use crate::build::{Input, TraceLevel};
use nash_ast::primitives;
use nash_config::PlutusVersion;
use nash_plutus::machine::PlutusVersion as MachineVersion;
use nash_test::{eval, prng::Prng};

fn compile(
    snapshot_name: &str,
    source: &str,
    version: PlutusVersion,
    trace: TraceLevel,
) -> Result<Vec<TestProgram>, String> {
    compile_selected(snapshot_name, source, version, trace, None)
}

fn compile_selected(
    snapshot_name: &str,
    source: &str,
    version: PlutusVersion,
    trace: TraceLevel,
    name: Option<&str>,
) -> Result<Vec<TestProgram>, String> {
    let arena = Arena::new();
    let bump = arena.as_bump();
    let mut interfaces = BTreeMap::from([("Builtin", nash_can::kinds::builtin_interface(bump))]);
    let mut modules = Vec::new();
    for (text, package) in [
        (
            include_str!("../../tests/fixtures/VestingLiteral.nash"),
            Some(primitives::BASE),
        ),
        (
            include_str!("../../../nash-driver/base/src/Show.nash"),
            Some(primitives::BASE),
        ),
        (
            include_str!("../../../nash-driver/base/src/Lift.nash"),
            Some(primitives::BASE),
        ),
        (
            include_str!("../../../nash-driver/base/src/Bool.nash"),
            Some(primitives::BASE),
        ),
        (
            "module Logic exposing ((&&), (||), (==))\nimport Bool exposing (and, or)\nimport Builtin\ninfix right 2 (||) = or\ninfix right 3 (&&) = and\ninfix non 4 (==) = equal\nequal : int -> int -> bool\nequal = Builtin.equalsInteger\n",
            Some(primitives::BASE),
        ),
        (
            "module Option exposing (type option(..))\ntype option 'a = Some 'a | None\n",
            Some(primitives::BASE),
        ),
        (
            indoc::indoc!(
                r#"
            module Prop exposing (type prng(..), type generator, constant, reject)
            import Primitive exposing (..)
            import Builtin exposing (..)
            import Option exposing (type option(..))
            type prng = Seeded bytes (list int) | Replayed (list int)
            type alias generator 'a = prng -> option ('a, prng)
            constant : 'a -> generator 'a
            constant value prng = Some (value, prng)
            reject : generator 'a
            reject _ = None
        "#
            ),
            Some(primitives::BASE),
        ),
        (
            include_str!("../../../nash-driver/base/src/Test.nash"),
            Some(primitives::BASE),
        ),
        (source, None),
    ] {
        let text = bump.alloc_str(text);
        let parsed = nash_parse::Parser::new(bump, text)
            .module()
            .map_err(|e| format!("parse: {e:?}\n{text}"))?;
        let canonical = nash_can::canonicalize(
            bump,
            nash_can::Context {
                package,
                interfaces: Some(&interfaces),
            },
            &parsed,
        )
        .map_err(|e| format!("canonicalize: {e:?}\n{text}"))?;
        let (annotations, solved) = nash_solve::run(
            bump,
            &mut nash_constrain::UnionFind::new(),
            &canonical.module,
            &canonical.tables,
        )
        .map_err(|e| format!("solve: {e:?}\n{text}"))?;
        interfaces.insert(
            canonical.module.name.name,
            nash_can::from_module(bump, &canonical.module, &annotations),
        );
        modules.push((canonical, solved));
    }
    let build = Build::new(modules.iter().map(|(module, solved)| Input {
        module: &module.module,
        types: solved,
        tables: &module.tables,
    }));
    let programs = compile_tests_matching(
        &arena,
        &build,
        modules.last().unwrap().0.module.name,
        source,
        Path::new("Main.nash"),
        version,
        TraceConfig {
            user: trace,
            compiler: false,
        },
        |test| name.is_none_or(|name| name == test.name.value),
    )
    .map_err(|e| e.to_string())?;
    let render = |bytes: &[u8]| {
        let program: &nash_plutus::program::Program<'_, nash_plutus::binder::DeBruijn> =
            flat::decode(&arena, bytes).unwrap();
        nash_plutus::pretty::program(program)
    };
    let mut output = String::new();
    for program in &programs {
        output.push_str(&format!("--- {}\n", program.name));
        match &program.programs {
            Programs::Unit { run } => output.push_str(&render(run)),
            Programs::Prop { prepare } => {
                output.push_str(&format!("--- prepare\n{}", render(prepare)))
            }
        }
        output.push('\n');
    }
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(format!("{snapshot_name}_{version:?}_{trace:?}"), output);
    });
    Ok(programs)
}

fn unit(program: &TestProgram) -> (bool, Vec<String>) {
    let Programs::Unit { run } = &program.programs else {
        panic!("unit program");
    };
    let arena = Arena::new();
    let result = eval::evaluate(&arena, MachineVersion::V3, run, None);
    (result.term.is_ok(), result.logs)
}

#[test]
fn unit_roots_and_power_assert_payloads() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Literal
        privateValue : int
        privateValue = 1
        tests
            test "passes" = do
                assert True
            test "fails without captures" = do
                assert False
            test "captures private value" = do
                assert (Builtin.equalsInteger privateValue 2)
    "#
    );
    let programs = compile(
        "unit_roots_and_power_assert_payloads_1",
        source,
        PlutusVersion::V3,
        TraceLevel::Silent,
    )
    .unwrap();
    assert_eq!(unit(&programs[0]), (true, vec![]));
    assert_eq!(unit(&programs[1]), (false, vec!["\0assert\x000".into()]));
    let (passed, logs) = unit(&programs[2]);
    assert!(!passed);
    assert_eq!(logs, ["\0assert\x000", "\0assert\x000\x001\x001"]);
    assert_eq!(programs[2].asserts[0].captures.len(), 2);
    assert!(!programs[2].asserts[0].captures[0].shown);
    assert!(programs[2].asserts[0].captures[1].shown);
    insta::with_settings!({description => source, omit_expression => true}, {
        insta::assert_snapshot!(format!("{:#?}\n{logs:?}", programs[2].asserts));
    });
}

#[test]
fn captures_preserve_partial_application_order_and_lazy_branches() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Literal
        import Logic exposing (..)
        compare : int -> int -> bool
        compare x = trace "partial" (\y -> Builtin.equalsInteger x y)
        one : int
        one = 1
        badBool : unit -> bool
        badBool _ = fail "unselected"
        ordinary : unit
        ordinary = if True || (badBool ()) then () else (fail "wrong")
        tests
            test "ordered" = do
                assert (compare (trace "left" one) (trace "right" 2))
            test "lazy" = do
                assert (if False then (fail "unselected") else True)
            test "lazy or" = do
                assert (True || (badBool ()))
            test "lazy and" = do
                assert (if False && (badBool ()) then False else True)
            test "ordinary lowering is lazy" = do
                ordinary
    "#
    );
    let programs = compile(
        "captures_preserve_partial_application_order_and_lazy_branches_1",
        source,
        PlutusVersion::V3,
        TraceLevel::Verbose,
    )
    .unwrap();
    let (passed, logs) = unit(&programs[0]);
    assert!(!passed);
    assert_eq!(&logs[..3], ["left", "partial", "right"]);
    assert_eq!(logs.last().unwrap(), "\0assert\x000\x001\x001");
    assert_eq!(unit(&programs[1]), (true, vec![]));
    assert!(programs[1].asserts[0].captures.is_empty());
    assert_eq!(unit(&programs[2]), (true, vec![]));
    assert_eq!(unit(&programs[3]), (true, vec![]));
    assert_eq!(unit(&programs[4]), (true, vec![]));
}

#[test]
fn custom_show_runs_only_on_failure_and_test_local_native_layouts_specialize() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Literal
        import Show exposing (Show)
        type boxed = Box int
        impl Show boxed where
            show = trace "show init" (\_ -> trace "show" "Box")
        isZero : boxed -> bool
        isZero (Box value) = Builtin.equalsInteger value 0
        ordinary : unit
        ordinary = assert False
        tests
            test "passing show is lazy" = do
                value <- trace "value" (Box 0)
                assert (isZero value)
            test "failing show executes" = do
                value <- trace "value" (Box 1)
                assert (isZero value)
            test "native local specialization" = do
                values <- let singleton x = [x] in singleton 1
                assert (Builtin.equalsInteger (Builtin.headList values) 1)
            test "ordinary assertion unchanged" = do
                ordinary
    "#
    );
    let programs = compile(
        "custom_show_runs_only_on_failure_and_test_local_native_layouts_specialize_1",
        source,
        PlutusVersion::V3,
        TraceLevel::Verbose,
    )
    .unwrap();
    assert_eq!(unit(&programs[0]), (true, vec!["value".into()]));
    assert_eq!(
        unit(&programs[1]),
        (
            false,
            vec![
                "value".into(),
                "\0assert\x000".into(),
                "show init".into(),
                "show".into(),
                "\0assert\x000\x001\0Box".into()
            ]
        )
    );
    assert_eq!(unit(&programs[2]), (true, vec![]));
    assert!(programs[3].asserts.is_empty());
    assert_eq!(unit(&programs[3]), (false, vec!["assertion failed".into()]));
}

#[test]
fn assertion_json_preserves_nested_call_delimiters_and_string_parentheses() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Literal
        import Logic exposing ((==))
        one : int
        one = 1
        tests
            test "nested infix call" = do
                assert (one == Builtin.addInteger one (Builtin.addInteger one one))
            test "parentheses inside strings" = do
                assert (Builtin.equalsString "(" (Builtin.appendString ")" ("(")))
            test "grouped operands" = do
                assert ((Builtin.addInteger one one) == (Builtin.addInteger one (Builtin.addInteger one one)))
            test "grouped callee" = do
                assert ((\x -> x) False)
    "#
    );
    let programs = compile(
        "assertion_json_preserves_nested_call_delimiters_and_string_parentheses_1",
        source,
        PlutusVersion::V3,
        TraceLevel::Silent,
    )
    .unwrap();
    let outcomes = nash_test::run_all(programs, &nash_test::Config::default());
    let report: serde_json::Value =
        serde_json::from_str(&nash_test::report::json::render(0, 100, &outcomes)).unwrap();
    assert_eq!(
        report["tests"][0]["assert"]["source"],
        "one == Builtin.addInteger one (Builtin.addInteger one one)"
    );
    assert_eq!(
        report["tests"][1]["assert"]["source"],
        r#"Builtin.equalsString "(" (Builtin.appendString ")" ("("))"#
    );
    assert_eq!(
        report["tests"][2]["assert"]["source"],
        "(Builtin.addInteger one one) == (Builtin.addInteger one (Builtin.addInteger one one))"
    );
    assert_eq!(report["tests"][3]["assert"]["source"], r#"(\x -> x) False"#);
}

#[test]
fn properties_thread_prng_bind_patterns_and_draw_without_running_body() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Literal
        import Prop exposing (constant)
        tests
            prop "patterns" =
                let
                    x via trace "first" (constant 7)
                    (y, z) via trace "second" (constant (8, True))
                in
                do
                    trace "body" ()
                    assert (if z then Builtin.equalsInteger y (Builtin.addInteger x 1) else False)
    "#
    );
    let programs = compile(
        "properties_thread_prng_bind_patterns_and_draw_without_running_body_1",
        source,
        PlutusVersion::V3,
        TraceLevel::Verbose,
    )
    .unwrap();
    assert_eq!(programs[0].binder_texts, ["x", "(y, z)"]);
    let Programs::Prop { prepare } = &programs[0].programs else {
        panic!("property program");
    };
    let arena = Arena::new();
    let prepared = eval::prepare(
        &arena,
        MachineVersion::V3,
        prepare,
        &Prng::from_choices(&[]),
        nash_plutus::machine::ExBudget::max(),
    )
    .unwrap()
    .unwrap();
    let shown = eval::show_prepared(
        &arena,
        MachineVersion::V3,
        &prepared,
        nash_plutus::machine::ExBudget::max(),
    )
    .unwrap();
    assert_eq!(shown, ["7", "?"]);
    let result = eval::run_prepared(
        &arena,
        MachineVersion::V3,
        &prepared,
        nash_plutus::machine::ExBudget::max(),
    );
    assert!(result.term.is_ok(), "{:?}", result.term);
    assert_eq!(result.logs, ["first", "second", "body"]);
}

#[test]
fn rejected_generator_skips_body_and_selected_target_is_enforced() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Primitive exposing (..)
        import Builtin exposing (..)
        import Prop exposing (reject)
        tests
            test "plain" = do
                assert True
            prop "invalid" =
                let x via reject in
                do
                    assert (Builtin.equalsInteger x x)
    "#
    );
    let programs = compile(
        "rejected_generator_skips_body_and_selected_target_is_enforced_3",
        source,
        PlutusVersion::V3,
        TraceLevel::Silent,
    )
    .unwrap();
    let Programs::Prop { prepare } = &programs[1].programs else {
        panic!("property program");
    };
    let arena = Arena::new();
    assert!(
        eval::prepare(
            &arena,
            MachineVersion::V3,
            prepare,
            &Prng::from_choices(&[]),
            nash_plutus::machine::ExBudget::max()
        )
        .unwrap()
        .is_none()
    );
    for version in [PlutusVersion::V1, PlutusVersion::V2] {
        let all = compile(
            "rejected_generator_skips_body_and_selected_target_is_enforced_2",
            source,
            version,
            TraceLevel::Silent,
        )
        .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].plutus_version, version);
        assert_eq!(all[1].plutus_version, version);
        let selected = compile_selected(
            "rejected_generator_skips_body_and_selected_target_is_enforced_1",
            source,
            version,
            TraceLevel::Silent,
            Some("plain"),
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(unit(&selected[0]), (true, vec![]));
    }
}

#[test]
fn rejected_generator_does_not_initialize_later_generators() {
    let source = indoc::indoc!(
        r#"
        module Main exposing (..)
        import Builtin
        import Prop exposing (reject)
        tests
            prop "reject before later generator" =
                let
                    x via trace "first" reject
                    y via fail
                in
                do
                    assert (Builtin.equalsInteger x y)
    "#
    );
    let programs = compile(
        "rejected_generator_does_not_initialize_later_generators",
        source,
        PlutusVersion::V3,
        TraceLevel::Verbose,
    )
    .unwrap();
    let Programs::Prop { prepare } = &programs[0].programs else {
        panic!("property");
    };
    let arena = Arena::new();
    let evaluated = eval::evaluate(
        &arena,
        MachineVersion::V3,
        prepare,
        Some(Prng::from_choices(&[]).to_term(&arena)),
    );
    assert!(matches!(
        evaluated.term.unwrap(),
        nash_plutus::term::Term::Constr { tag: 1, fields: [] }
    ));
    assert_eq!(evaluated.logs, ["first"]);
}
