use super::*;
use crate::{
    core::Binder,
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
macro_rules! assert_optimization_snapshot {
    ($builder:expr, $input:expr $(, $name:expr)?) => {{
        let b = $builder;
        let core = $input;
        let before = crate::beta::simplify(b, core);
        let after = inline(b, before);
        let fixed = simplify(b, before);
        insta::with_settings!({omit_expression => true}, {
            insta::assert_snapshot!($($name,)? format!(
                "--- core before rule 3\n{}\n--- core after rule 3\n{}\n--- core after rules 1 + 2 + 3\n{}",
                pretty(before),
                pretty(after),
                pretty(fixed)
            ));
        });
        for v in [before, after, fixed] {
            anf::validate(v).unwrap();
            hygiene::validate(v, &[]).unwrap();
            assert_eq!(v.ty, core.ty);
        }
        assert!(std::ptr::eq(fixed, simplify(b, fixed)));
    }};
}
#[test]
fn immediate_computed_return_is_removed() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "result", INT);
    assert_optimization_snapshot!(
        &b,
        b.let_(
            x,
            b.builtin(F::AddInteger, &[b.int(40), b.int(2)], INT),
            b.var(x.name, INT)
        )
    );
}
#[test]
fn single_use_function_exposes_beta_reduction() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let lam = b.lam(
        &[x],
        b.builtin(F::AddInteger, &[b.var(x.name, INT), b.int(2)], INT),
    );
    let f = binder(&b, "f", lam.ty);
    assert_optimization_snapshot!(
        &b,
        b.let_(f, lam, b.app(b.var(f.name, f.ty), &[b.int(40)], INT))
    );
}
#[test]
fn nested_function_bindings_do_not_resurrect_removed_binders() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let inner = b.lam(&[y], b.var(y.name, INT));
    let g = binder(&b, "g", inner.ty);
    let outer = b.lam(
        &[x],
        b.let_(
            g,
            inner,
            b.app(b.var(g.name, g.ty), &[b.var(x.name, INT)], INT),
        ),
    );
    let f = binder(&b, "f", outer.ty);
    assert_optimization_snapshot!(
        &b,
        b.let_(f, outer, b.app(b.var(f.name, f.ty), &[b.int(42)], INT))
    );
}
#[test]
fn computed_function_operand_remains_bound() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let value = b.trace(
        b.lit(Constant::string(&a, "before")),
        b.lam(&[x], b.var(x.name, INT)),
    );
    let f = binder(&b, "f", value.ty);
    let core = b.let_(f, value, b.app(b.var(f.name, f.ty), &[b.int(42)], INT));
    assert_optimization_snapshot!(&b, core);
    assert!(std::ptr::eq(core, inline(&b, core)));
}
#[test]
fn computed_capture_is_not_moved_into_a_delay() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let core = b.let_(x, b.error(INT), b.delay(b.var(x.name, INT)));
    assert_optimization_snapshot!(&b, core);
    assert!(std::ptr::eq(core, inline(&b, core)));
}
#[test]
fn multiple_use_function_stays_shared() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let r = binder(&b, "result", INT);
    let lam = b.lam(&[x], b.var(x.name, INT));
    let f = binder(&b, "f", lam.ty);
    let core = b.let_(
        f,
        lam,
        b.let_(
            r,
            b.app(b.var(f.name, f.ty), &[b.int(1)], INT),
            b.app(b.var(f.name, f.ty), &[b.int(42)], INT),
        ),
    );
    assert_optimization_snapshot!(&b, core);
    assert!(std::ptr::eq(core, inline(&b, core)));
}
#[test]
fn occurrence_and_root_type_views_survive() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let bytes = Ty::Const(&ConstTy::Bytes);
    let core = b.with_type(b.let_(x, b.error(INT), b.var(x.name, bytes)), bytes);
    assert_eq!(inline(&b, core).ty, bytes);
    let delayed = b.delay(b.int(42));
    let d = binder(&b, "d", delayed.ty);
    let core = b.let_(d, delayed, b.with_type(b.var(d.name, d.ty), bytes));
    assert_eq!(inline(&b, core).ty, bytes);
}

#[test]
fn single_use_delay_and_builtin_are_values() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let delayed = b.delay(b.error(INT));
    let d = binder(&b, "delayed", delayed.ty);
    assert_optimization_snapshot!(
        &b,
        b.let_(d, delayed, b.force(b.var(d.name, d.ty), INT)),
        "single_delay"
    );
    let ty = Ty::Term(a.alloc(crate::ty::TermTy::Fun(a.alloc_slice_copy(&[INT, INT]), INT)));
    let f = binder(&b, "add", ty);
    assert_optimization_snapshot!(
        &b,
        b.let_(
            f,
            b.builtin(F::AddInteger, &[], ty),
            b.app(b.var(f.name, ty), &[b.int(40), b.int(2)], INT)
        ),
        "single_builtin"
    );
}

#[test]
fn substitution_preserves_occurrence_and_enclosing_views_separately() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let bytes = Ty::Const(&ConstTy::Bytes);
    let value = b.delay(b.int(42));
    let d = binder(&b, "d", value.ty);
    let viewed = Ty::Runtime(a.alloc(crate::ty::RuntimeTy::Delay(bytes)));
    let core = b.with_type(b.let_(d, value, b.force(b.var(d.name, viewed), bytes)), INT);
    let after = inline(&b, core);
    let CoreKind::Force(inner) = after.kind else {
        panic!("force")
    };
    assert_eq!(inner.ty, viewed);
    assert_eq!(after.ty, INT);
}

#[test]
fn forced_builtin_reference_stays_bound_even_when_returned() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let ty = Ty::Term(a.alloc(crate::ty::TermTy::Fun(
        a.alloc_slice_copy(&[Ty::Const(&ConstTy::String), INT]),
        INT,
    )));
    let f = binder(&b, "tracer", ty);
    let core = b.let_(f, b.builtin(F::Trace, &[], ty), b.var(f.name, ty));
    assert_optimization_snapshot!(&b, core);
    assert!(std::ptr::eq(core, inline(&b, core)));
}

#[test]
fn adjacent_applications_preserve_stages_and_type_views() {
    use crate::ty::TermTy;
    let a = Arena::new();
    let b = Builder::new(&a);
    let partial = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[INT]), INT)));
    let full = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[INT]), partial)));
    let f = binder(&b, "f", full);
    let p = binder(&b, "p", partial);
    for (name, occurrence_ty) in [("adjacent_calls", partial), ("call_type_view", Ty::Erased)] {
        let core = b.lam(
            &[f],
            b.let_(
                p,
                b.app(b.var(f.name, full), &[b.int(20)], partial),
                b.app(b.var(p.name, occurrence_ty), &[b.int(22)], INT),
            ),
        );
        assert_optimization_snapshot!(&b, core, name);
    }
}

#[test]
fn application_fusion_does_not_cross_a_computation_or_capture() {
    use crate::ty::TermTy;
    let a = Arena::new();
    let b = Builder::new(&a);
    let partial = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[INT]), INT)));
    let full = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[INT]), partial)));
    let f = binder(&b, "f", full);
    let p = binder(&b, "p", partial);
    let n = binder(&b, "n", INT);
    let core = b.lam(
        &[f],
        b.let_(
            p,
            b.app(b.var(f.name, full), &[b.int(20)], partial),
            b.let_(
                n,
                b.trace(b.lit(Constant::string(&a, "later")), b.int(22)),
                b.app(b.var(p.name, partial), &[b.var(n.name, INT)], INT),
            ),
        ),
    );
    assert_optimization_snapshot!(&b, core, "crossed_trace");
    let escape = b.lam(
        &[f],
        b.let_(
            p,
            b.app(b.var(f.name, full), &[b.int(20)], partial),
            b.lam(
                &[n],
                b.app(b.var(p.name, partial), &[b.var(n.name, INT)], INT),
            ),
        ),
    );
    assert_optimization_snapshot!(&b, escape, "escaping_partial_call");
}
