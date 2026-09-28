//! Direct native-constructor case folding, including isolated-pass evidence.
use nash_ir::{
    build::Builder,
    core::*,
    hygiene, known_case,
    pretty::pretty,
    ty::{ConstTy, RuntimeTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};

const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
fn bind<'a>(b: &Builder<'a>, label: &'a str) -> Binder<'a> {
    Binder {
        name: b.fresh(label),
        ty: INT,
    }
}
fn trace<'a>(b: &Builder<'a>, label: &'a str, value: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, label)), value)
}
fn constr<'a>(b: &Builder<'a>, tag: u16, fields: &[&'a Core<'a>]) -> &'a Core<'a> {
    let types: Vec<_> = fields.iter().map(|f| f.ty).collect();
    let ty = Ty::Runtime(b.arena.alloc(RuntimeTy::Constr {
        tag,
        fields: b.arena.alloc_slice_copy(&types),
    }));
    b.constr(tag, fields, ty)
}
fn arm<'a>(b: &Builder<'a>, tag: u16, binders: &[Binder<'a>], body: &'a Core<'a>) -> Branch<'a> {
    Branch {
        test: Test::Tag(tag),
        binders: b.arena.alloc_slice_copy(binders),
        body,
    }
}
fn check(name: &str, b: &Builder<'_>, before: &Core<'_>, fails: bool) {
    let after = known_case::reduce_constr(b, before);
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
    hygiene::validate(before, &[]).unwrap();
    hygiene::validate(after, &[]).unwrap();
    assert_eq!(before.ty, after.ty);
    assert_eq!(left.observable, right.observable);
    assert_eq!(left.logs, right.logs);
    assert!(std::ptr::eq(after, known_case::reduce_constr(b, after)));
}

#[test]
fn selects_tag_with_fields_in_source_order() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = bind(&b, "x");
    let y = bind(&b, "y");
    let body = trace(
        &b,
        "body",
        b.builtin(
            F::SubtractInteger,
            &[b.var(x.name, INT), b.var(y.name, INT)],
            INT,
        ),
    );
    let arms = [
        arm(&b, 1, &[x, y], body),
        arm(&b, 0, &[], trace(&b, "cold", b.error(INT))),
    ];
    check(
        "field_order",
        &b,
        b.case(
            CaseKind::Tag,
            constr(
                &b,
                1,
                &[trace(&b, "first", b.int(50)), trace(&b, "second", b.int(8))],
            ),
            &arms,
            None,
            INT,
        ),
        false,
    );
}

#[test]
fn nullary_and_expanded_wildcard_branches() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (tag, name) in [
        (0, "nullary"),
        (1, "wildcard_one_field"),
        (2, "wildcard_two_fields"),
    ] {
        let x = bind(&b, "ignored");
        let y = bind(&b, "ignored");
        let z = bind(&b, "ignored");
        let arms = [
            arm(&b, 0, &[], b.int(42)),
            arm(&b, 1, &[x], b.int(42)),
            arm(&b, 2, &[y, z], b.int(42)),
        ];
        let fields: Vec<_> = (0..tag).map(|_| trace(&b, "field", b.int(0))).collect();
        check(
            name,
            &b,
            b.case(CaseKind::Tag, constr(&b, tag, &fields), &arms, None, INT),
            false,
        );
    }
}

#[test]
fn ignored_failing_field_still_fails_before_later_fields_and_body() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = bind(&b, "ignored");
    let y = bind(&b, "ignored");
    check(
        "ignored_failure",
        &b,
        b.case(
            CaseKind::Tag,
            constr(
                &b,
                0,
                &[
                    trace(&b, "first", b.error(INT)),
                    trace(&b, "second", b.int(0)),
                ],
            ),
            &[arm(&b, 0, &[x, y], trace(&b, "body", b.int(42)))],
            None,
            INT,
        ),
        true,
    );
}

#[test]
fn returned_function_and_delay_capture_evaluated_fields() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (delayed, name) in [(false, "returned_function"), (true, "returned_delay")] {
        let x = bind(&b, "x");
        let p = bind(&b, "p");
        let body = trace(&b, "body", b.var(x.name, INT));
        let result = if delayed {
            b.delay(body)
        } else {
            b.lam(&[p], body)
        };
        let case = b.case(
            CaseKind::Tag,
            constr(&b, 0, &[trace(&b, "field", b.int(42))]),
            &[arm(&b, 0, &[x], result)],
            None,
            result.ty,
        );
        let invoke = if delayed {
            b.force(case, INT)
        } else {
            b.app(case, &[trace(&b, "argument", b.int(0))], INT)
        };
        check(name, &b, invoke, false);
    }
}

#[test]
fn nested_cases_and_cold_construction() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let inner = b.case(
        CaseKind::Tag,
        constr(&b, 0, &[]),
        &[arm(&b, 0, &[], b.int(42))],
        None,
        INT,
    );
    check(
        "nested",
        &b,
        b.case(
            CaseKind::Tag,
            constr(&b, 0, &[]),
            &[arm(&b, 0, &[], inner)],
            None,
            INT,
        ),
        false,
    );
    let x = bind(&b, "ignored");
    let cold = b.case(
        CaseKind::Tag,
        constr(&b, 0, &[trace(&b, "cold", b.error(INT))]),
        &[arm(&b, 0, &[x], b.int(0))],
        None,
        INT,
    );
    check(
        "cold",
        &b,
        b.if_(b.lit(Constant::bool(&a, true)), b.int(42), cold),
        false,
    );
}

#[test]
fn unknown_tag_and_arity_mismatch_are_unchanged() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = bind(&b, "x");
    let cases = [
        (
            "unknown_tag",
            constr(&b, 1, &[trace(&b, "field", b.int(0))]),
            arm(&b, 0, &[], b.int(42)),
        ),
        (
            "extra_field",
            constr(&b, 0, &[b.int(0)]),
            arm(&b, 0, &[], b.int(42)),
        ),
    ];
    for (name, subject, branch) in cases {
        let c = b.case(CaseKind::Tag, subject, &[branch], None, INT);
        check(name, &b, c, true);
        assert!(std::ptr::eq(c, known_case::reduce_constr(&b, c)));
    }
    // Too few fields returns a lambda in UPLC; explicitly apply it for evaluation.
    let c = b.case(
        CaseKind::Tag,
        constr(&b, 0, &[]),
        &[arm(&b, 0, &[x], b.var(x.name, INT))],
        None,
        b.lam(&[x], b.var(x.name, INT)).ty,
    );
    check("missing_field", &b, b.app(c, &[b.int(42)], INT), false);
    assert!(std::ptr::eq(c, known_case::reduce_constr(&b, c)));
}

#[test]
fn invalid_tables_remain_lowering_errors() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let good = arm(&b, 0, &[], b.int(42));
    for (name, branches, default) in [
        ("sparse", vec![arm(&b, 59, &[], b.int(42))], None),
        ("duplicate", vec![good, good], None),
        (
            "wrong_test",
            vec![Branch {
                test: Test::True,
                ..good
            }],
            None,
        ),
        ("unexpanded_default", vec![good], Some(b.int(0))),
    ] {
        let c = b.case(CaseKind::Tag, constr(&b, 0, &[]), &branches, default, INT);
        let after = known_case::reduce_constr(&b, c);
        let left = crate::lower::lower(&a, c).unwrap_err();
        let right = crate::lower::lower(&a, after).unwrap_err();
        insta::assert_snapshot!(
            name,
            format!(
                "--- before\n{}\n--- after\n{}\n--- errors\n{left:?}\n{right:?}",
                pretty(c),
                pretty(after)
            )
        );
        assert!(std::ptr::eq(c, after));
    }
}

#[test]
fn empty_table_and_effectful_subject_are_unchanged() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let empty = b.case(CaseKind::Tag, constr(&b, 0, &[]), &[], None, INT);
    check("empty_table", &b, empty, true);
    assert!(std::ptr::eq(empty, known_case::reduce_constr(&b, empty)));
    let subject = trace(&b, "subject", constr(&b, 0, &[]));
    let c = b.case(
        CaseKind::Tag,
        subject,
        &[arm(&b, 0, &[], b.int(42))],
        None,
        INT,
    );
    check("effectful_subject", &b, c, false);
    assert!(std::ptr::eq(c, known_case::reduce_constr(&b, c)));
}

// Retain isolated evidence for bound-constructor folding and its cleanup.
fn check_bound(name: &str, b: &Builder<'_>, original: &Core<'_>, fails: bool) {
    let before = nash_ir::anf::normalize(b, original);
    let folded = known_case::reduce_bound_constr(b, before);
    let after = nash_ir::small_inline::simplify(b, folded);
    let after = known_case::simplify_bound_constr(b, after);
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
                "--- trial ANF input\n{}\n--- trial folded Core\n{}\n--- trial cleaned Core\n{}\n--- trial UPLC before\n{}\n--- trial UPLC after\n{}\n--- result\n{}\n--- logs\n{:?}",
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
    for candidate in [before, folded, after] {
        hygiene::validate(candidate, &[]).unwrap();
        assert_eq!(original.ty, candidate.ty);
    }
    for result in [middle, right] {
        assert_eq!(left.observable, result.observable);
        assert_eq!(left.logs, result.logs);
    }
    nash_ir::anf::validate(before).unwrap();
    nash_ir::anf::validate(after).unwrap();
}

fn bound_subject<'a>(b: &Builder<'a>, value: &'a Core<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh("subject"),
        ty: value.ty,
    }
}

#[test]
fn bound_aliases_repeated_matches_and_escaping_value() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (escape, name) in [(false, "bound_alias_repeated"), (true, "bound_escape")] {
        let value = constr(&b, 0, &[trace(&b, "field", b.int(21))]);
        let subject = bound_subject(&b, value);
        let alias = bound_subject(&b, value);
        let alias2 = bound_subject(&b, value);
        let x = bind(&b, "x");
        let y = bind(&b, "y");
        let first = bind(&b, "first");
        let second = bind(&b, "second");
        let [match_first, match_second] = [(alias2, x), (subject, y)].map(|(s, field)| {
            b.case(
                CaseKind::Tag,
                b.var(s.name, s.ty),
                &[arm(&b, 0, &[field], b.var(field.name, field.ty))],
                None,
                INT,
            )
        });
        let sum = b.builtin(
            F::AddInteger,
            &[b.var(first.name, INT), b.var(second.name, INT)],
            INT,
        );
        let result = if escape {
            constr(&b, 0, &[b.var(subject.name, subject.ty), sum])
        } else {
            sum
        };
        let body = b.let_(first, match_first, b.let_(second, match_second, result));
        let original = b.let_(
            subject,
            value,
            b.let_(
                alias,
                b.var(subject.name, subject.ty),
                b.let_(alias2, b.var(alias.name, alias.ty), body),
            ),
        );
        check_bound(name, &b, original, false);
    }
}

#[test]
fn bound_fields_remain_strict_before_cold_match() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (fail, name) in [
        (false, "bound_cold_fields"),
        (true, "bound_ignored_failure"),
    ] {
        let value = constr(
            &b,
            0,
            &[
                trace(&b, "first", if fail { b.error(INT) } else { b.int(7) }),
                trace(&b, "second", b.int(8)),
            ],
        );
        let subject = bound_subject(&b, value);
        let x = bind(&b, "ignored");
        let y = bind(&b, "ignored");
        let matched = b.case(
            CaseKind::Tag,
            b.var(subject.name, subject.ty),
            &[arm(&b, 0, &[x, y], trace(&b, "cold", b.int(0)))],
            None,
            INT,
        );
        let body = b.if_(
            b.lit(Constant::bool(&a, false)),
            matched,
            trace(&b, "chosen", b.int(42)),
        );
        check_bound(name, &b, b.let_(subject, value, body), fail);
    }
}

#[test]
fn bound_function_and_delay_fields_are_named_once() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (delayed, name) in [(false, "bound_function_field"), (true, "bound_delay_field")] {
        let p = bind(&b, "parameter");
        let field = if delayed {
            // Include an internal binder even in the delay.
            b.delay(b.let_(p, b.int(21), trace(&b, "called", b.var(p.name, INT))))
        } else {
            b.lam(&[p], trace(&b, "called", b.var(p.name, INT)))
        };
        let value = constr(&b, 0, &[field]);
        let subject = bound_subject(&b, value);
        let x = Binder {
            name: b.fresh("x"),
            ty: field.ty,
        };
        let y = Binder {
            name: b.fresh("y"),
            ty: field.ty,
        };
        let matches = [x, y].map(|field| {
            let invoke = if delayed {
                b.force(b.var(field.name, field.ty), INT)
            } else {
                b.app(b.var(field.name, field.ty), &[b.int(21)], INT)
            };
            b.case(
                CaseKind::Tag,
                b.var(subject.name, subject.ty),
                &[arm(&b, 0, &[field], invoke)],
                None,
                INT,
            )
        });
        let body = b.builtin(F::AddInteger, &matches, INT);
        check_bound(name, &b, b.let_(subject, value, body), false);
    }
}

#[test]
fn bound_constructor_captured_by_function_and_delay() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (delayed, name) in [
        (false, "bound_function_capture"),
        (true, "bound_delay_capture"),
    ] {
        let value = constr(&b, 0, &[trace(&b, "field", b.int(42))]);
        let subject = bound_subject(&b, value);
        let x = bind(&b, "x");
        let p = bind(&b, "ignored");
        let matched = b.case(
            CaseKind::Tag,
            b.var(subject.name, subject.ty),
            &[arm(&b, 0, &[x], b.var(x.name, INT))],
            None,
            INT,
        );
        let body = if delayed {
            b.force(b.delay(matched), INT)
        } else {
            b.app(b.lam(&[p], matched), &[b.int(0)], INT)
        };
        check_bound(name, &b, b.let_(subject, value, body), false);
    }
}

#[test]
fn bound_nullary_reordered_and_missing_tags() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for (tag, name, fails) in [
        (0, "bound_nullary", false),
        (1, "bound_reordered", false),
        (2, "bound_missing_tag", true),
    ] {
        let value = constr(&b, tag, &[]);
        let subject = bound_subject(&b, value);
        let matched = b.case(
            CaseKind::Tag,
            b.var(subject.name, subject.ty),
            &[arm(&b, 1, &[], b.int(42)), arm(&b, 0, &[], b.int(21))],
            None,
            INT,
        );
        check_bound(name, &b, b.let_(subject, value, matched), fails);
    }
}

#[test]
fn bound_invalid_tables_are_unchanged() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let good = arm(&b, 0, &[], b.int(42));
    for (name, branches, default) in [
        ("bound_sparse", vec![arm(&b, 59, &[], b.int(42))], None),
        ("bound_duplicate", vec![good, good], None),
        (
            "bound_wrong_test",
            vec![Branch {
                test: Test::True,
                ..good
            }],
            None,
        ),
        ("bound_default", vec![good], Some(b.int(0))),
    ] {
        let value = constr(&b, 0, &[]);
        let subject = bound_subject(&b, value);
        let before = b.let_(
            subject,
            value,
            b.case(
                CaseKind::Tag,
                b.var(subject.name, subject.ty),
                &branches,
                default,
                INT,
            ),
        );
        let after = known_case::reduce_bound_constr(&b, before);
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
fn bound_nested_constructor_facts_exposed_by_cleanup() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let inner = constr(&b, 0, &[trace(&b, "field", b.int(42))]);
    let inner_name = bound_subject(&b, inner);
    let outer = constr(&b, 0, &[b.var(inner_name.name, inner_name.ty)]);
    let outer_name = bound_subject(&b, outer);
    let selected = bound_subject(&b, inner);
    let x = bind(&b, "x");
    let inner_case = b.case(
        CaseKind::Tag,
        b.var(selected.name, selected.ty),
        &[arm(&b, 0, &[x], b.var(x.name, INT))],
        None,
        INT,
    );
    let outer_case = b.case(
        CaseKind::Tag,
        b.var(outer_name.name, outer_name.ty),
        &[arm(&b, 0, &[selected], inner_case)],
        None,
        INT,
    );
    check_bound(
        "bound_nested",
        &b,
        b.let_(inner_name, inner, b.let_(outer_name, outer, outer_case)),
        false,
    );
}

#[test]
fn bound_empty_table_and_arity_mismatch_remain_errors() {
    let a = Arena::new();
    let b = Builder::new(&a);
    for empty in [false, true] {
        let value = constr(&b, 0, &[b.int(21), b.int(22)]);
        let subject = bound_subject(&b, value);
        let x = bind(&b, "x");
        let branches = [arm(&b, 0, &[x], b.var(x.name, INT))];
        let matched = b.case(
            CaseKind::Tag,
            b.var(subject.name, subject.ty),
            if empty { &[] } else { &branches },
            None,
            INT,
        );
        check_bound(
            if empty { "bound_empty" } else { "bound_arity" },
            &b,
            b.let_(subject, value, matched),
            true,
        );
    }
}
