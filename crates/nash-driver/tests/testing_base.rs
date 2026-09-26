//! Execute the real core testing helpers through production code generation.
mod support;

use nash_plutus::{arena::Arena, syn, term::Term};

async fn compile(body: &str) -> nash_driver::build::ValidatorOutput {
    support::compile_validator(&format!("validator module TestingCore exposing (main)\nimport Primitive exposing (type bool(..))\nimport Builtin\nimport Prelude exposing (..)\nimport Literal\nimport Num exposing (Num)\nimport Lift exposing (Lift)\nimport Prop exposing (type prng(..))\nimport Option exposing (type option(..))\nimport Test\nimport Cons\nimport List\none value = Cons.Cons value Cons.Nil\ntwo a b = Cons.Cons a (one b)\ng = Prop.Group\nc = Prop.Choice\nreplay : Cons.cons Prop.choiceTree -> prng\nreplay values = Replayed values Cons.Nil\nmain : Data -> unit\nmain _ =\n{body}\n")).await
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
        ("Prop.int", "two (c 1) (c 65535)", "32767"),
        ("Prop.int", "two (c 1) (c 0)", "(-32768)"),
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

async fn measure_budget(name: &str, source: &str) -> String {
    let output = compile(source).await;
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
    assert!(result.term.is_ok(), "{name}: {:?}", result.term);
    let budget = result.info.consumed_budget;
    format!("{name}: cpu={} memory={}\n", budget.cpu, budget.mem)
}

#[tokio::test]
async fn integer_predicate_costs() {
    let mut report = String::new();
    let mut sources = Vec::new();
    for (name, computed, predicate, expected) in [
        (
            "gcd_valid",
            "Int.gcd 48 18 == 6",
            "Int.isGcd 48 18 6",
            "True",
        ),
        (
            "gcd_coprime",
            "Int.gcd 6765 4181 == 1",
            "Int.isGcd 6765 4181 1",
            "True",
        ),
        (
            "gcd_reject",
            "Int.gcd 6765 4181 == 5000",
            "Int.isGcd 6765 4181 5000",
            "False",
        ),
        (
            "lcm_valid",
            "Int.lcm 48 18 == 144",
            "Int.isLcm 48 18 144",
            "True",
        ),
        (
            "lcm_reject",
            "Int.lcm 48 18 == 143",
            "Int.isLcm 48 18 143",
            "False",
        ),
        (
            "lcm_multiple",
            "Int.lcm 48 18 == 288",
            "Int.isLcm 48 18 288",
            "False",
        ),
        ("sqrt_small", "Int.isqrt 48 == 6", "Int.isSqrt 48 6", "True"),
        (
            "sqrt_large",
            "Int.isqrt (Int.fromBytes True #\"0100000000000000020000000000000001\") == 18446744073709551617",
            "Int.isSqrt (Int.fromBytes True #\"0100000000000000020000000000000001\") 18446744073709551617",
            "True",
        ),
    ] {
        for (kind, expression) in [("compute", computed), ("predicate", predicate)] {
            let source = format!("    assert (({expression}) == {expected})");
            report.push_str(&measure_budget(&format!("{name} {kind}"), &source).await);
            sources.push(source);
        }
    }
    insta::with_settings!({description => sources.join("\n"), omit_expression => true}, {
        insta::assert_snapshot!(report);
    });
}
