//! Accepted rule 3: paired Core/UPLC semantics, without budget assertions.
use nash_ir::{
    anf,
    build::Builder,
    core::*,
    hygiene,
    pretty::pretty,
    single_use::simplify,
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
    let before = nash_ir::beta::simplify(b, anf::normalize(b, core));
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
fn computed_tail_result_removes_binding() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "result", INT);
    check(
        "computed_tail",
        &b,
        b.let_(
            x,
            b.builtin(F::AddInteger, &[b.int(40), b.int(2)], INT),
            b.var(x.name, INT),
        ),
        false,
    );
}
#[test]
fn single_function_keeps_argument_and_body_order() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let lam = b.lam(&[x], traced(&b, "body", b.var(x.name, INT)));
    let f = binder(&b, "f", lam.ty);
    check(
        "single_function",
        &b,
        b.let_(
            f,
            lam,
            b.app(
                b.var(f.name, f.ty),
                &[traced(&b, "argument", b.int(42))],
                INT,
            ),
        ),
        false,
    );
}
#[test]
fn single_function_unused_argument_stays_strict() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "unused", INT);
    let lam = b.lam(&[x], traced(&b, "unreachable", b.int(42)));
    let f = binder(&b, "f", lam.ty);
    check(
        "unused_argument_failure",
        &b,
        b.let_(
            f,
            lam,
            b.app(
                b.var(f.name, f.ty),
                &[traced(&b, "failure", b.error(INT))],
                INT,
            ),
        ),
        true,
    );
}
#[test]
fn computed_binding_is_not_moved_past_trace() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    check(
        "trace_barrier",
        &b,
        b.let_(
            x,
            traced(&b, "first", b.int(42)),
            traced(&b, "second", b.var(x.name, INT)),
        ),
        false,
    );
}
#[test]
fn computed_binding_is_not_moved_into_unselected_branch() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let body = b.if_(
        b.lit(Constant::bool(&a, true)),
        b.int(42),
        b.var(x.name, INT),
    );
    check(
        "branch_barrier",
        &b,
        b.let_(x, traced(&b, "failure", b.error(INT)), body),
        true,
    );
}
#[test]
fn computed_capture_stays_strict_when_function_is_unused() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let lam = b.lam(&[y], b.var(x.name, INT));
    let f = binder(&b, "unused", lam.ty);
    check(
        "capture_barrier",
        &b,
        b.let_(
            x,
            traced(&b, "capture", b.int(1)),
            b.let_(f, lam, b.int(42)),
        ),
        false,
    );
}
#[test]
fn single_delay_keeps_its_force_site() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let value = b.delay(traced(&b, "delayed", b.int(42)));
    let d = binder(&b, "d", value.ty);
    check(
        "single_delay",
        &b,
        b.let_(
            d,
            value,
            traced(&b, "before force", b.force(b.var(d.name, d.ty), INT)),
        ),
        false,
    );
}
#[test]
fn computed_function_operand_remains_bound() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let value = traced(
        &b,
        "function",
        b.lam(&[x], traced(&b, "body", b.var(x.name, INT))),
    );
    let f = binder(&b, "f", value.ty);
    check(
        "computed_function",
        &b,
        b.let_(f, value, b.app(b.var(f.name, f.ty), &[b.int(42)], INT)),
        false,
    );
}
#[test]
fn single_builtin_reference_applies_normally() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let ty = Ty::Term(a.alloc(nash_ir::ty::TermTy::Fun(
        a.alloc_slice_copy(&[INT, INT]),
        INT,
    )));
    let f = binder(&b, "add", ty);
    check(
        "single_builtin",
        &b,
        b.let_(
            f,
            b.builtin(F::AddInteger, &[], ty),
            b.app(b.var(f.name, ty), &[b.int(40), b.int(2)], INT),
        ),
        false,
    );
}

#[test]
fn single_builtin_use_inside_repeated_function_preserves_logs() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let string = Ty::Const(&ConstTy::String);
    let ty = Ty::Term(a.alloc(nash_ir::ty::TermTy::Fun(
        a.alloc_slice_copy(&[string, INT]),
        INT,
    )));
    let tracer = binder(&b, "tracer", ty);
    let x = binder(&b, "x", INT);
    let lam = b.lam(
        &[x],
        b.app(
            b.var(tracer.name, ty),
            &[b.lit(Constant::string(&a, "tick")), b.var(x.name, INT)],
            INT,
        ),
    );
    let f = binder(&b, "repeated", lam.ty);
    let result = binder(&b, "unused", INT);
    let body = b.let_(
        result,
        b.app(b.var(f.name, f.ty), &[b.int(1)], INT),
        b.app(b.var(f.name, f.ty), &[b.int(42)], INT),
    );
    check(
        "builtin_inside_repeated_function",
        &b,
        b.let_(tracer, b.builtin(F::Trace, &[], ty), b.let_(f, lam, body)),
        false,
    );
}
