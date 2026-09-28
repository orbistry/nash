//! Isolated native-list case folding with accepted-pipeline comparisons.
use nash_ir::{
    anf,
    build::Builder,
    core::*,
    hygiene, known_case,
    pretty::pretty,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant, typ::Type};
const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
const LIST: Ty<'static> = Ty::Const(&ConstTy::List(INT));
fn bind<'a>(b: &Builder<'a>, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty,
    }
}
fn list<'a>(b: &Builder<'a>, values: &[i128]) -> &'a Core<'a> {
    let values: Vec<_> = values
        .iter()
        .map(|&v| Constant::integer_from(b.arena, v))
        .collect();
    b.lit(Constant::proto_list(
        b.arena,
        &Type::Integer,
        b.arena.alloc_slice_copy(&values),
    ))
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
    let folded = known_case::reduce_list(b, before);
    let after = nash_ir::small_inline::simplify(b, folded);
    let after = known_case::simplify_list(b, after);
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
    assert!(std::ptr::eq(after, known_case::simplify_list(b, after)));
}
#[test]
fn literal_lists_select_nil_cons_and_tail() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (values, name) in [
        (&[][..], "empty"),
        (&[42][..], "singleton"),
        (&[42, 7, 9][..], "nonempty"),
    ] {
        let head = bind(&b, "head", INT);
        let tail = bind(&b, "tail", LIST);
        let c = b.case(
            CaseKind::List,
            list(&b, values),
            &[
                arm(
                    &b,
                    Test::Cons,
                    &[head, tail],
                    trace(&b, "cons", b.var(tail.name, LIST)),
                ),
                arm(&b, Test::Nil, &[], trace(&b, "nil", list(&b, &[0]))),
            ],
            None,
            LIST,
        );
        check(name, &b, c, false);
    }
}
#[test]
fn missing_arms_use_fallback_or_fail() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (values, name, default) in [
        (&[][..], "empty_default", true),
        (&[42][..], "cons_default", true),
        (&[][..], "empty_missing", false),
        (&[42][..], "cons_missing", false),
    ] {
        let fallback = default.then(|| trace(&b, "fallback", b.int(42)));
        check(
            name,
            &b,
            b.case(CaseKind::List, list(&b, values), &[], fallback, INT),
            !default,
        );
    }
}
#[test]
fn explicit_arm_wins_over_failing_default() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let h = bind(&b, "head", INT);
    let t = bind(&b, "tail", LIST);
    let c = b.case(
        CaseKind::List,
        list(&b, &[42]),
        &[arm(&b, Test::Cons, &[h, t], b.var(h.name, INT))],
        Some(trace(&b, "cold", b.error(INT))),
        INT,
    );
    check("explicit_wins", &b, c, false);
}
#[test]
fn mkcons_preserves_strict_head_tail_and_runtime_checks() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (mode, name) in [
        (0, "strict_order"),
        (1, "head_failure"),
        (2, "tail_failure"),
        (3, "wrong_tail"),
        (4, "wrong_head_type"),
        (5, "function_head"),
        (6, "delay_head"),
    ] {
        let p = bind(&b, "p", INT);
        let head = match mode {
            1 => b.error(INT),
            4 => b.lit(Constant::bool(&a, true)),
            5 => b.lam(&[p], b.var(p.name, INT)),
            6 => b.delay(b.let_(p, b.int(42), b.var(p.name, INT))),
            _ => b.int(42),
        };
        let tail = match mode {
            2 => b.error(LIST),
            3 => b.int(0),
            _ => list(&b, &[7]),
        };
        let xs = b.builtin(
            F::MkCons,
            &[
                if mode >= 5 {
                    head
                } else {
                    trace(&b, "head", head)
                },
                trace(&b, "tail", tail),
            ],
            LIST,
        );
        let h = bind(&b, "ignored", INT);
        let t = bind(&b, "ignored", LIST);
        let c = b.case(
            CaseKind::List,
            xs,
            &[
                arm(&b, Test::Nil, &[], trace(&b, "cold", b.error(INT))),
                arm(&b, Test::Cons, &[h, t], trace(&b, "body", b.int(21))),
            ],
            None,
            INT,
        );
        check(name, &b, c, mode != 0);
    }
}
#[test]
fn aliases_repeated_matches_and_escaping_list() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (literal, name) in [(false, "bound_mkcons"), (true, "bound_literal")] {
        let value = if literal {
            list(&b, &[21, 7])
        } else {
            b.builtin(
                F::MkCons,
                &[trace(&b, "head", b.int(21)), list(&b, &[7])],
                LIST,
            )
        };
        let xs = bind(&b, "xs", LIST);
        let alias = bind(&b, "alias", LIST);
        let alias2 = bind(&b, "alias2", LIST);
        let cases = [0, 1].map(|_| {
            let h = bind(&b, "head", INT);
            let t = bind(&b, "tail", LIST);
            b.case(
                CaseKind::List,
                b.var(alias2.name, LIST),
                &[arm(&b, Test::Cons, &[h, t], b.var(h.name, INT))],
                None,
                INT,
            )
        });
        let sum = b.builtin(F::AddInteger, &cases, INT);
        let body = b.builtin(F::MkCons, &[sum, b.var(xs.name, LIST)], LIST);
        check(
            name,
            &b,
            b.let_(
                xs,
                value,
                b.let_(
                    alias,
                    b.var(xs.name, LIST),
                    b.let_(alias2, b.var(alias.name, LIST), body),
                ),
            ),
            false,
        );
    }
}
#[test]
fn cold_match_does_not_delay_construction() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let xs = bind(&b, "xs", LIST);
    let value = b.builtin(
        F::MkCons,
        &[
            trace(&b, "head", b.int(42)),
            trace(&b, "tail", list(&b, &[])),
        ],
        LIST,
    );
    let h = bind(&b, "head", INT);
    let t = bind(&b, "tail", LIST);
    let c = b.case(
        CaseKind::List,
        b.var(xs.name, LIST),
        &[arm(
            &b,
            Test::Cons,
            &[h, t],
            trace(&b, "cold", b.var(h.name, INT)),
        )],
        None,
        INT,
    );
    check(
        "cold_match",
        &b,
        b.let_(
            xs,
            value,
            b.if_(
                b.lit(Constant::bool(&a, false)),
                c,
                trace(&b, "chosen", b.int(0)),
            ),
        ),
        false,
    );
}
#[test]
fn nested_literal_tail_match_and_delayed_result() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let h = bind(&b, "head", INT);
    let t = bind(&b, "tail", LIST);
    let h2 = bind(&b, "head", INT);
    let t2 = bind(&b, "tail", LIST);
    let inner = b.case(
        CaseKind::List,
        b.var(t.name, LIST),
        &[arm(&b, Test::Cons, &[h2, t2], b.var(h2.name, INT))],
        None,
        INT,
    );
    let delayed = b.delay(inner);
    let c = b.case(
        CaseKind::List,
        list(&b, &[21, 42]),
        &[arm(&b, Test::Cons, &[h, t], delayed)],
        None,
        delayed.ty,
    );
    check("nested_tail_delay", &b, b.force(c, INT), false);
}
#[test]
fn unknown_subject_is_unchanged() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let value = trace(&b, "subject", list(&b, &[]));
    let c = b.case(
        CaseKind::List,
        value,
        &[arm(&b, Test::Nil, &[], b.int(42))],
        None,
        INT,
    );
    check("unknown_traced_subject", &b, c, false);
}
#[test]
fn malformed_tables_remain_lowering_errors() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = bind(&b, "x", INT);
    let nil = arm(&b, Test::Nil, &[], b.int(42));
    let y = bind(&b, "tail", LIST);
    let cons = arm(&b, Test::Cons, &[x, y], b.int(42));
    for (name, branches) in [
        ("duplicate_nil", vec![nil, nil]),
        ("duplicate_cons", vec![cons, cons]),
        ("nil_binder", vec![arm(&b, Test::Nil, &[x], b.int(42))]),
        ("cons_arity", vec![arm(&b, Test::Cons, &[x], b.int(42))]),
        ("wrong_test", vec![arm(&b, Test::True, &[], b.int(42))]),
    ] {
        let before = b.case(CaseKind::List, list(&b, &[]), &branches, None, INT);
        let after = known_case::reduce_list(&b, before);
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
fn partial_and_oversaturated_mkcons_are_not_facts() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (name, subject) in [
        ("partial_cons", b.builtin(F::MkCons, &[b.int(42)], LIST)),
        (
            "oversaturated_cons",
            b.app(
                b.builtin(F::MkCons, &[b.int(42), list(&b, &[])], LIST),
                &[b.int(9)],
                LIST,
            ),
        ),
    ] {
        check(
            name,
            &b,
            b.case(CaseKind::List, subject, &[], Some(b.int(0)), INT),
            true,
        );
    }
}
#[test]
fn nested_mkcons_tail_enables_second_match() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let xs = b.builtin(
        F::MkCons,
        &[
            b.int(21),
            b.builtin(
                F::MkCons,
                &[trace(&b, "inner", b.int(42)), list(&b, &[])],
                LIST,
            ),
        ],
        LIST,
    );
    let h = bind(&b, "head", INT);
    let t = bind(&b, "tail", LIST);
    let h2 = bind(&b, "head", INT);
    let t2 = bind(&b, "tail", LIST);
    let inner = b.case(
        CaseKind::List,
        b.var(t.name, LIST),
        &[arm(&b, Test::Cons, &[h2, t2], b.var(h2.name, INT))],
        None,
        INT,
    );
    check(
        "nested_mkcons",
        &b,
        b.case(
            CaseKind::List,
            xs,
            &[arm(&b, Test::Cons, &[h, t], inner)],
            None,
            INT,
        ),
        false,
    );
}

#[test]
fn repeated_literal_tails_share_one_derived_value() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let xs = bind(&b, "xs", LIST);
    let value = list(&b, &[21, 42, 7, 9]);
    let cases = [0, 1].map(|_| {
        let h = bind(&b, "head", INT);
        let t = bind(&b, "tail", LIST);
        b.case(
            CaseKind::List,
            b.var(xs.name, LIST),
            &[arm(&b, Test::Cons, &[h, t], b.var(t.name, LIST))],
            None,
            LIST,
        )
    });
    let ty = Ty::Runtime(a.alloc(nash_ir::ty::RuntimeTy::Constr {
        tag: 0,
        fields: a.alloc_slice_copy(&[LIST, LIST, LIST]),
    }));
    let body = b.constr(0, &[b.var(xs.name, LIST), cases[0], cases[1]], ty);
    check("shared_literal_tails", &b, b.let_(xs, value, body), false);
}

#[test]
fn literal_data_tail_preserves_element_metadata() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let values = [
        Constant::data(&a, nash_plutus::data::PlutusData::integer_from(&a, 21)),
        Constant::data(&a, nash_plutus::data::PlutusData::integer_from(&a, 42)),
    ];
    let value = b.lit(Constant::proto_list(
        &a,
        &Type::Data,
        a.alloc_slice_copy(&values),
    ));
    let head = bind(&b, "head", b.lit(values[0]).ty);
    let tail = bind(&b, "tail", value.ty);
    let c = b.case(
        CaseKind::List,
        value,
        &[arm(
            &b,
            Test::Cons,
            &[head, tail],
            b.var(tail.name, tail.ty),
        )],
        None,
        tail.ty,
    );
    check("data_tail_metadata", &b, c, false);
}
