//! Execute the real core testing helpers through production code generation.
use std::sync::Arc;

use nash_driver::{Database, InMemorySource, build_graph, build_with};
use nash_plutus::{arena::Arena, syn, term::Term};
use tokio::sync::Mutex;
use url::Url;

async fn compile(body: &str) -> nash_driver::build::ValidatorOutput {
    let memory = InMemorySource::new();
    let mut origins = nash_driver::bundled_base::modules();
    let uri = Url::parse("file:///project/src/TestingCore.nash").unwrap();
    memory.insert(uri.clone(), format!("validator module TestingCore exposing (main)\nimport Primitive exposing (type bool(..))\nimport Builtin\nimport Prelude exposing (..)\nimport Literal\nimport Num exposing (Num)\nimport Lift exposing (Lift)\nimport Prop exposing (type prng(..))\nimport Option exposing (type option(..))\nimport Test\nimport Cons\nimport List\none value = Cons.Cons value Cons.Nil\ntwo a b = Cons.Cons a (one b)\ng = Prop.Group\nc = Prop.Choice\nreplay : Cons.cons Prop.choiceTree -> prng\nreplay values = Replayed values Cons.Nil\nmain : Data -> unit\nmain _ =\n{body}\n"));
    origins.insert(uri, None);
    let db = Arc::new(Mutex::new(Database::new(memory)));
    let graph = build_graph(db.clone(), &origins.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, result) = build_with(db, &graph, &origins, |solved| {
        nash_driver::build::build_validators(solved, nash_config::Build::default())
    })
    .await;
    assert!(report.is_success(), "{report:#?}");
    result.unwrap().unwrap().remove(0)
}

#[tokio::test]
async fn choice_seeded_and_replayed_draws_agree() {
    let output = compile(r##"    let
        initial = Seeded #"0000000000000000000000000000000000000000000000000000000000000000" Cons.Nil
    in
    case (Prop.choice 100) initial of
        Some (n, Seeded seed choices) ->
            case (Prop.choice 100) (Replayed choices Cons.Nil) of
                Some (replayed, Replayed rest _) ->
                    assert (n == replayed)
                _ -> (fail "replay rejected seeded choice")
        _ -> (fail "seeded draw failed")"##).await;
    let arena = Arena::new();
    let program = syn::parse_program(&arena, &output.uplc).unwrap();
    let result = program
        .apply(
            &arena,
            Term::data(
                &arena,
                nash_plutus::data::PlutusData::integer_from(&arena, 0),
            ),
        )
        .eval(&arena);
    assert!(result.term.is_ok(), "{:?}", result.term);
}

#[tokio::test]
async fn malformed_replayed_choices_are_rejected() {
    for choices in ["Cons.Nil", "one (c (-1))", "one (c 11)"] {
        let output = compile(&format!(
            r#"    let
        values = {choices}
    in
    case (Prop.choice 10) (replay values) of
        None -> ()
        Some _ -> (fail "invalid replay accepted")"#
        ))
        .await;
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &output.uplc).unwrap();
        let result = program
            .apply(
                &arena,
                Term::data(
                    &arena,
                    nash_plutus::data::PlutusData::integer_from(&arena, 0),
                ),
            )
            .eval(&arena);
        assert!(result.term.is_ok(), "choices={choices}: {:?}", result.term);
    }
}

#[tokio::test]
async fn generation_functions_thread_choices() {
    for (generator, choices, expected) in [
        (
            "Prop.tuple2 (Prop.choice 10) (Prop.choice 10)",
            "two (g (one (c 4))) (g (one (c 5)))",
            "(4, 5)",
        ),
        (
            "Prop.listBetween 1 2 (Prop.choice 10)",
            "two (g (one (g (one (c 3))))) (g (one (c 0)))",
            "[3]",
        ),
        (
            "Prop.bytes",
            "two (g (two (c 1) (g (one (c 65))))) (g (one (c 0)))",
            "#\"41\"",
        ),
        ("Prop.int", "two (c 0) (c 42)", "42"),
        (
            "Prop.int",
            "two (c 2) (c 18446744073709551615)",
            "9223372036854775807",
        ),
        ("Prop.int", "two (c 2) (c 0)", "(-9223372036854775808)"),
        (
            "Prop.oneOf (Cons.Cons (Prop.constant 0) (Cons.Cons (Prop.choice 10) Cons.Nil))",
            "two (c 1) (g (one (c 7)))",
            "7",
        ),
        ("Prop.intBetween 3 3", "Cons.Nil", "3"),
    ] {
        let output = compile(&format!(
            r#"    case ({generator}) (replay ({choices})) of
        Some (value, _) -> assert (value == {expected})
        None -> (fail "generator exhausted replay")"#
        ))
        .await;
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &output.uplc).unwrap();
        let result = program
            .apply(
                &arena,
                Term::data(
                    &arena,
                    nash_plutus::data::PlutusData::integer_from(&arena, 0),
                ),
            )
            .eval(&arena);
        assert!(result.term.is_ok(), "{generator}: {:?}", result.term);
    }
}

#[tokio::test]
async fn invalid_choice_bounds_fail() {
    for bound in ["-1", "18446744073709551616"] {
        let output = compile(&format!(
            r#"    case (Prop.choice ({bound})) (replay (one (c 0))) of
        Some _ -> ()
        None -> ()"#
        ))
        .await;
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &output.uplc).unwrap();
        let result = program
            .apply(
                &arena,
                Term::data(
                    &arena,
                    nash_plutus::data::PlutusData::integer_from(&arena, 0),
                ),
            )
            .eval(&arena);
        assert!(result.term.is_err(), "invalid bound {bound} accepted");
    }
}

#[tokio::test]
async fn test_protocol_logs_survive_silent_user_traces() {
    for (body, expected, succeeds) in [
        (
            "    Test.label \"coverage\"",
            vec!["\0label\0coverage"],
            true,
        ),
        (
            r#"    Test.assertAt "\u{0000}assert\u{0000}0" (\() -> Test.assertCapture "\u{0000}assert\u{0000}0\u{0000}0\u{0000}" "1" (\() -> fail))"#,
            vec!["\u{0}assert\u{0}0", "\u{0}assert\u{0}0\u{0}0\u{0}1"],
            false,
        ),
    ] {
        let output = compile(body).await;
        let arena = Arena::new();
        let program = syn::parse_program(&arena, &output.uplc).unwrap();
        let result = program
            .apply(
                &arena,
                Term::data(
                    &arena,
                    nash_plutus::data::PlutusData::integer_from(&arena, 0),
                ),
            )
            .eval(&arena);
        assert_eq!(result.term.is_ok(), succeeds);
        assert_eq!(result.info.logs, expected);
    }
}
