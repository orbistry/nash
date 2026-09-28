//! Direct cancellation preserves the force's evaluation point.
use nash_ir::{
    build::Builder,
    core::*,
    force_delay, hygiene,
    pretty::pretty,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
fn trace<'a>(b: &Builder<'a>, s: &'a str, x: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, s)), x)
}
fn check(name: &str, b: &Builder<'_>, before: &Core<'_>, fails: bool) {
    let after = force_delay::reduce(b, before);

    let left = crate::harness::eval_core_raw(b.arena, before);
    let right = crate::harness::eval_core_raw(b.arena, after);

    assert_eq!(right.result.starts_with("error:"), fails);
    insta::assert_snapshot!(
        name,
        crate::harness::pass_snapshot(
            b.arena,
            before,
            format!(
                "--- core before\n{}\n--- uplc before\n{}\n--- core after\n{}\n--- uplc after\n{}\n--- result\n{}\n--- logs\n{:?}",
                pretty(before),
                left.uplc,
                pretty(after),
                right.uplc,
                right.result,
                right.logs
            )
        )
    );
    hygiene::validate(after, &[]).unwrap();
    // Properties independent of the expected snapshot.
    assert_eq!(before.ty, after.ty);
    assert!(std::ptr::eq(after, force_delay::reduce(b, after)));
    assert_eq!(left.observable, right.observable);
    assert_eq!(left.logs, right.logs);
}
#[test]
fn literal_trace_failure_and_nested_pairs() {
    let a = Arena::new();
    let b = Builder::new(&a);
    check("literal", &b, b.force(b.delay(b.int(42)), INT), false);
    check(
        "trace",
        &b,
        b.force(b.delay(trace(&b, "body", b.int(42))), INT),
        false,
    );
    check(
        "failure",
        &b,
        b.force(b.delay(trace(&b, "before failure", b.error(INT))), INT),
        true,
    );
    check(
        "nested",
        &b,
        b.force(b.delay(b.force(b.delay(b.int(42)), INT)), INT),
        false,
    );
}
#[test]
fn selected_and_cold_branches() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (hot, name) in [(true, "hot_branch"), (false, "cold_branch")] {
        check(
            name,
            &b,
            b.if_(
                b.lit(Constant::bool(&a, hot)),
                b.force(b.delay(trace(&b, "selected", b.int(42))), INT),
                b.int(0),
            ),
            false,
        );
    }
}
#[test]
fn strict_order_and_repeated_forcing() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let first = Binder {
        name: b.fresh("first"),
        ty: INT,
    };
    let call = b.force(b.delay(trace(&b, "body", b.int(21))), INT);
    let root = b.let_(
        first,
        trace(&b, "first", b.int(0)),
        b.builtin(F::AddInteger, &[call, call], INT),
    );
    check("repeated_order", &b, root, false);
}
#[test]
fn reverse_pair_and_invalid_force_remain() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let invalid = b.force(b.int(42), INT);
    check("invalid_force", &b, invalid, true);
    assert!(std::ptr::eq(invalid, force_delay::reduce(&b, invalid)));
    let suspended = b.delay(invalid);
    check("reverse_pair", &b, suspended, false);
    assert!(std::ptr::eq(suspended, force_delay::reduce(&b, suspended)));
}
#[test]
fn outer_type_view_is_retained() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let root = b.force(b.delay(b.int(42)), Ty::Const(&ConstTy::Bytes));
    let after = force_delay::reduce(&b, root);
    assert_eq!(root.ty, after.ty);
    assert!(matches!(after.kind, CoreKind::Lit(_)));
}

#[test]
fn exposed_let_body_needs_cleanup_but_not_another_anf_pass() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = Binder {
        name: b.fresh("x"),
        ty: INT,
    };
    let y = Binder {
        name: b.fresh("y"),
        ty: INT,
    };
    let delayed_body = b.let_(x, trace(&b, "inner", b.int(21)), b.var(x.name, INT));
    let root = b.let_(
        y,
        b.force(b.delay(delayed_body), INT),
        b.builtin(F::AddInteger, &[b.var(y.name, INT), b.int(21)], INT),
    );
    check("exposed_let", &b, root, false);
    nash_ir::anf::validate(root).unwrap();
    let after = nash_ir::small_inline::simplify(&b, root);
    nash_ir::anf::validate(after).unwrap();
    let before_result = crate::harness::eval_core_raw(&a, root);
    let after_result = crate::harness::eval_core_raw(&a, after);
    assert_eq!(before_result.observable, after_result.observable);
    assert_eq!(before_result.logs, after_result.logs);
}
