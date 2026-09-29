//! Integer representation cancellation: isolated trial evidence.
use nash_ir::{
    build::Builder,
    core::*,
    hygiene, inverse,
    pretty::pretty,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
fn trace<'a>(b: &Builder<'a>, s: &'a str, x: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, s)), x)
}
fn check(name: &str, b: &Builder<'_>, before: &Core<'_>, fails: bool) {
    let after = inverse::reduce(b, before);

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
    assert!(std::ptr::eq(after, inverse::reduce(b, after)));
    assert_eq!(left.observable, right.observable);
    assert_eq!(left.logs, right.logs);
}

#[test]
fn direct_integer_roundtrips() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let data = nash_ir::ty::Ty::Big(&nash_ir::ty::BigTy::Data);
    let wrap = |x| b.builtin(F::UnIData, &[b.builtin(F::IData, &[x], data)], INT);
    for (name, value, fails) in [
        ("literal", b.int(42), false),
        ("negative", b.int(-42), false),
        ("trace", trace(&b, "before", b.int(42)), false),
        ("wrong_kind", b.lit(Constant::bool(&a, true)), true),
        (
            "forged_metadata",
            b.with_type(b.lit(Constant::bool(&a, true)), INT),
            true,
        ),
        ("failure", trace(&b, "before failure", b.error(INT)), true),
    ] {
        check(name, &b, wrap(value), fails);
    }
    let encoded = b.lit(Constant::data(
        &a,
        nash_plutus::data::PlutusData::integer_from(&a, 42),
    ));
    check(
        "decoded",
        &b,
        wrap(b.builtin(F::UnIData, &[encoded], INT)),
        false,
    );
    check(
        "decode_failure",
        &b,
        wrap(b.builtin(F::UnIData, &[b.int(1)], INT)),
        true,
    );
    check(
        "reverse_valid",
        &b,
        b.builtin(F::IData, &[b.builtin(F::UnIData, &[encoded], INT)], data),
        false,
    );
    check(
        "reverse_invalid",
        &b,
        b.builtin(F::IData, &[b.builtin(F::UnIData, &[b.int(1)], INT)], data),
        true,
    );
    check("nested", &b, wrap(wrap(b.int(42))), false);
    let p = Binder {
        name: b.fresh("x"),
        ty: INT,
    };
    check(
        "unknown_valid",
        &b,
        b.app(b.lam(&[p], wrap(b.var(p.name, INT))), &[b.int(42)], INT),
        false,
    );
    check(
        "unknown_invalid",
        &b,
        b.app(
            b.lam(&[p], wrap(b.var(p.name, INT))),
            &[b.lit(Constant::bool(&a, true))],
            INT,
        ),
        true,
    );
    check(
        "cold",
        &b,
        b.if_(
            b.lit(Constant::bool(&a, true)),
            b.int(7),
            wrap(trace(&b, "cold", b.int(42))),
        ),
        false,
    );
    check(
        "partial",
        &b,
        b.app(
            b.builtin(F::UnIData, &[], nash_ir::ty::Ty::Erased),
            &[b.builtin(F::IData, &[b.int(42)], data)],
            INT,
        ),
        false,
    );
}
