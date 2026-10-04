//! Inputs shared by pair-projection snapshots and explicit measurements.
use nash_ir::{
    build::Builder,
    core::*,
    ty::{ConstTy, Ty},
};
use nash_plutus::{
    builtin::DefaultFunction as F, constant::Constant as C, data::PlutusData as D, typ::Type,
};

pub fn cases<'a>(b: &Builder<'a>) -> Vec<(String, &'a Core<'a>, bool)> {
    let a = b.arena;
    let int = b.int(0).ty;
    let pair = b.lit(C::proto_pair(
        a,
        &Type::Integer,
        &Type::Integer,
        C::integer_from(a, 7),
        C::integer_from(a, 8),
    ));
    let data = b.lit(C::data(a, D::integer_from(a, 7)));
    let made = b.builtin(
        F::MkPairData,
        &[data, data],
        Ty::Const(a.alloc(ConstTy::Pair(data.ty, data.ty))),
    );
    let unwrapped = b.builtin(
        F::UnConstrData,
        &[b.lit(C::data(a, D::constr(a, 0, &[])))],
        Ty::Const(a.alloc(ConstTy::Pair(
            int,
            Ty::Const(a.alloc(ConstTy::List(data.ty))),
        ))),
    );
    let trace = |s, x| b.trace(b.lit(C::string(a, s)), x);
    let mut cases = Vec::new();
    for (name, subject, left_ty, right_ty, fails) in [
        ("literal", pair, int, int, false),
        ("traced", trace("subject", pair), int, int, false),
        ("made_data", made, data.ty, data.ty, false),
        (
            "decoded",
            unwrapped,
            int,
            Ty::Const(a.alloc(ConstTy::List(data.ty))),
            false,
        ),
        (
            "bad_decoder",
            b.builtin(F::UnConstrData, &[data], unwrapped.ty),
            int,
            Ty::Const(a.alloc(ConstTy::List(data.ty))),
            true,
        ),
        (
            "ignored_failure",
            b.builtin(
                F::MkPairData,
                &[data, trace("unused field", b.error(data.ty))],
                made.ty,
            ),
            data.ty,
            data.ty,
            true,
        ),
        (
            "wrong_subject",
            b.with_type(b.int(1), pair.ty),
            int,
            int,
            true,
        ),
        // Generic UPLC case accepts this, whereas fstPair would reject it.
        (
            "native_constructor",
            b.with_type(b.constr(0, &[b.int(7), b.int(8)], Ty::Erased), pair.ty),
            int,
            int,
            false,
        ),
    ] {
        for second in [false, true] {
            let x = Binder {
                name: b.fresh("x"),
                ty: left_ty,
            };
            let y = Binder {
                name: b.fresh("y"),
                ty: right_ty,
            };
            let selected = if second { y } else { x };
            let value = b.var(selected.name, selected.ty);
            let core = b.case(
                CaseKind::Pair,
                subject,
                &[Branch {
                    test: Test::Pair,
                    binders: a.alloc([x, y]),
                    body: value,
                }],
                None,
                value.ty,
            );
            cases.push((
                format!("{name}_{}", if second { "second" } else { "first" }),
                core,
                fails,
            ));
        }
    }
    for mode in ["body", "both", "neither", "delay", "lambda", "repeated"] {
        let x = Binder {
            name: b.fresh("x"),
            ty: int,
        };
        let y = Binder {
            name: b.fresh("y"),
            ty: int,
        };
        let xv = b.var(x.name, x.ty);
        let body = match mode {
            "both" => b.builtin(F::AddInteger, &[xv, b.var(y.name, y.ty)], int),
            "neither" => b.int(42),
            "delay" => b.delay(xv),
            "lambda" => b.lam(
                &[Binder {
                    name: b.fresh("unused"),
                    ty: int,
                }],
                xv,
            ),
            "repeated" => b.builtin(F::AddInteger, &[xv, xv], int),
            _ => trace("body", b.builtin(F::AddInteger, &[xv, b.int(1)], int)),
        };
        let core = b.case(
            CaseKind::Pair,
            trace("subject", pair),
            &[Branch {
                test: Test::Pair,
                binders: a.alloc([x, y]),
                body,
            }],
            None,
            body.ty,
        );
        // Force/application is after the case, exposing any misplaced strict work.
        let core = match mode {
            "delay" => b.force(core, int),
            "lambda" => b.app(core, &[trace("argument", b.int(0))], int),
            _ => core,
        };
        cases.push((mode.into(), core, false));
    }
    for second in [false, true] {
        let select = |subject| {
            let x = Binder {
                name: b.fresh("x"),
                ty: int,
            };
            let y = Binder {
                name: b.fresh("y"),
                ty: int,
            };
            let selected = if second { y } else { x };
            b.case(
                CaseKind::Pair,
                subject,
                &[Branch {
                    test: Test::Pair,
                    binders: a.alloc([x, y]),
                    body: b.var(selected.name, int),
                }],
                None,
                int,
            )
        };
        let first = select(trace("first", pair));
        let second_core = select(trace("second", pair));
        cases.push((
            format!(
                "two_projections_{}",
                if second { "second" } else { "first" }
            ),
            b.builtin(F::AddInteger, &[first, second_core], int),
            false,
        ));
    }
    for delayed in [false, true] {
        let x = Binder {
            name: b.fresh("x"),
            ty: int,
        };
        let y = Binder {
            name: b.fresh("y"),
            ty: Ty::Const(a.alloc(ConstTy::List(data.ty))),
        };
        let value = b.var(x.name, x.ty);
        let body = if delayed {
            b.delay(value)
        } else {
            b.lam(
                &[Binder {
                    name: b.fresh("arg"),
                    ty: int,
                }],
                value,
            )
        };
        let subject = b.builtin(
            F::UnConstrData,
            &[trace("before failure", data)],
            unwrapped.ty,
        );
        let case = b.case(
            CaseKind::Pair,
            subject,
            &[Branch {
                test: Test::Pair,
                binders: a.alloc([x, y]),
                body,
            }],
            None,
            body.ty,
        );
        let ignored = Binder {
            name: b.fresh("ignored closure"),
            ty: case.ty,
        };
        cases.push((
            format!(
                "unconsumed_{}_failure",
                if delayed { "delay" } else { "lambda" }
            ),
            b.let_(ignored, case, b.int(0)),
            true,
        ));
    }
    for bad_tag in [false, true] {
        for second in [false, true] {
            let tag = Binder {
                name: b.fresh("tag"),
                ty: int,
            };
            let fields_ty = Ty::Const(a.alloc(ConstTy::List(data.ty)));
            let fields = Binder {
                name: b.fresh("fields"),
                ty: fields_ty,
            };
            let encoded = Binder {
                name: b.fresh("encoded"),
                ty: data.ty,
            };
            let decoded = Binder {
                name: b.fresh("decoded"),
                ty: unwrapped.ty,
            };
            let x = Binder {
                name: b.fresh("x"),
                ty: int,
            };
            let y = Binder {
                name: b.fresh("y"),
                ty: fields_ty,
            };
            let selected = if second { y } else { x };
            let body = b.case(
                CaseKind::Pair,
                b.var(decoded.name, decoded.ty),
                &[Branch {
                    test: Test::Pair,
                    binders: a.alloc([x, y]),
                    body: b.var(selected.name, selected.ty),
                }],
                None,
                selected.ty,
            );
            let body = b.let_(
                decoded,
                b.builtin(
                    F::UnConstrData,
                    &[b.var(encoded.name, encoded.ty)],
                    decoded.ty,
                ),
                body,
            );
            let body = b.let_(
                encoded,
                b.builtin(
                    F::ConstrData,
                    &[b.var(tag.name, tag.ty), b.var(fields.name, fields.ty)],
                    encoded.ty,
                ),
                body,
            );
            let body = b.let_(
                fields,
                b.builtin(
                    F::MkCons,
                    &[
                        trace("field", data),
                        b.lit(C::proto_list(a, &Type::Data, &[])),
                    ],
                    fields_ty,
                ),
                body,
            );
            let tag_value = if bad_tag {
                let bytes = b.lit(C::data(a, D::byte_string(a, b"bad")));
                b.with_type(
                    b.builtin(
                        F::UnBData,
                        &[trace("tag", bytes)],
                        b.lit(C::byte_string(a, b"bad")).ty,
                    ),
                    int,
                )
            } else {
                let tag_data = b.lit(C::data(a, D::integer_from(a, 0)));
                b.builtin(F::UnIData, &[trace("tag", tag_data)], int)
            };
            let body = b.let_(tag, tag_value, body);
            cases.push((
                format!(
                    "roundtrip_{}_{}",
                    if bad_tag { "bad_tag" } else { "valid" },
                    if second { "second" } else { "first" }
                ),
                body,
                bad_tag,
            ));
        }
    }
    cases
}
