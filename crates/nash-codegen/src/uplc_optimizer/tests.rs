use super::*;
use nash_plutus::{builtin::DefaultFunction as F, pretty};

fn check(a: &Arena, name: &str, input: Uplc<'_>, fails: bool) {
    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    let _settings = settings.bind_to_scope();
    let after = optimize(a, input);
    let before_eval = crate::harness::eval_named(a, input);
    let after_eval = crate::harness::eval_named(a, after);
    assert_eq!(after_eval.result.starts_with("error:"), fails);
    insta::assert_snapshot!(
        name,
        format!(
            "--- UPLC before\n{}\n--- UPLC after\n{}\n--- result\n{}\n--- logs\n{:?}",
            pretty::term(input),
            pretty::term(after),
            after_eval.result,
            after_eval.logs
        )
    );
    assert_eq!(before_eval.observable, after_eval.observable);
    assert_eq!(before_eval.logs, after_eval.logs);
    assert!(std::ptr::eq(after, optimize(a, after)));
}
fn trace<'a>(a: &'a Arena, message: &'a str, term: Uplc<'a>) -> Uplc<'a> {
    Term::trace(a)
        .force(a)
        .apply(a, Term::string(a, message))
        .apply(a, term.delay(a))
        .force(a)
}
#[test]
fn cleanup_keeps_strict_effects() {
    let a = Arena::new();
    let x = Name::new(&a, "x", 0);
    check(
        &a,
        "identity_trace",
        Term::var(&a, x)
            .lambda(&a, x)
            .apply(&a, trace(&a, "argument", Term::integer_from(&a, 42))),
        false,
    );
    check(
        &a,
        "unused_error",
        Term::integer_from(&a, 42)
            .lambda(&a, x)
            .apply(&a, trace(&a, "argument", Term::error(&a))),
        true,
    );
    check(
        &a,
        "unused_value",
        Term::integer_from(&a, 42)
            .lambda(&a, x)
            .apply(&a, trace(&a, "cold", Term::error(&a)).delay(&a)),
        false,
    );
    check(
        &a,
        "value_in_delay",
        Term::var(&a, x)
            .delay(&a)
            .lambda(&a, x)
            .apply(&a, Term::integer_from(&a, 42))
            .force(&a),
        false,
    );
}
#[test]
fn substitution_does_not_capture_shadowed_names() {
    let a = Arena::new();
    let x = Name::new(&a, "x", 0);
    let y = Name::new(&a, "y", 1);
    // y=42; (lambda x. lambda y. x) y 0. Inner y must not capture outer y.
    let root = Term::var(&a, x)
        .lambda(&a, y)
        .lambda(&a, x)
        .apply(&a, Term::var(&a, y))
        .apply(&a, Term::integer_from(&a, 0))
        .lambda(&a, y)
        .apply(&a, Term::integer_from(&a, 42));
    check(&a, "shadow_capture", root, false);
}
#[test]
fn packing_retains_stages_and_effectful_arguments() {
    let a = Arena::new();
    let x = Name::new(&a, "x", 0);
    let y = Name::new(&a, "y", 1);
    let z = Name::new(&a, "z", 2);
    let f = Name::new(&a, "f", 3);
    let sum = Term::add_integer(&a)
        .apply(&a, Term::var(&a, x))
        .apply(&a, Term::var(&a, y));
    let body = Term::add_integer(&a)
        .apply(&a, sum)
        .apply(&a, Term::var(&a, z));
    let func = trace(
        &a,
        "first",
        trace(&a, "second", body.lambda(&a, z)).lambda(&a, y),
    )
    .lambda(&a, x);
    for (name, last, fails) in [
        ("packed_values", Term::integer_from(&a, 2), false),
        (
            "effectful_last_argument",
            trace(&a, "last", Term::integer_from(&a, 2)),
            false,
        ),
        (
            "failing_last_argument",
            trace(&a, "last", Term::error(&a)),
            true,
        ),
    ] {
        // Twice-used f retains an unknown function value at the first call.
        let call = Term::var(&a, f)
            .apply(&a, Term::integer_from(&a, 20))
            .apply(&a, Term::integer_from(&a, 20))
            .apply(&a, last);
        let root = Term::constr(
            &a,
            0,
            a.alloc_slice_copy(&[call, Term::var(&a, f).delay(&a)]),
        )
        .lambda(&a, f)
        .apply(&a, func);
        // Return the ground call result; the second field is deliberately cold.
        let unused = Name::new(&a, "unused", 4);
        let result = Name::new(&a, "result", 5);
        let select = Term::var(&a, result).lambda(&a, unused).lambda(&a, result);
        check(
            &a,
            name,
            Term::case(&a, root, a.alloc_slice_copy(&[select])),
            fails,
        );
    }
}
#[test]
fn partial_builtin_and_forced_sharing_stay_strict() {
    let a = Arena::new();
    let f = Name::new(&a, "f", 0);
    let shared = Term::builtin(&a, a.alloc(F::Trace)).force(&a);
    let call = Term::var(&a, f)
        .apply(&a, Term::string(&a, "kept"))
        .apply(&a, Term::integer_from(&a, 42));
    check(
        &a,
        "forced_reference",
        call.lambda(&a, f).apply(&a, shared),
        false,
    );
}
#[test]
fn recursive_self_application_is_not_unfolded() {
    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    let _settings = settings.bind_to_scope();
    let a = Arena::new();
    let x = Name::new(&a, "x", 0);
    let worker = Term::var(&a, x).apply(&a, Term::var(&a, x)).lambda(&a, x);
    let divergent = worker.apply(&a, worker);
    let after = optimize(&a, divergent);
    insta::assert_snapshot!(format!(
        "--- UPLC before\n{}\n--- UPLC after\n{}",
        pretty::term(divergent),
        pretty::term(after)
    ));
    assert!(std::ptr::eq(divergent, after));
    assert!(std::ptr::eq(after, optimize(&a, after)));
}

#[test]
fn maximal_packing_keeps_a_later_computation_after_the_prefix() {
    let a = Arena::new();
    let x = Name::new(&a, "x", 0);
    let y = Name::new(&a, "y", 1);
    let z = Name::new(&a, "z", 2);
    let w = Name::new(&a, "w", 3);
    let f = Name::new(&a, "f", 4);
    let add = |l, r| Term::add_integer(&a).apply(&a, l).apply(&a, r);
    let sum = add(
        add(Term::var(&a, x), Term::var(&a, y)),
        add(Term::var(&a, z), Term::var(&a, w)),
    );
    let func = trace(&a, "third", sum.lambda(&a, w))
        .lambda(&a, z)
        .lambda(&a, y)
        .lambda(&a, x);
    let unused = Name::new(&a, "unused", 5);
    let result = Name::new(&a, "result", 6);
    let select = Term::var(&a, result).lambda(&a, unused).lambda(&a, result);
    for (name, last, fails) in [
        ("four_values", Term::integer_from(&a, 2), false),
        (
            "packed_prefix_then_trace",
            trace(&a, "fourth", Term::integer_from(&a, 2)),
            false,
        ),
        (
            "packed_prefix_then_failure",
            trace(&a, "fourth", Term::error(&a)),
            true,
        ),
    ] {
        let call = Term::var(&a, f)
            .apply(&a, Term::integer_from(&a, 10))
            .apply(&a, Term::integer_from(&a, 10))
            .apply(&a, Term::integer_from(&a, 20))
            .apply(&a, last);
        let root = Term::constr(
            &a,
            0,
            a.alloc_slice_copy(&[call, Term::var(&a, f).delay(&a)]),
        )
        .lambda(&a, f)
        .apply(&a, func);
        check(
            &a,
            name,
            Term::case(&a, root, a.alloc_slice_copy(&[select])),
            fails,
        );
    }
}
