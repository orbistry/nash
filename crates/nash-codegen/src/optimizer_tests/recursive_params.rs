//! Recursive signature reduction before ANF, including forwarding dependencies.
#[path = "../../tests/support/recursive_params.rs"]
mod input;
use nash_ir::{anf, build::Builder, hygiene, pretty::pretty, unused_params};
use nash_plutus::arena::Arena;

#[test]
fn recursive_parameter_cases() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    for (name, before, fails) in input::cases(&b) {
        let fresh = Builder::new(&arena);
        let after = unused_params::reduce(&fresh, before);
        let original =
            crate::harness::eval_core_raw(&arena, crate::recursion::rewrite(&b, before).unwrap());
        let normalized = anf::normalize(&fresh, after);
        let result = crate::harness::eval_core_raw(
            &arena,
            crate::recursion::rewrite(&b, normalized).unwrap(),
        );
        assert_eq!(result.result.starts_with("error:"), fails);
        insta::assert_snapshot!(
            name,
            crate::harness::pass_snapshot(
                &arena,
                before,
                format!(
                    "--- isolated Core before\n{}\n--- isolated Core after\n{}\n--- isolated UPLC before\n{}\n--- isolated UPLC after\n{}\n--- result\n{}\n--- logs\n{:?}",
                    pretty(before),
                    pretty(after),
                    original.uplc,
                    result.uplc,
                    result.result,
                    result.logs
                )
            )
        );
        assert_eq!(
            !std::ptr::eq(before, after),
            !matches!(
                name,
                "mutual_consumed"
                    | "partial"
                    | "escaping"
                    | "oversaturated"
                    | "compound_dependency"
            )
        );
        assert_eq!(original.observable, result.observable);
        assert_eq!(original.logs, result.logs);
        assert_eq!(before.ty, after.ty);
        hygiene::validate(after, &[]).unwrap();
        anf::validate(normalized).unwrap();
        assert!(std::ptr::eq(after, unused_params::reduce(&fresh, after)));
    }
}

#[test]
fn static_indices_and_call_type_views_survive() {
    use nash_ir::{
        core::*,
        ty::{ConstTy, TermTy, Ty},
    };
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let bytes = Ty::Const(&ConstTy::Bytes);
    let fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int, int]), int)));
    let f = Binder {
        name: b.fresh("f"),
        ty: fun,
    };
    let x = Binder {
        name: b.fresh("unused"),
        ty: int,
    };
    let y = Binder {
        name: b.fresh("used"),
        ty: int,
    };
    let view = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int, int]), bytes)));
    let before = b.with_type(
        b.let_rec(
            &[RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[x, y]),
                static_params: &[1],
                body: b.var(y.name, int),
            }],
            b.app(b.var(f.name, view), &[b.int(0), b.int(42)], bytes),
        ),
        Ty::Erased,
    );
    let after = unused_params::reduce(&b, before);
    insta::assert_snapshot!(crate::harness::pass_snapshot(
        &arena,
        before,
        format!(
            "--- isolated Core before\n{}\n--- isolated Core after\n{}",
            pretty(before),
            pretty(after)
        )
    ));
    assert_eq!(after.ty, Ty::Erased);
    let CoreKind::LetRec { binders: [r], body } = after.kind else {
        panic!("group")
    };
    assert_eq!(
        r.binder.ty,
        Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)))
    );
    let CoreKind::App { func, .. } = body.kind else {
        panic!("call")
    };
    assert_eq!(
        func.ty,
        Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), bytes)))
    );
    hygiene::validate(after, &[]).unwrap();
}

#[test]
fn forwarding_divergence_remains_delayed_and_strict() {
    use nash_ir::{
        core::*,
        ty::{TermTy, Ty},
    };
    use nash_plutus::builtin::DefaultFunction as F;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)));
    let f = Binder {
        name: b.fresh("f"),
        ty: fun,
    };
    let g = Binder {
        name: b.fresh("g"),
        ty: fun,
    };
    let x = Binder {
        name: b.fresh("x"),
        ty: int,
    };
    let y = Binder {
        name: b.fresh("y"),
        ty: int,
    };
    let invoke = |use_f: bool, x| b.app(b.var(if use_f { f.name } else { g.name }, fun), &[x], int);
    let before = b.let_rec(
        &[
            RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[x]),
                static_params: &[],
                body: invoke(false, b.var(x.name, int)),
            },
            RecBinder {
                binder: g,
                params: arena.alloc_slice_copy(&[y]),
                static_params: &[],
                body: invoke(true, b.var(y.name, int)),
            },
        ],
        invoke(true, b.builtin(F::AddInteger, &[b.int(1), b.int(2)], int)),
    );
    let after = unused_params::reduce(&b, before);
    // Render deliberate divergence without evaluating it.
    insta::assert_snapshot!(crate::harness::pass_snapshot(
        &arena,
        before,
        format!(
            "--- isolated Core before\n{}\n--- isolated Core after\n{}",
            pretty(before),
            pretty(after)
        )
    ));
    hygiene::validate(after, &[]).unwrap();
    assert!(std::ptr::eq(after, unused_params::reduce(&b, after)));
    crate::recursion::rewrite(&b, after).unwrap();
}

#[test]
fn delayed_mutual_worker_returns_captured_closure() {
    use nash_ir::{
        core::*,
        ty::{TermTy, Ty},
    };
    use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let unary = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)));
    let fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), unary)));
    let bind = |text, ty| Binder {
        name: b.fresh(text),
        ty,
    };
    let outer = bind("captured", int);
    let f = bind("f", fun);
    let g = bind("g", fun);
    let x = bind("x", int);
    let y = bind("y", int);
    let z = bind("z", int);
    let f_body = b.lam(
        &[z],
        b.builtin(
            F::AddInteger,
            &[b.var(outer.name, int), b.var(z.name, int)],
            int,
        ),
    );
    let g_body = b.trace(
        b.lit(Constant::string(&arena, "worker")),
        b.app(b.var(f.name, fun), &[b.var(y.name, int)], unary),
    );
    let before = b.let_(
        outer,
        b.int(40),
        b.let_rec(
            &[
                RecBinder {
                    binder: f,
                    params: arena.alloc_slice_copy(&[x]),
                    static_params: &[],
                    body: f_body,
                },
                RecBinder {
                    binder: g,
                    params: arena.alloc_slice_copy(&[y]),
                    static_params: &[],
                    body: g_body,
                },
            ],
            b.app(
                b.app(b.var(g.name, fun), &[b.int(7)], unary),
                &[b.int(2)],
                int,
            ),
        ),
    );
    let after = unused_params::reduce(&b, before);
    let original =
        crate::harness::eval_core_raw(&arena, crate::recursion::rewrite(&b, before).unwrap());
    let result =
        crate::harness::eval_core_raw(&arena, crate::recursion::rewrite(&b, after).unwrap());
    assert!(!result.result.starts_with("error:"));
    insta::assert_snapshot!(crate::harness::pass_snapshot(
        &arena,
        before,
        format!(
            "--- isolated Core before\n{}\n--- isolated Core after\n{}\n--- isolated UPLC after\n{}\n--- result\n{}\n--- logs\n{:?}",
            pretty(before),
            pretty(after),
            result.uplc,
            result.result,
            result.logs
        )
    ));
    assert_eq!(original.observable, result.observable);
    assert_eq!(original.logs, result.logs);
    hygiene::validate(after, &[]).unwrap();
}

#[test]
fn mixed_group_prunes_to_single_delayed_worker() {
    use nash_ir::{
        core::*,
        ty::{TermTy, Ty},
    };
    use nash_plutus::constant::Constant;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)));
    let bind = |text| Binder {
        name: b.fresh(text),
        ty: int,
    };
    let f = Binder {
        name: b.fresh("delayed"),
        ty: fun,
    };
    let g = Binder {
        name: b.fresh("unusedFunction"),
        ty: fun,
    };
    let x = bind("x");
    let y = bind("y");
    let before = b.let_rec(
        &[
            RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[x]),
                static_params: &[],
                body: b.trace(b.lit(Constant::string(&arena, "body")), b.int(42)),
            },
            RecBinder {
                binder: g,
                params: arena.alloc_slice_copy(&[y]),
                static_params: &[],
                body: b.var(y.name, int),
            },
        ],
        b.app(b.var(f.name, fun), &[b.int(0)], int),
    );
    let reduced = unused_params::reduce(&b, before);
    let after = nash_ir::dead_code::prune_recursive(&b, reduced);
    let original =
        crate::harness::eval_core_raw(&arena, crate::recursion::rewrite(&b, before).unwrap());
    let result =
        crate::harness::eval_core_raw(&arena, crate::recursion::rewrite(&b, after).unwrap());
    assert!(!result.result.starts_with("error:"));
    insta::assert_snapshot!(crate::harness::pass_snapshot(
        &arena,
        before,
        format!(
            "--- reduced mixed group\n{}\n--- pruned group\n{}\n--- lowered\n{}\n--- result\n{}\n--- logs\n{:?}",
            pretty(reduced),
            pretty(after),
            result.uplc,
            result.result,
            result.logs
        )
    ));
    assert_eq!(original.observable, result.observable);
    assert_eq!(original.logs, result.logs);
    hygiene::validate(after, &[]).unwrap();
    assert!(std::ptr::eq(
        after,
        nash_ir::dead_code::prune_recursive(&b, after)
    ));
}
