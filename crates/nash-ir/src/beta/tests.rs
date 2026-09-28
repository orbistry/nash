use super::*;
use crate::{
    anf,
    core::Binder,
    hygiene,
    pretty::pretty,
    ty::{ConstTy, TermTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F};
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
        let before = $input;
        let after = reduce(b, before);
        let fixed = simplify(b, before);
        insta::assert_snapshot!($($name,)? format!(
            "--- core before\n{}\n--- core after beta\n{}\n--- core after rules 1 + 2\n{}",
            pretty(before),
            pretty(after),
            pretty(fixed)
        ));
        anf::validate(before).unwrap();
        hygiene::validate(before, &[]).unwrap();
        for core in [after, fixed] {
            anf::validate(core).unwrap();
            hygiene::validate(core, &[]).unwrap();
            assert_eq!(core.ty, before.ty);
        }
        assert!(std::ptr::eq(fixed, simplify(b, fixed)));
    }};
}
#[test]
fn beta_exposes_aliases() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let core = b.app(
        b.lam(&[x], b.let_(y, b.var(x.name, INT), b.var(y.name, INT))),
        &[b.int(42)],
        INT,
    );
    assert_optimization_snapshot!(&b, core);
}
#[test]
fn partial_application_binds_outside_remaining_lambda() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let body = b.builtin(
        F::AddInteger,
        &[b.var(x.name, INT), b.var(y.name, INT)],
        INT,
    );
    let ty = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[INT]), INT)));
    assert_optimization_snapshot!(&b, b.app(b.lam(&[x, y], body), &[b.int(40)], ty));
}
#[test]
fn oversaturation_exposes_more_than_one_round() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let z = binder(&b, "z", INT);
    let f = b.lam(&[x], b.lam(&[y], b.lam(&[z], b.var(z.name, INT))));
    let core = b.app(f, &[b.int(1), b.int(2), b.int(42)], INT);
    let first = reduce(&b, crate::propagate::propagate(&b, core));
    let second = reduce(&b, crate::propagate::propagate(&b, first));
    assert_optimization_snapshot!(&b, core);
    assert_ne!(pretty(second), pretty(simplify(&b, core)));
}
#[test]
fn beta_splices_nested_rhs_and_preserves_strict_computation() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "result", INT);
    let strict = binder(&b, "strict", INT);
    let call = b.app(
        b.lam(&[x], b.let_(strict, b.error(INT), b.var(x.name, INT))),
        &[b.int(42)],
        INT,
    );
    assert_optimization_snapshot!(&b, b.let_(y, call, b.var(y.name, INT)));
}
#[test]
fn root_and_peeled_type_views_survive() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "result", INT);
    let bytes = Ty::Const(&ConstTy::Bytes);
    let call = b.app(b.lam(&[x], b.var(x.name, INT)), &[b.int(42)], bytes);
    let core = b.with_type(b.let_(y, call, b.var(y.name, bytes)), INT);
    let after = reduce(&b, core);
    assert_eq!(after.ty, INT);
    let CoreKind::Let { body, .. } = after.kind else {
        panic!("parameter binding")
    };
    let CoreKind::Let { value, .. } = body.kind else {
        panic!("result binding")
    };
    assert_eq!(value.ty, bytes);
    assert_eq!(simplify(&b, core).ty, INT);
}
#[test]
fn no_redex_preserves_pointer() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let core = b.int(42);
    assert!(std::ptr::eq(core, reduce(&b, core)));
    assert!(std::ptr::eq(core, simplify(&b, core)));
}

#[test]
fn recursive_prefix_is_spliced_without_crossing_its_body() {
    use crate::core::RecBinder;
    let a = Arena::new();
    let b = Builder::new(&a);
    let ty = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[INT]), INT)));
    let f = binder(&b, "worker", ty);
    let n = binder(&b, "n", INT);
    let x = binder(&b, "x", INT);
    let result = binder(&b, "result", INT);
    let body = b.let_rec(
        &[RecBinder {
            binder: f,
            params: a.alloc_slice_copy(&[n]),
            static_params: &[],
            body: b.var(n.name, INT),
        }],
        b.app(b.var(f.name, ty), &[b.var(x.name, INT)], INT),
    );
    let core = b.let_(
        result,
        b.app(b.lam(&[x], body), &[b.int(42)], INT),
        b.var(result.name, INT),
    );
    assert_optimization_snapshot!(&b, core);
}

#[test]
fn oversaturation_temp_avoids_enclosing_ids_with_fresh_builder() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let outer = binder(&b, "outer", INT);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let fun = b.lam(
        &[x],
        b.trace(
            b.lit(nash_plutus::constant::Constant::string(&a, "body")),
            b.lam(&[y], b.var(y.name, INT)),
        ),
    );
    let core = b.lam(&[outer], b.app(fun, &[b.int(1), b.int(42)], INT));
    let fresh = Builder::new(&a);
    assert_optimization_snapshot!(&fresh, core);
}
