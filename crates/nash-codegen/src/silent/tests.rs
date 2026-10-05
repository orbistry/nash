use super::*;
use nash_config::OptimizationLevel;
use nash_ir::{
    pretty::pretty,
    ty::{ConstTy, TermTy},
};
use nash_plutus::{
    builtin::DefaultFunction as F, constant::Constant, machine::PlutusVersion, term::Term,
};

fn check(arena: &Arena, name: &str, core: &Core<'_>, fails: bool) {
    let after = crate::optimizer::optimize_silent(arena, core).unwrap();
    let before_program = crate::program::assemble_core(arena, core).unwrap();
    let after_program = crate::program::assemble_core_with_options(
        arena,
        core,
        PlutusVersion::V3,
        OptimizationLevel::O2,
    )
    .unwrap();
    let baseline = crate::harness::eval_named(arena, before_program.named);
    let optimized = crate::harness::eval_named(arena, after_program.named);
    assert_eq!(optimized.result.starts_with("error:"), fails);
    insta::with_settings!({description => "Hand-built Core", omit_expression => true}, {
        insta::assert_snapshot!(name, format!("--- unoptimized Core\n{}\n--- unoptimized UPLC\n{}\n--- optimized Core (O2)\n{}\n--- optimized UPLC (O2)\n{}\n--- O0 result\n{}\n--- O0 logs\n{:?}\n--- O2 result\n{}\n--- O2 logs\n{:?}", pretty(core), baseline.uplc, pretty(after), optimized.uplc, baseline.result, baseline.logs, optimized.result, optimized.logs));
    });
    // O2 intentionally drops failures in trace messages. Value failures remain
    // covered by the success/error guard and the evaluated snapshot.
    assert_eq!(core.ty, after.ty);
    nash_ir::hygiene::validate(after, &[]).unwrap();
    nash_ir::anf::validate(after).unwrap();
    assert_silent(after_program.named);
    let twice = crate::program::assemble_core_with_options(
        arena,
        after,
        PlutusVersion::V3,
        OptimizationLevel::O2,
    )
    .unwrap();
    assert_eq!(
        nash_plutus::flat::encode(after_program.program).unwrap(),
        nash_plutus::flat::encode(twice.program).unwrap()
    );
}
fn assert_silent(root: &Term<'_, nash_plutus::binder::Name<'_>>) {
    let mut pending = vec![root];
    while let Some(term) = pending.pop() {
        match term {
            Term::Builtin(F::Trace) => panic!("O2 emitted a trace builtin"),
            Term::Lambda { body, .. } | Term::Delay(body) | Term::Force(body) => pending.push(body),
            Term::Apply { function, argument } => pending.extend([*function, *argument]),
            Term::Constr { fields, .. } => pending.extend(fields.iter().copied()),
            Term::Case { constr, branches } => {
                pending.push(constr);
                pending.extend(branches.iter().copied());
            }
            _ => {}
        }
    }
}
#[test]
fn prebuilt_core_trace_and_delayed_body() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let message = b.lit(Constant::string(&arena, "message"));
    check(&arena, "core_trace", b.trace(message, b.int(42)), false);
    check(
        &arena,
        "core_trace_failing_message",
        b.trace(b.error(message.ty), b.int(42)),
        false,
    );
    check(
        &arena,
        "trace_inside_delay",
        b.delay(b.trace(message, b.error(Ty::Const(&ConstTy::Int)))),
        false,
    );
    let value = Binder {
        name: b.fresh("value"),
        ty: Ty::Const(&ConstTy::Int),
    };
    check(
        &arena,
        "trace_inside_lambda",
        b.lam(&[value], b.trace(message, b.var(value.name, value.ty))),
        false,
    );
}
#[test]
fn builtin_trace_all_saturations() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let string = Ty::Const(&ConstTy::String);
    let int = Ty::Const(&ConstTy::Int);
    let partial_ty = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)));
    let full_ty = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[string]), partial_ty)));
    let message = b.lit(Constant::string(&arena, "direct"));
    for (name, trace) in [
        (
            "builtin_zero",
            b.app(
                b.builtin(F::Trace, &[], full_ty),
                &[message, b.int(42)],
                int,
            ),
        ),
        (
            "builtin_one",
            b.app(
                b.builtin(F::Trace, &[message], partial_ty),
                &[b.int(42)],
                int,
            ),
        ),
        (
            "builtin_two",
            b.builtin(F::Trace, &[message, b.int(42)], int),
        ),
    ] {
        check(&arena, name, trace, false);
    }
    let unused = Binder {
        name: b.fresh("unused"),
        ty: partial_ty,
    };
    check(
        &arena,
        "builtin_unused_failing_partial",
        b.let_(
            unused,
            b.builtin(F::Trace, &[b.error(string)], partial_ty),
            b.int(42),
        ),
        false,
    );
}
