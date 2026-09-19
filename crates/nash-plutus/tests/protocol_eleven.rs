use nash_plutus::{arena::Arena, machine::PlutusVersion, pretty, syn};

#[test]
fn constant_cases_select_and_apply_only_the_chosen_branch() {
    for (source, expected) in [
        (
            "(case (con bool False) (con integer 1) (error))",
            "(con integer 1)",
        ),
        (
            "(case (con bool True) (error) (con integer 2))",
            "(con integer 2)",
        ),
        ("(case (con unit ()) (con integer 3))", "(con integer 3)"),
        (
            "(case (con integer 1) (error) (con integer 4))",
            "(con integer 4)",
        ),
        (
            "(case (con (list integer) []) (error) (con integer 5))",
            "(con integer 5)",
        ),
        (
            "(case (con (list integer) [6, 7]) (lam head (lam tail head)) (error))",
            "(con integer 6)",
        ),
        (
            "(case (con (pair integer integer) (7, 8)) (lam first (lam second second)))",
            "(con integer 8)",
        ),
    ] {
        for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
            let arena = Arena::new();
            let source = format!("(program 1.1.0 {source})");
            let program = syn::parse_program(&arena, &source).into_result().unwrap();
            let result = program.eval_version(&arena, version);
            assert_eq!(pretty::term(result.term.unwrap()), expected);
            assert!(result.info.consumed_budget.cpu < 1_000_000);
        }
    }
}

#[test]
fn constant_cases_reject_missing_and_excess_branches() {
    for source in [
        "(case (con bool True) (con unit ()))",
        "(case (con bool False) (con unit ()) (con unit ()) (con unit ()))",
        "(case (con unit ()) (con unit ()) (con unit ()))",
        "(case (con integer -1) (con unit ()))",
        "(case (con (list integer) []) (con unit ()))",
        "(case (con (list integer) []) (error) (con unit ()) (con unit ()))",
        "(case (con (list integer) [1]) (lam h (lam t h)) (error) (error))",
        "(case (con (pair integer integer) (1, 2)) (lam a (lam b a)) (error))",
    ] {
        for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
            let arena = Arena::new();
            let source = format!("(program 1.1.0 {source})");
            let program = syn::parse_program(&arena, &source).into_result().unwrap();
            assert!(
                program.eval_version(&arena, version).term.is_err(),
                "{source}"
            );
        }
    }
}

#[test]
fn newly_enabled_builtins_have_bundled_costs_in_every_language() {
    for (source, expected) in [
        (
            "[[[(builtin expModInteger) (con integer 2)] (con integer 3)] (con integer 5)]",
            "(con integer 3)",
        ),
        (
            "[[(builtin byteStringToInteger) (con bool True)] (con bytestring #0102)]",
            "(con integer 258)",
        ),
    ] {
        for version in [PlutusVersion::V1, PlutusVersion::V2, PlutusVersion::V3] {
            let arena = Arena::new();
            let source = format!("(program 1.1.0 {source})");
            let program = syn::parse_program(&arena, &source).into_result().unwrap();
            let result = program.eval_version(&arena, version);
            assert_eq!(pretty::term(result.term.unwrap()), expected);
        }
    }
}

#[test]
fn supplied_costs_do_not_receive_bundled_fallbacks() {
    let arena = Arena::new();
    let source = "(program 1.1.0 [[[(builtin expModInteger) (con integer 2)] (con integer 3)] (con integer 5)])";
    let program = syn::parse_program(&arena, source).into_result().unwrap();
    for version in [PlutusVersion::V1, PlutusVersion::V2] {
        let result =
            program.eval_with_params(&arena, version, &[], nash_plutus::machine::ExBudget::max());
        assert!(matches!(
            result.term,
            Err(nash_plutus::machine::MachineError::NoCostForBuiltin(
                nash_plutus::builtin::DefaultFunction::ExpModInteger
            ))
        ));
    }
}
