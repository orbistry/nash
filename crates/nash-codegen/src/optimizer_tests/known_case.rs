//! Known-case folding semantics with accepted-pipeline and isolated-pass evidence.

mod boolean {
    //! Known Boolean case semantics, independent of the accepted pipeline.
    use nash_ir::{
        build::Builder,
        core::*,
        hygiene, known_case,
        pretty::pretty,
        ty::{ConstTy, Ty},
    };
    use nash_plutus::{arena::Arena, constant::Constant};
    const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
    fn trace<'a>(b: &Builder<'a>, s: &'a str, x: &'a Core<'a>) -> &'a Core<'a> {
        b.trace(b.lit(Constant::string(b.arena, s)), x)
    }
    fn check<'a>(name: &str, b: &Builder<'a>, before: &'a Core<'a>, fails: bool) -> &'a Core<'a> {
        let after = known_case::reduce_bool(b, before);

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
        assert_eq!(left.observable, right.observable);
        assert_eq!(left.logs, right.logs);
        after
    }
    #[test]
    fn known_true_false_and_unselected_effects() {
        let a = Arena::new();
        let b = Builder::new(&a);
        for (value, name) in [(true, "true"), (false, "false")] {
            let good = trace(&b, "chosen", b.int(42));
            let bad = trace(&b, "wrong", b.error(INT));
            check(
                name,
                &b,
                b.if_(
                    b.lit(Constant::bool(&a, value)),
                    if value { good } else { bad },
                    if value { bad } else { good },
                ),
                false,
            );
        }
        check(
            "chosen_failure",
            &b,
            b.if_(
                b.lit(Constant::bool(&a, true)),
                trace(&b, "chosen", b.error(INT)),
                b.int(42),
            ),
            true,
        );
    }
    #[test]
    fn defaults_missing_matches_and_reversed_order() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let bs = [Branch {
            test: Test::False,
            binders: &[],
            body: b.int(0),
        }];
        let yes = b.lit(Constant::bool(&a, true));
        check(
            "default",
            &b,
            b.case(CaseKind::Bool, yes, &bs, Some(b.int(42)), INT),
            false,
        );
        let missing = b.case(CaseKind::Bool, yes, &bs, None, INT);
        let after = check("missing", &b, missing, true);
        assert!(std::ptr::eq(missing, after));
        check(
            "reversed",
            &b,
            b.case(
                CaseKind::Bool,
                yes,
                &[
                    bs[0],
                    Branch {
                        test: Test::True,
                        binders: &[],
                        body: b.int(42),
                    },
                ],
                Some(b.error(INT)),
                INT,
            ),
            false,
        );
    }
    #[test]
    fn earlier_strict_work_remains_and_effectful_subject_is_not_folded() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let x = Binder {
            name: b.fresh("ignored"),
            ty: INT,
        };
        let c = b.if_(b.lit(Constant::bool(&a, true)), b.int(42), b.error(INT));
        check(
            "strict_trace",
            &b,
            b.let_(x, trace(&b, "before", b.int(0)), c),
            false,
        );
        check("strict_failure", &b, b.let_(x, b.error(INT), c), true);
        let c = b.if_(
            trace(&b, "subject", b.lit(Constant::bool(&a, true))),
            b.int(42),
            b.error(INT),
        );
        let after = check("effectful_subject", &b, c, false);
        assert!(std::ptr::eq(c, after));
    }
    #[test]
    fn returned_functions_delays_and_nested_cases() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let yes = b.lit(Constant::bool(&a, true));
        let p = Binder {
            name: b.fresh("p"),
            ty: INT,
        };
        let f = b.lam(&[p], trace(&b, "called", b.var(p.name, p.ty)));
        check(
            "returned_function",
            &b,
            b.app(b.if_(yes, f, b.error(f.ty)), &[b.int(42)], INT),
            false,
        );
        let d = b.delay(trace(&b, "forced", b.int(42)));
        check(
            "returned_delay",
            &b,
            b.force(b.if_(yes, d, b.error(d.ty)), INT),
            false,
        );
        check(
            "nested",
            &b,
            b.if_(
                b.if_(yes, yes, b.lit(Constant::bool(&a, false))),
                b.int(42),
                b.error(INT),
            ),
            false,
        );
    }
    #[test]
    fn malformed_boolean_tables_are_not_hidden() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let p = Binder {
            name: b.fresh("field"),
            ty: INT,
        };
        let yes = b.lit(Constant::bool(&a, true));
        for (name, bs) in [
            (
                "duplicate",
                vec![
                    Branch {
                        test: Test::True,
                        binders: &[],
                        body: b.int(42),
                    },
                    Branch {
                        test: Test::True,
                        binders: &[],
                        body: b.int(0),
                    },
                ],
            ),
            (
                "wrong_test",
                vec![Branch {
                    test: Test::Nil,
                    binders: &[],
                    body: b.int(42),
                }],
            ),
            (
                "fields",
                vec![Branch {
                    test: Test::True,
                    binders: b.arena.alloc_slice_copy(&[p]),
                    body: b.int(42),
                }],
            ),
        ] {
            let root = b.case(CaseKind::Bool, yes, &bs, None, INT);
            let after = known_case::reduce_bool(&b, root);
            let left = crate::lower::lower(&a, root).unwrap_err();
            let right = crate::lower::lower(&a, after).unwrap_err();
            insta::assert_snapshot!(
                name,
                format!(
                    "--- before\n{}\n--- after\n{}\n--- errors\n{left:?}\n{right:?}",
                    pretty(root),
                    pretty(after)
                )
            );
            assert!(std::ptr::eq(root, after));
        }
    }
}

mod constructor {
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
    fn arm<'a>(
        b: &Builder<'a>,
        tag: u16,
        binders: &[Binder<'a>],
        body: &'a Core<'a>,
    ) -> Branch<'a> {
        Branch {
            test: Test::Tag(tag),
            binders: b.arena.alloc_slice_copy(binders),
            body,
        }
    }
    fn check<'a>(name: &str, b: &Builder<'a>, before: &'a Core<'a>, fails: bool) -> &'a Core<'a> {
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
        after
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
            let after = check(name, &b, c, true);
            assert!(std::ptr::eq(c, after));
        }
        // Too few fields returns a lambda in UPLC; explicitly apply it for evaluation.
        let c = b.case(
            CaseKind::Tag,
            constr(&b, 0, &[]),
            &[arm(&b, 0, &[x], b.var(x.name, INT))],
            None,
            b.lam(&[x], b.var(x.name, INT)).ty,
        );
        let root = b.app(c, &[b.int(42)], INT);
        let after = check("missing_field", &b, root, false);
        assert!(std::ptr::eq(root, after));
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
        let after = check("empty_table", &b, empty, true);
        assert!(std::ptr::eq(empty, after));
        let subject = trace(&b, "subject", constr(&b, 0, &[]));
        let c = b.case(
            CaseKind::Tag,
            subject,
            &[arm(&b, 0, &[], b.int(42))],
            None,
            INT,
        );
        let after = check("effectful_subject", &b, c, false);
        assert!(std::ptr::eq(c, after));
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
}

mod list {
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
}

mod data {
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

    mod wrappers {
        //! IData/BData shape selection trial; constructor checks remain strict.
        use super::*;
        const BYTES: Ty<'static> = Ty::Const(&ConstTy::Bytes);
        fn kinds<'a>(b: &Builder<'a>) -> [(F, Test<'a>, &'static str, &'a Core<'a>); 2] {
            [
                (F::IData, Test::DataI, "integer", b.int(42)),
                (
                    F::BData,
                    Test::DataB,
                    "bytes",
                    b.lit(Constant::byte_string(b.arena, b"nash")),
                ),
            ]
        }
        fn check(name: &str, b: &Builder<'_>, original: &Core<'_>, fails: bool) {
            let before = anf::normalize(b, original);
            let folded = known_case::reduce_data_wrappers(b, before);
            let after =
                known_case::simplify_data_wrappers(b, nash_ir::small_inline::simplify(b, folded));
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
            assert!(std::ptr::eq(
                after,
                known_case::simplify_data_wrappers(b, after)
            ));
        }
        #[test]
        fn literal_and_variable_operands_select_payload() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, value) in kinds(&b) {
                for parameter in [false, true] {
                    let x = bind(&b, "input", value.ty);
                    let p = bind(&b, "payload", value.ty);
                    let operand = if parameter {
                        b.var(x.name, x.ty)
                    } else {
                        value
                    };
                    let c = b.case(
                        CaseKind::Data,
                        b.builtin(func, &[operand], DATA),
                        &[arm(&b, test, &[p], b.var(p.name, p.ty))],
                        Some(trace(&b, "cold", b.error(p.ty))),
                        p.ty,
                    );
                    let root = if parameter {
                        b.app(b.lam(&[x], c), &[value], c.ty)
                    } else {
                        c
                    };
                    check(&format!("{name}_parameter_{parameter}"), &b, root, false);
                }
            }
        }
        #[test]
        fn strict_operands_and_intervening_effects_run_once() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, value) in kinds(&b) {
                for fail in [false, true] {
                    let d = bind(&b, "data", DATA);
                    let p = bind(&b, "payload", value.ty);
                    let effect = bind(&b, "effect", INT);
                    let c = b.case(
                        CaseKind::Data,
                        b.var(d.name, DATA),
                        &[arm(&b, test, &[p], trace(&b, "selected", b.int(9)))],
                        None,
                        INT,
                    );
                    let operand =
                        trace(&b, "operand", if fail { b.error(value.ty) } else { value });
                    let root = b.let_(
                        d,
                        b.builtin(func, &[operand], DATA),
                        b.let_(effect, trace(&b, "between", b.int(0)), c),
                    );
                    check(&format!("{name}_strict_failure_{fail}"), &b, root, fail);
                }
            }
        }
        #[test]
        fn wrong_runtime_operands_fail_even_when_payload_is_unused() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, _) in kinds(&b) {
                let x = bind(&b, "x", INT);
                for (bad, kind) in [
                    (b.lit(Constant::bool(&a, true)), "bool"),
                    (b.lam(&[x], b.var(x.name, INT)), "function"),
                    (b.delay(b.lam(&[x], b.var(x.name, INT))), "delay"),
                ] {
                    for fallback in [false, true] {
                        let p = bind(&b, "unused", bad.ty);
                        let arms = if fallback {
                            vec![]
                        } else {
                            vec![arm(&b, test, &[p], b.int(9))]
                        };
                        let c = b.case(
                            CaseKind::Data,
                            b.builtin(func, &[bad], DATA),
                            &arms,
                            Some(trace(&b, "unreachable", b.int(0))),
                            INT,
                        );
                        check(
                            &format!("{name}_invalid_{kind}_fallback_{fallback}"),
                            &b,
                            c,
                            true,
                        );
                    }
                }
            }
        }
        #[test]
        fn missing_shape_uses_default_or_errors() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, _, name, value) in kinds(&b) {
                let p = bind(&b, "wrong", Ty::Const(&ConstTy::List(DATA)));
                for default in [false, true] {
                    let c = b.case(
                        CaseKind::Data,
                        b.builtin(func, &[trace(&b, "operand", value)], DATA),
                        &[arm(
                            &b,
                            Test::DataList,
                            &[p],
                            trace(&b, "wrong", b.error(INT)),
                        )],
                        default.then(|| trace(&b, "fallback", b.int(9))),
                        INT,
                    );
                    check(&format!("{name}_default_{default}"), &b, c, !default);
                }
            }
        }
        #[test]
        fn aliases_repeated_matches_and_original_escape() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, value) in kinds(&b) {
                let d = bind(&b, "data", DATA);
                let alias = bind(&b, "alias", DATA);
                let fields: Vec<_> = (0..2)
                    .map(|_| {
                        let p = bind(&b, "payload", value.ty);
                        b.case(
                            CaseKind::Data,
                            b.var(alias.name, DATA),
                            &[arm(&b, test, &[p], b.var(p.name, p.ty))],
                            None,
                            p.ty,
                        )
                    })
                    .collect();
                let root = b.let_(
                    d,
                    b.builtin(func, &[trace(&b, "once", value)], DATA),
                    b.let_(
                        alias,
                        b.var(d.name, DATA),
                        b.constr(0, &[b.var(d.name, DATA), fields[0], fields[1]], Ty::Erased),
                    ),
                );
                check(&format!("{name}_shared_escape"), &b, root, false);
            }
        }
        #[test]
        fn suspended_matches_do_not_delay_original_constructor() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, value) in kinds(&b) {
                let d = bind(&b, "data", DATA);
                let p = bind(&b, "payload", value.ty);
                let c = b.case(
                    CaseKind::Data,
                    b.var(d.name, DATA),
                    &[arm(&b, test, &[p], b.var(p.name, p.ty))],
                    None,
                    p.ty,
                );
                for cold in [false, true] {
                    let branch = b.if_(
                        b.lit(Constant::bool(&a, cold)),
                        value,
                        b.force(b.delay(c), p.ty),
                    );
                    check(
                        &format!("{name}_captured_cold_{cold}"),
                        &b,
                        b.let_(
                            d,
                            b.builtin(func, &[trace(&b, "before", value)], DATA),
                            branch,
                        ),
                        false,
                    );
                }
            }
        }
        #[test]
        fn partial_overapplied_and_traced_producers_are_not_facts() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, value) in kinds(&b) {
                let p = bind(&b, "payload", value.ty);
                let full = b.builtin(func, &[value], DATA);
                for (subject, label, fails) in [
                    (b.builtin(func, &[], DATA), "partial", true),
                    (b.app(full, &[b.int(0)], DATA), "overapplied", true),
                    (trace(&b, "producer", full), "traced", false),
                ] {
                    check(
                        &format!("{name}_{label}"),
                        &b,
                        b.case(
                            CaseKind::Data,
                            subject,
                            &[arm(&b, test, &[p], b.var(p.name, p.ty))],
                            None,
                            p.ty,
                        ),
                        fails,
                    );
                }
            }
        }
        #[test]
        fn repeated_matches_share_long_literal_bytes() {
            let a = Arena::new();
            let b = Builder::new(&a);
            let bytes = b.lit(Constant::byte_string(&a, a.alloc_slice_copy(&[7; 128])));
            let d = bind(&b, "data", DATA);
            let fields: Vec<_> = (0..3)
                .map(|_| {
                    let p = bind(&b, "payload", BYTES);
                    b.case(
                        CaseKind::Data,
                        b.var(d.name, DATA),
                        &[arm(&b, Test::DataB, &[p], b.var(p.name, BYTES))],
                        None,
                        BYTES,
                    )
                })
                .collect();
            check(
                "shared_long_bytes",
                &b,
                b.let_(
                    d,
                    b.builtin(F::BData, &[bytes], DATA),
                    b.constr(0, &fields, Ty::Erased),
                ),
                false,
            );
        }
        #[test]
        fn nested_wrapper_and_returned_function_preserve_type_views() {
            let a = Arena::new();
            let b = Builder::new(&a);
            let p = bind(&b, "payload", INT);
            let q = bind(&b, "inner", INT);
            let x = bind(&b, "x", INT);
            let fun = b.lam(
                &[x],
                b.builtin(
                    F::AddInteger,
                    &[b.var(q.name, INT), b.var(x.name, INT)],
                    INT,
                ),
            );
            let inner = b.case(
                CaseKind::Data,
                b.builtin(F::IData, &[b.var(p.name, INT)], DATA),
                &[arm(&b, Test::DataI, &[q], fun)],
                None,
                fun.ty,
            );
            let outer = b.case(
                CaseKind::Data,
                b.builtin(F::IData, &[b.int(40)], DATA),
                &[arm(&b, Test::DataI, &[p], inner)],
                None,
                inner.ty,
            );
            check(
                "nested_returned_function",
                &b,
                b.with_type(b.app(outer, &[b.int(2)], INT), Ty::Erased),
                false,
            );
        }
        #[test]
        fn malformed_unselected_arms_remain_lowering_errors() {
            let a = Arena::new();
            let b = Builder::new(&a);
            for (func, test, name, value) in kinds(&b) {
                let p = bind(&b, "payload", value.ty);
                let d = bind(&b, "data", DATA);
                let valid = arm(&b, test, &[p], b.int(42));
                for (arms, label) in [
                    (vec![valid, valid], "duplicate"),
                    (
                        vec![valid, arm(&b, Test::DataList, &[], b.int(0))],
                        "unselected_arity",
                    ),
                    (vec![arm(&b, test, &[p, p], b.int(0))], "selected_arity"),
                    (
                        vec![valid, arm(&b, Test::True, &[], b.int(0))],
                        "wrong_test",
                    ),
                ] {
                    let before = b.let_(
                        d,
                        b.builtin(func, &[value], DATA),
                        b.case(CaseKind::Data, b.var(d.name, DATA), &arms, None, INT),
                    );
                    let after = known_case::reduce_data_wrappers(&b, before);
                    let left = crate::lower::lower(&a, before).unwrap_err();
                    let right = crate::lower::lower(&a, after).unwrap_err();
                    insta::assert_snapshot!(
                        format!("{name}_invalid_{label}"),
                        format!(
                            "--- before\n{}\n--- after\n{}\n--- errors\n{left:?}\n{right:?}",
                            pretty(before),
                            pretty(after)
                        )
                    );
                    assert!(std::ptr::eq(before, after));
                }
            }
        }
        #[test]
        fn unrelated_producers_and_unknown_data_parameter_are_not_wrapper_facts() {
            let a = Arena::new();
            let b = Builder::new(&a);
            let list = b.lit(Constant::proto_list(&a, &nash_plutus::typ::Type::Data, &[]));
            let map = b.lit(Constant::proto_list(
                &a,
                nash_plutus::typ::Type::pair(
                    &a,
                    &nash_plutus::typ::Type::Data,
                    &nash_plutus::typ::Type::Data,
                ),
                &[],
            ));
            for (name, subject) in [
                ("list_producer", b.builtin(F::ListData, &[list], DATA)),
                ("map_producer", b.builtin(F::MapData, &[map], DATA)),
                (
                    "constr_producer",
                    b.builtin(F::ConstrData, &[b.int(0), list], DATA),
                ),
            ] {
                check(
                    name,
                    &b,
                    b.case(CaseKind::Data, subject, &[], Some(b.int(42)), INT),
                    false,
                );
            }
            let d = bind(&b, "unknown", DATA);
            let p = bind(&b, "payload", INT);
            let c = b.case(
                CaseKind::Data,
                b.var(d.name, DATA),
                &[arm(&b, Test::DataI, &[p], b.var(p.name, INT))],
                None,
                INT,
            );
            check(
                "unknown_data_parameter",
                &b,
                b.app(
                    b.lam(&[d], c),
                    &[literal(&b, PlutusData::integer_from(&a, 42))],
                    INT,
                ),
                false,
            );
        }
    }
}
