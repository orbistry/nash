//! Composition contracts: deterministic progress, phase boundaries and bounded work.
use nash_ir::{
    anf, beta,
    build::Builder,
    core::*,
    dead_code, force_delay, hygiene, inverse, known_case, pair_projection,
    pretty::pretty,
    propagate, single_use, small_inline, static_lift,
    ty::{BigTy, TermTy, Ty},
    unused_params,
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant as C, flat};

// Semantic snapshots omit evaluator costs; budget regression checks live in the
// separate optimizer-perf workspace.
fn semantic_snapshot(fixture: &crate::harness::Fixture<'_>) -> String {
    format!(
        "{}\n--- result\n{}\n--- logs\n{:?}",
        fixture.code_snapshot(),
        fixture.evaluated.result,
        fixture.evaluated.logs
    )
}

fn bind<'a>(b: &Builder<'a>, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty,
    }
}
fn trace<'a>(b: &Builder<'a>, text: &'a str, core: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(C::string(b.arena, text)), core)
}
fn encoded(arena: &Arena, core: &Core<'_>) -> Vec<u8> {
    flat::encode(crate::program::assemble_core(arena, core).unwrap().program).unwrap()
}
fn chain<'a>(b: &Builder<'a>) -> &'a Core<'a> {
    let int = b.int(0).ty;
    let data = Ty::Big(&BigTy::Data);
    let x = bind(b, "x", int);
    let wrap = b.lam(&[x], b.builtin(F::IData, &[b.var(x.name, int)], data));
    let f = bind(b, "wrap", wrap.ty);
    let packed = bind(b, "packed", data);
    let sum = b.builtin(F::AddInteger, &[b.int(20), b.int(22)], int);
    b.let_(
        f,
        wrap,
        b.let_(
            packed,
            b.app(b.var(f.name, f.ty), &[sum], data),
            trace(
                b,
                "answer",
                b.if_(
                    b.lit(C::bool(b.arena, true)),
                    b.builtin(F::UnIData, &[b.var(packed.name, data)], int),
                    b.error(int),
                ),
            ),
        ),
    )
}

#[test]
fn composed_phases_are_hygienic_and_deterministic() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let core = chain(&b);
    let fixture = crate::harness::prepare_fixture(&arena, core);
    assert!(!fixture.evaluated.result.starts_with("error:"));
    let fresh = hygiene::freshen(&b, core);
    let lifted = static_lift::lift(&b, fresh);
    let shortened = unused_params::reduce(&b, lifted);
    let normalized = anf::normalize(&b, shortened);
    let optimized = crate::optimizer::optimize(&arena, core);
    let rewritten = crate::recursion::rewrite(&b, optimized).unwrap();
    let lowered = crate::harness::eval_core_raw(&arena, rewritten);
    insta::assert_snapshot!(format!(
        "{}\n--- fresh Core\n{}\n--- lifted Core\n{}\n--- signatures before ANF\n{}\n--- ANF Core\n{}\n--- optimized recursive Core\n{}\n--- rewritten Core (no second ANF)\n{}\n--- ordinary lowered output\n{}",
        semantic_snapshot(&fixture),
        pretty(fresh),
        pretty(lifted),
        pretty(shortened),
        pretty(normalized),
        pretty(optimized),
        pretty(rewritten),
        lowered.uplc
    ));
    fixture.assert_equivalent(&arena);
    for phase in [fresh, lifted, shortened, normalized, optimized, rewritten] {
        hygiene::validate(phase, &[]).unwrap();
        assert_eq!(core.ty, phase.ty);
    }
    anf::validate(normalized).unwrap();
    anf::validate(optimized).unwrap();
    // Check each constituent rewrite, including intermediate nested binding forms.
    let mut phase = normalized;
    for pass in [
        inverse::reduce,
        force_delay::reduce,
        known_case::reduce_bool,
        known_case::reduce_literals,
        propagate::propagate,
        beta::reduce,
        single_use::inline,
        small_inline::inline,
        dead_code::simplify_bindings,
        dead_code::prune_recursive,
        known_case::reduce_constr,
        known_case::reduce_bound_constr,
        known_case::reduce_list,
        known_case::reduce_data,
        known_case::reduce_data_wrappers,
        pair_projection::reduce,
        known_case::reduce_constr_data,
    ] {
        phase = pass(&b, phase);
        hygiene::validate(phase, &[]).unwrap();
        assert_eq!(core.ty, phase.ty);
    }
    let cleaned = known_case::simplify_constr_data(&b, phase);
    anf::validate(cleaned).unwrap();
    assert!(std::ptr::eq(
        cleaned,
        known_case::simplify_constr_data(&b, cleaned)
    ));
    assert!(std::ptr::eq(
        optimized,
        known_case::simplify_constr_data(&b, optimized)
    ));
    let repeat = crate::optimizer::optimize(&arena, core);
    assert_eq!(pretty(optimized), pretty(repeat));
    assert_eq!(encoded(&arena, optimized), encoded(&arena, repeat));
    let twice = crate::optimizer::optimize(&arena, optimized);
    assert_eq!(encoded(&arena, optimized), encoded(&arena, twice));
    assert_eq!(fixture.evaluated.observable, lowered.observable);
    assert_eq!(fixture.evaluated.logs, lowered.logs);
}

#[test]
fn same_size_binding_reassociation_exposes_more_cleanup() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let x = bind(&b, "x", int);
    let note = bind(&b, "note", int);
    let lambda = b.lam(&[x], b.var(x.name, int));
    let f = bind(&b, "f", lambda.ty);
    // Nested lets arise from accepted cancellation and inlining; beta flattens them.
    let core = b.let_(
        f,
        b.let_(note, trace(&b, "before", b.int(0)), lambda),
        b.app(b.var(f.name, f.ty), &[b.int(42)], int),
    );
    let step = beta::reduce(&b, core);
    let after = small_inline::simplify(&b, step);
    let fixture = crate::harness::prepare_fixture(&arena, core);
    let result = crate::harness::eval_core_raw(&arena, after);
    assert!(!result.result.starts_with("error:"));
    insta::assert_snapshot!(format!(
        "{}\n--- isolated before\n{}\n--- same-size reassociation\n{}\n--- cleanup fixed point\n{}\n--- lowered\n{}",
        semantic_snapshot(&fixture),
        pretty(core),
        pretty(step),
        pretty(after),
        result.uplc
    ));
    fixture.assert_equivalent(&arena);
    let count = |core: &Core<'_>| {
        let mut n = 0;
        core.walk(&mut |_| n += 1);
        n
    };
    assert_eq!(count(core), count(step));
    assert!(!std::ptr::eq(core, step));
    assert!(!std::ptr::eq(step, after));
    assert!(std::ptr::eq(after, small_inline::simplify(&b, after)));
    hygiene::validate(after, &[]).unwrap();
    anf::validate(after).unwrap();
    assert_eq!(fixture.evaluated.observable, result.observable);
    assert_eq!(fixture.evaluated.logs, result.logs);
}

#[test]
fn reachable_cycle_reaches_cleanup_fixed_point_without_execution() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let ty = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)));
    let f = bind(&b, "loop", ty);
    let n = bind(&b, "n", int);
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[n]),
            static_params: &[],
            body: trace(
                &b,
                "loop",
                b.app(b.var(f.name, ty), &[b.var(n.name, int)], int),
            ),
        }],
        b.app(b.var(f.name, ty), &[b.int(0)], int),
    );
    let after = crate::optimizer::optimize(&arena, core);
    insta::assert_snapshot!(crate::harness::pass_snapshot(
        &arena,
        core,
        format!(
            "--- recursive cleanup fixed point (not evaluated)\n{}",
            pretty(after)
        )
    ));
    hygiene::validate(after, &[]).unwrap();
    anf::validate(after).unwrap();
    assert!(std::ptr::eq(
        after,
        known_case::simplify_constr_data(&b, after)
    ));
    assert_eq!(
        pretty(after),
        pretty(crate::optimizer::optimize(&arena, core))
    );
}

#[test]
fn later_cleanup_and_signature_removal_reach_a_joint_fixed_point() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let x = bind(&b, "x", int);
    let y = bind(&b, "y", int);
    let value = b.lam(
        &[x, y],
        b.if_(
            b.lit(C::bool(&arena, true)),
            b.var(y.name, int),
            b.var(x.name, int),
        ),
    );
    let f = bind(&b, "f", value.ty);
    let call = |n| b.app(b.var(f.name, f.ty), &[b.int(7), b.int(n)], int);
    let core = b.let_(
        f,
        value,
        b.builtin(F::AddInteger, &[call(20), call(22)], int),
    );
    let once = crate::optimizer::optimize(&arena, core);
    let twice = crate::optimizer::optimize(&arena, once);
    let fixture = crate::harness::prepare_fixture(&arena, core);
    let result =
        crate::harness::eval_core_raw(&arena, crate::recursion::rewrite(&b, twice).unwrap());
    insta::assert_snapshot!(format!(
        "{}\n--- second invocation\n{}\n--- second lowered\n{}",
        semantic_snapshot(&fixture),
        pretty(twice),
        result.uplc
    ));
    fixture.assert_equivalent(&arena);
    assert_eq!(encoded(&arena, once), encoded(&arena, twice));
    hygiene::validate(twice, &[]).unwrap();
    anf::validate(twice).unwrap();
    assert_eq!(fixture.evaluated.observable, result.observable);
    assert_eq!(fixture.evaluated.logs, result.logs);
}

#[test]
fn folding_reaches_fixed_point_beyond_128_calls() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = b.int(0).ty;
    let fields: Vec<_> = (0..130)
        .map(|i| b.builtin(F::AddInteger, &[b.int(i), b.int(1)], int))
        .collect();
    let core = b.constr(0, &fields, Ty::Erased);
    let once = crate::optimizer::optimize(&arena, core);
    let twice = crate::optimizer::optimize(&arena, once);
    let fixture = crate::harness::prepare_fixture(&arena, core);
    let result = crate::harness::eval_core_raw(&arena, twice);
    assert!(!result.result.starts_with("error:"));
    insta::assert_snapshot!(format!(
        "{}\n--- second invocation Core\n{}\n--- second invocation UPLC\n{}",
        semantic_snapshot(&fixture),
        pretty(twice),
        result.uplc
    ));
    fixture.assert_equivalent(&arena);
    assert_eq!(
        pretty(once),
        pretty(crate::optimizer::optimize(&arena, core))
    );
    assert_eq!(encoded(&arena, once), encoded(&arena, twice));
    assert!(std::ptr::eq(once, crate::constant_fold::simplify(&b, once)));
    assert_eq!(fixture.evaluated.observable, result.observable);
    assert_eq!(fixture.evaluated.logs, result.logs);
    for phase in [once, twice] {
        hygiene::validate(phase, &[]).unwrap();
        anf::validate(phase).unwrap();
    }
    assert!(std::ptr::eq(
        once,
        known_case::simplify_constr_data(&b, once)
    ));
}
