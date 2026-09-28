//! Isolated literal Data-shape case folding with accepted-pipeline comparisons.
use nash_ir::{
    anf,
    build::Builder,
    core::*,
    hygiene, known_case,
    pretty::pretty,
    ty::{ConstTy, Ty},
};
use nash_plutus::{
    arena::Arena, builtin::DefaultFunction as F, constant::Constant, data::PlutusData,
};
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
const DATA: Ty<'static> = Ty::Big(&nash_ir::ty::BigTy::Data);
fn bind<'a>(b: &Builder<'a>, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty,
    }
}
fn trace<'a>(b: &Builder<'a>, label: &'a str, value: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, label)), value)
}
fn arm<'a>(
    b: &Builder<'a>,
    test: Test<'a>,
    binders: &[Binder<'a>],
    body: &'a Core<'a>,
) -> Branch<'a> {
    Branch {
        test,
        binders: b.arena.alloc_slice_copy(binders),
        body,
    }
}
fn check(name: &str, b: &Builder<'_>, original: &Core<'_>, fails: bool) {
    let before = anf::normalize(b, original);
    let folded = known_case::reduce_data(b, before);
    let after = nash_ir::small_inline::simplify(b, folded);
    let after = known_case::simplify_data(b, after);
    let left = crate::harness::eval_core_raw(b.arena, before);
    let middle = crate::harness::eval_core_raw(b.arena, folded);
    let right = crate::harness::eval_core_raw(b.arena, after);
    assert_eq!(right.result.starts_with("error:"), fails);
    insta::assert_snapshot!(
        name,
        crate::harness::pass_snapshot(
            b.arena,
            original,
            format!(
                "--- isolated ANF input\n{}\n--- isolated folded Core\n{}\n--- isolated cleaned Core\n{}\n--- isolated UPLC before\n{}\n--- isolated UPLC after\n{}\n--- result\n{}\n--- logs\n{:?}",
                pretty(before),
                pretty(folded),
                pretty(after),
                left.uplc,
                right.uplc,
                right.result,
                right.logs
            )
        )
    );
    for core in [before, folded, after] {
        hygiene::validate(core, &[]).unwrap();
        assert_eq!(original.ty, core.ty);
    }
    anf::validate(after).unwrap();
    for actual in [middle, right] {
        assert_eq!(left.observable, actual.observable);
        assert_eq!(left.logs, actual.logs);
    }
    assert!(std::ptr::eq(after, known_case::simplify_data(b, after)));
}
fn literal<'a>(b: &Builder<'a>, data: &'a PlutusData<'a>) -> &'a Core<'a> {
    b.lit(Constant::data(b.arena, data))
}
fn shapes<'a>(a: &'a Arena) -> Vec<(&'static str, Test<'a>, &'a PlutusData<'a>, Ty<'a>)> {
    let item = PlutusData::integer_from(a, 42);
    let data_list = Ty::Const(a.alloc(ConstTy::List(DATA)));
    let data_pair = Ty::Const(a.alloc(ConstTy::Pair(DATA, DATA)));
    let map = Ty::Const(a.alloc(ConstTy::List(data_pair)));
    let constr = Ty::Const(a.alloc(ConstTy::Pair(INT, data_list)));
    vec![
        ("integer", Test::DataI, item, INT),
        (
            "bytes",
            Test::DataB,
            PlutusData::byte_string(a, b"nash"),
            Ty::Const(&ConstTy::Bytes),
        ),
        (
            "empty_list",
            Test::DataList,
            PlutusData::list(a, &[]),
            data_list,
        ),
        (
            "list",
            Test::DataList,
            PlutusData::list(a, a.alloc_slice_copy(&[item])),
            data_list,
        ),
        ("empty_map", Test::DataMap, PlutusData::map(a, &[]), map),
        (
            "map",
            Test::DataMap,
            PlutusData::map(
                a,
                a.alloc_slice_copy(&[(item, item), (item, PlutusData::integer_from(a, 9))]),
            ),
            map,
        ),
        (
            "nullary_constr",
            Test::DataConstr,
            PlutusData::constr(a, 59, &[]),
            constr,
        ),
        (
            "constr",
            Test::DataConstr,
            PlutusData::constr(a, 122, a.alloc_slice_copy(&[item])),
            constr,
        ),
    ]
}
#[test]
fn all_shapes_bind_native_payload_and_ignore_cold_default() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (name, test, data, ty) in shapes(&a) {
        let payload = bind(&b, "payload", ty);
        check(
            name,
            &b,
            b.case(
                CaseKind::Data,
                literal(&b, data),
                &[arm(&b, test, &[payload], b.var(payload.name, ty))],
                Some(trace(&b, "cold", b.error(ty))),
                ty,
            ),
            false,
        );
    }
}
#[test]
fn missing_shapes_use_default_or_fail() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (name, _, data, _) in shapes(&a) {
        check(
            &format!("{name}_default"),
            &b,
            b.case(
                CaseKind::Data,
                literal(&b, data),
                &[],
                Some(trace(&b, "fallback", b.int(9))),
                INT,
            ),
            false,
        );
        check(
            &format!("{name}_missing"),
            &b,
            b.case(CaseKind::Data, literal(&b, data), &[], None, INT),
            true,
        );
    }
}
#[test]
fn aliases_repeated_payloads_and_escaping_original() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (name, test, data, ty) in shapes(&a) {
        let value = bind(&b, "data", DATA);
        let alias = bind(&b, "alias", DATA);
        let branches: Vec<_> = (0..2)
            .map(|_| {
                let p = bind(&b, "payload", ty);
                b.case(
                    CaseKind::Data,
                    b.var(alias.name, DATA),
                    &[arm(&b, test, &[p], b.var(p.name, ty))],
                    None,
                    ty,
                )
            })
            .collect();
        let result = b.constr(
            0,
            &[b.var(value.name, DATA), branches[0], branches[1]],
            Ty::Erased,
        );
        check(
            &format!("shared_{name}"),
            &b,
            b.let_(
                value,
                literal(&b, data),
                b.let_(alias, b.var(value.name, DATA), result),
            ),
            false,
        );
    }
}
#[test]
fn strict_effects_survive_and_cold_branch_stays_cold() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let effect = bind(&b, "effect", INT);
    let value = bind(&b, "data", DATA);
    let p = bind(&b, "payload", INT);
    let c = b.case(
        CaseKind::Data,
        b.var(value.name, DATA),
        &[arm(
            &b,
            Test::DataI,
            &[p],
            trace(&b, "selected", b.var(p.name, INT)),
        )],
        Some(trace(&b, "cold", b.error(INT))),
        INT,
    );
    check(
        "strict_trace",
        &b,
        b.let_(
            effect,
            trace(&b, "before", b.int(0)),
            b.let_(value, literal(&b, PlutusData::integer_from(&a, 42)), c),
        ),
        false,
    );
    check(
        "strict_failure",
        &b,
        b.let_(
            effect,
            trace(&b, "before", b.error(INT)),
            b.let_(value, literal(&b, PlutusData::integer_from(&a, 42)), c),
        ),
        true,
    );
}
#[test]
fn returned_functions_and_delays_capture_payload() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let p = bind(&b, "payload", INT);
    let x = bind(&b, "x", INT);
    let body = b.builtin(
        F::AddInteger,
        &[b.var(p.name, INT), b.var(x.name, INT)],
        INT,
    );
    let function = b.lam(&[x], body);
    let c = b.case(
        CaseKind::Data,
        literal(&b, PlutusData::integer_from(&a, 40)),
        &[arm(&b, Test::DataI, &[p], function)],
        None,
        function.ty,
    );
    check("returned_function", &b, b.app(c, &[b.int(2)], INT), false);
    let delay = b.delay(trace(&b, "forced", b.var(p.name, INT)));
    let c = b.case(
        CaseKind::Data,
        literal(&b, PlutusData::integer_from(&a, 42)),
        &[arm(&b, Test::DataI, &[p], delay)],
        None,
        delay.ty,
    );
    check("returned_delay", &b, b.force(c, INT), false);
}
#[test]
fn list_payload_exposes_nested_data_case() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let list_ty = Ty::Const(&ConstTy::List(DATA));
    let xs = bind(&b, "xs", list_ty);
    let h = bind(&b, "head", DATA);
    let t = bind(&b, "tail", list_ty);
    let n = bind(&b, "number", INT);
    let number = b.case(
        CaseKind::Data,
        b.var(h.name, DATA),
        &[arm(&b, Test::DataI, &[n], b.var(n.name, INT))],
        None,
        INT,
    );
    let head = b.case(
        CaseKind::List,
        b.var(xs.name, list_ty),
        &[arm(&b, Test::Cons, &[h, t], number)],
        None,
        INT,
    );
    let data = PlutusData::list(&a, a.alloc_slice_copy(&[PlutusData::integer_from(&a, 42)]));
    check(
        "nested_data_list",
        &b,
        b.case(
            CaseKind::Data,
            literal(&b, data),
            &[arm(&b, Test::DataList, &[xs], head)],
            None,
            INT,
        ),
        false,
    );
}
#[test]
fn unknown_traced_builtin_and_wrong_runtime_subjects_are_not_facts() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let n = bind(&b, "number", INT);
    for (name, value, fails) in [
        (
            "traced_subject",
            trace(&b, "subject", literal(&b, PlutusData::integer_from(&a, 42))),
            false,
        ),
        (
            "idata_subject",
            b.builtin(F::IData, &[b.int(42)], DATA),
            false,
        ),
        (
            "invalid_idata",
            b.builtin(F::IData, &[b.lit(Constant::bool(&a, true))], DATA),
            true,
        ),
        ("wrong_subject", b.with_type(b.int(42), DATA), true),
    ] {
        check(
            name,
            &b,
            b.case(
                CaseKind::Data,
                value,
                &[arm(&b, Test::DataI, &[n], b.var(n.name, INT))],
                None,
                INT,
            ),
            fails,
        );
    }
}
#[test]
fn malformed_tables_remain_lowering_errors() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let p = bind(&b, "p", INT);
    let valid = arm(&b, Test::DataI, &[p], b.int(42));
    for (name, branches) in [
        ("duplicate_shape", vec![valid, valid]),
        (
            "unselected_bad_arity",
            vec![valid, arm(&b, Test::DataB, &[], b.int(9))],
        ),
        (
            "no_payload_binder",
            vec![arm(&b, Test::DataI, &[], b.int(42))],
        ),
        (
            "extra_payload_binder",
            vec![arm(&b, Test::DataI, &[p, p], b.int(42))],
        ),
        (
            "wrong_test",
            vec![valid, arm(&b, Test::True, &[], b.int(9))],
        ),
    ] {
        let before = b.case(
            CaseKind::Data,
            literal(&b, PlutusData::integer_from(&a, 42)),
            &branches,
            None,
            INT,
        );
        let after = known_case::reduce_data(&b, before);
        let left = crate::lower::lower(&a, before).unwrap_err();
        let right = crate::lower::lower(&a, after).unwrap_err();
        insta::assert_snapshot!(
            name,
            format!(
                "--- before\n{}\n--- after\n{}\n--- errors\n{left:?}\n{right:?}",
                pretty(before),
                pretty(after)
            )
        );
        assert!(std::ptr::eq(before, after));
    }
}

#[test]
fn complete_table_selects_each_shape_with_unused_payload() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let shapes = shapes(&a);
    for (name, _, data, _) in &shapes {
        let branches: Vec<_> = [0, 1, 2, 4, 6]
            .iter()
            .map(|&index| {
                let (_, test, _, ty) = shapes[index];
                let p = bind(&b, "unused", ty);
                arm(&b, test, &[p], trace(&b, name, b.int(index as i128)))
            })
            .collect();
        check(
            &format!("complete_{name}"),
            &b,
            b.case(
                CaseKind::Data,
                literal(&b, data),
                &branches,
                Some(b.error(INT)),
                INT,
            ),
            false,
        );
    }
}
#[test]
fn large_integer_and_constructor_tag_are_not_narrowed() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let integer = a.alloc_integer("-340282366920938463463374607431768211457".parse().unwrap());
    let p = bind(&b, "integer", INT);
    check(
        "large_signed_integer",
        &b,
        b.case(
            CaseKind::Data,
            literal(&b, PlutusData::integer(&a, integer)),
            &[arm(&b, Test::DataI, &[p], b.var(p.name, INT))],
            None,
            INT,
        ),
        false,
    );
    let ty = Ty::Const(&ConstTy::Pair(INT, Ty::Const(&ConstTy::List(DATA))));
    let p = bind(&b, "constr", ty);
    check(
        "max_constructor_tag",
        &b,
        b.case(
            CaseKind::Data,
            literal(&b, PlutusData::constr(&a, u64::MAX, &[])),
            &[arm(&b, Test::DataConstr, &[p], b.var(p.name, ty))],
            None,
            ty,
        ),
        false,
    );
}

#[test]
fn captured_original_unknown_parameter_and_result_type_view() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let data = bind(&b, "data", DATA);
    let payload = bind(&b, "payload", INT);
    let c = b.case(
        CaseKind::Data,
        b.var(data.name, DATA),
        &[arm(&b, Test::DataI, &[payload], b.var(payload.name, INT))],
        None,
        INT,
    );
    let delayed = b.delay(c);
    check(
        "captured_original",
        &b,
        b.let_(
            data,
            literal(&b, PlutusData::integer_from(&a, 42)),
            b.force(delayed, INT),
        ),
        false,
    );
    let function = b.lam(&[data], c);
    check(
        "unknown_parameter",
        &b,
        b.app(
            function,
            &[literal(&b, PlutusData::integer_from(&a, 42))],
            INT,
        ),
        false,
    );
    let c = b.case(
        CaseKind::Data,
        literal(&b, PlutusData::integer_from(&a, 42)),
        &[arm(&b, Test::DataI, &[payload], b.int(9))],
        None,
        Ty::Erased,
    );
    check("result_type_view", &b, c, false);
}
