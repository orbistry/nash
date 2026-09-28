//! Rules 1 and 2: paired Core/UPLC semantics, without performance assertions.
use nash_ir::{
    anf,
    beta::simplify,
    build::Builder,
    core::*,
    hygiene,
    pretty::pretty,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
fn binder<'a>(b: &Builder<'a>, name: &'a str, ty: Ty<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh(name),
        ty,
    }
}
fn traced<'a>(b: &Builder<'a>, message: &'a str, value: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, message)), value)
}
fn check<'a>(name: &str, b: &Builder<'a>, core: &'a Core<'a>, fails: bool) {
    let before = anf::normalize(b, core);
    let after = simplify(b, before);

    let baseline = crate::harness::eval_core_raw(b.arena, before);
    let candidate = crate::harness::eval_core_raw(b.arena, after);

    assert_eq!(candidate.result.starts_with("error:"), fails);
    fn output(v: &crate::harness::Evaluated) -> String {
        format!(
            "--- uplc\n{}\n--- result\n{}\n--- logs\n{:?}",
            v.uplc, v.result, v.logs
        )
    }
    insta::assert_snapshot!(
        name,
        crate::harness::pass_snapshot(
            b.arena,
            core,
            format!(
                "--- core before\n{}\n--- baseline\n{}\n--- core after\n{}\n--- candidate\n{}",
                pretty(before),
                output(&baseline),
                pretty(after),
                output(&candidate)
            )
        )
    );
    for phase in [before, after] {
        anf::validate(phase).unwrap();
        hygiene::validate(phase, &[]).unwrap();
    }
    // Properties independent of the expected snapshot.
    assert_eq!(before.ty, after.ty);
    assert_eq!(pretty(after), pretty(simplify(b, after)));
    assert_eq!(baseline.observable, candidate.observable);
    assert_eq!(baseline.logs, candidate.logs);
}
#[test]
fn exact_application_exposes_literal_aliases() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "alias", INT);
    check(
        "exact_application",
        &b,
        b.app(
            b.lam(&[x], b.let_(y, b.var(x.name, INT), b.var(y.name, INT))),
            &[b.int(42)],
            INT,
        ),
        false,
    );
}
#[test]
fn unused_argument_still_fails_before_body() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "unused", INT);
    check(
        "unused_argument_failure",
        &b,
        b.app(
            b.lam(&[x], traced(&b, "unreachable body", b.int(42))),
            &[traced(&b, "argument", b.error(INT))],
            INT,
        ),
        true,
    );
}
#[test]
fn partial_application_evaluates_capture_before_returning_function() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let lam = b.lam(&[x, y], traced(&b, "unreachable body", b.var(x.name, INT)));
    let ty = Ty::Term(a.alloc(nash_ir::ty::TermTy::Fun(a.alloc_slice_copy(&[INT]), INT)));
    let partial = binder(&b, "partial", ty);
    check(
        "partial_capture",
        &b,
        b.let_(
            partial,
            b.app(lam, &[traced(&b, "capture", b.int(42))], ty),
            b.int(0),
        ),
        false,
    );
}
#[test]
fn multiple_arguments_remain_left_to_right() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let lam = b.lam(
        &[x, y],
        traced(
            &b,
            "body",
            b.builtin(
                F::SubtractInteger,
                &[b.var(x.name, INT), b.var(y.name, INT)],
                INT,
            ),
        ),
    );
    check(
        "argument_order",
        &b,
        b.app(
            lam,
            &[
                traced(&b, "first", b.int(44)),
                traced(&b, "second", b.int(2)),
            ],
            INT,
        ),
        false,
    );
}
#[test]
fn oversaturation_runs_intermediate_body_before_extra_argument() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let lam = b.lam(
        &[x],
        traced(
            &b,
            "intermediate",
            b.lam(&[y], traced(&b, "returned body", b.var(y.name, INT))),
        ),
    );
    check(
        "oversaturation_argument_order",
        &b,
        b.app(lam, &[b.int(0), traced(&b, "extra", b.int(42))], INT),
        false,
    );
}
#[test]
fn oversaturation_with_atomic_extra_keeps_intermediate_failure() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let ty = Ty::Term(a.alloc(nash_ir::ty::TermTy::Fun(a.alloc_slice_copy(&[INT]), INT)));
    let lam = b.lam(&[x], traced(&b, "failure", b.error(ty)));
    check(
        "oversaturation_failure",
        &b,
        b.app(lam, &[b.int(0), b.int(42)], INT),
        true,
    );
}
#[test]
fn unselected_branch_and_unforced_delay_keep_reduced_bodies_suspended() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "branch", INT);
    let y = binder(&b, "delay", INT);
    let dead = b.app(
        b.lam(&[x], traced(&b, "unreachable branch", b.error(INT))),
        &[b.int(0)],
        INT,
    );
    let delayed = b.delay(b.app(
        b.lam(&[y], traced(&b, "unreachable delay", b.error(INT))),
        &[b.int(0)],
        INT,
    ));
    let d = binder(&b, "suspended", delayed.ty);
    let core = b.let_(
        d,
        delayed,
        b.if_(b.lit(Constant::bool(&a, true)), b.int(42), dead),
    );
    check("suspended_bodies", &b, core, false);
}
#[test]
fn repeated_rounds_reduce_nested_direct_applications() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let z = binder(&b, "z", INT);
    let lam = b.lam(&[x], b.lam(&[y], b.lam(&[z], b.var(z.name, INT))));
    check(
        "multiple_rounds",
        &b,
        b.app(lam, &[b.int(0), b.int(1), b.int(42)], INT),
        false,
    );
}

#[test]
fn oversaturation_with_atomic_extra_evaluates_returned_function() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let returned = b.lam(
        &[y],
        traced(
            &b,
            "returned body",
            b.builtin(
                F::AddInteger,
                &[b.var(x.name, INT), b.var(y.name, INT)],
                INT,
            ),
        ),
    );
    let lam = b.lam(&[x], traced(&b, "intermediate", returned));
    check(
        "oversaturation_atomic_success",
        &b,
        b.app(lam, &[b.int(40), b.int(2)], INT),
        false,
    );
}

#[test]
fn oversaturation_inlines_repeated_integer_parameter() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let returned = b.lam(
        &[y],
        b.builtin(
            F::AddInteger,
            &[b.var(x.name, INT), b.var(x.name, INT)],
            INT,
        ),
    );
    let lam = b.lam(&[x], traced(&b, "intermediate", returned));
    check(
        "oversaturation_repeated_integer",
        &b,
        b.app(lam, &[b.int(21), b.int(0)], INT),
        false,
    );
}
