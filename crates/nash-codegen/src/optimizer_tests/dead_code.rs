//! Dead-code elimination semantics with accepted-pipeline and isolated-pass evidence.

mod bindings {
    //! Safe unused nonrecursive bindings.
    use nash_ir::{
        analysis,
        build::Builder,
        core::*,
        dead_code, hygiene,
        pretty::pretty,
        ty::{ConstTy, TermTy, Ty},
    };
    use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};
    const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
    fn unary<'a>(b: &Builder<'a>) -> Ty<'a> {
        Ty::Term(
            b.arena
                .alloc(TermTy::Fun(b.arena.alloc_slice_copy(&[INT]), INT)),
        )
    }
    fn bind<'a>(b: &Builder<'a>, ty: Ty<'a>) -> Binder<'a> {
        Binder {
            name: b.fresh("unused"),
            ty,
        }
    }
    fn check(name: &str, b: &Builder<'_>, before: &Core<'_>, removes: bool, fails: bool) {
        let after = dead_code::simplify_bindings(b, before);

        let baseline = crate::harness::eval_core_raw(b.arena, before);
        let candidate = crate::harness::eval_core_raw(b.arena, after);

        assert_eq!(candidate.result.starts_with("error:"), fails);
        insta::assert_snapshot!(
            name,
            crate::harness::pass_snapshot(
                b.arena,
                before,
                format!(
                    "--- core before\n{}\n--- uplc before\n{}\n--- core after\n{}\n--- uplc after\n{}\n--- result\n{}\n--- logs\n{:?}",
                    pretty(before),
                    baseline.uplc,
                    pretty(after),
                    candidate.uplc,
                    candidate.result,
                    candidate.logs
                )
            )
        );
        let normalized = nash_ir::anf::normalize(b, before);
        nash_ir::anf::validate(dead_code::simplify_bindings(b, normalized)).unwrap();
        hygiene::validate(after, &[]).unwrap();
        // Properties independent of the expected snapshot.
        assert_eq!(before.ty, after.ty);
        assert_eq!(
            pretty(after),
            pretty(dead_code::simplify_bindings(b, after))
        );
        assert_eq!(
            analysis::size_estimate(after).nodes < analysis::size_estimate(before).nodes,
            removes
        );
        assert_eq!(baseline.observable, candidate.observable);
        assert_eq!(baseline.logs, candidate.logs);
    }
    #[test]
    fn unused_safe_values() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let p = bind(&b, INT);
        let values = [
            ("literal", b.int(9)),
            ("closure_with_failure", b.lam(&[p], b.error(INT))),
            ("delayed_failure", b.delay(b.error(INT))),
            (
                "partial_builtin",
                b.builtin(F::AddInteger, &[b.int(9)], unary(&b)),
            ),
            (
                "forced_builtin",
                b.builtin(
                    F::HeadList,
                    &[],
                    Ty::Term(a.alloc(TermTy::Fun(
                        a.alloc_slice_copy(&[Ty::Const(a.alloc(ConstTy::List(INT)))]),
                        INT,
                    ))),
                ),
            ),
        ];
        for (name, value) in values {
            check(
                name,
                &b,
                b.let_(bind(&b, value.ty), value, b.int(42)),
                true,
                false,
            );
        }
    }
    #[test]
    fn unused_strict_work_stays() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let trace = b.trace(b.lit(Constant::string(&a, "kept")), b.int(7));
        for (name, value, fails) in [
            ("trace", trace, false),
            ("failure", b.error(INT), true),
            ("forced_failure", b.force(b.delay(b.error(INT)), INT), true),
            (
                "saturated_builtin",
                b.builtin(F::DivideInteger, &[b.int(1), b.int(0)], INT),
                true,
            ),
            (
                "partial_strict_argument",
                b.builtin(F::AddInteger, &[trace], unary(&b)),
                false,
            ),
        ] {
            check(
                name,
                &b,
                b.let_(bind(&b, value.ty), value, b.int(42)),
                false,
                fails,
            );
        }
    }
    #[test]
    fn cascading_unused_bindings() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let x = bind(&b, INT);
        let y = bind(&b, INT);
        check(
            "cascade",
            &b,
            b.let_(x, b.int(7), b.let_(y, b.var(x.name, INT), b.int(42))),
            true,
            false,
        );
    }
    #[test]
    fn used_capture_stays() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let x = bind(&b, INT);
        let p = bind(&b, INT);
        check(
            "used_capture",
            &b,
            b.let_(x, b.int(7), b.lam(&[p], b.var(x.name, INT))),
            false,
            false,
        );
    }
    #[test]
    fn constructor_fields_are_strict() {
        let a = Arena::new();
        let b = Builder::new(&a);
        for (name, field, removes, fails) in [
            ("safe_constructor", b.int(7), true, false),
            ("strict_constructor", b.error(INT), false, true),
        ] {
            let ty = Ty::Runtime(a.alloc(nash_ir::ty::RuntimeTy::Constr {
                tag: 0,
                fields: a.alloc_slice_copy(&[INT]),
            }));
            let value = b.constr(0, &[field], ty);
            check(
                name,
                &b,
                b.let_(bind(&b, ty), value, b.int(42)),
                removes,
                fails,
            );
        }
    }
    #[test]
    fn diverging_call_is_retained() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let p = bind(&b, INT);
        let f = bind(&b, unary(&b));
        let recur = b.app(b.var(f.name, f.ty), &[b.var(p.name, INT)], INT);
        let root = b.let_rec(
            &[RecBinder {
                binder: f,
                params: a.alloc_slice_copy(&[p]),
                static_params: &[],
                body: recur,
            }],
            b.let_(
                bind(&b, INT),
                b.app(b.var(f.name, f.ty), &[b.int(0)], INT),
                b.int(42),
            ),
        );
        let after = dead_code::simplify_bindings(&b, root);
        insta::assert_snapshot!(
            "diverging_call",
            crate::harness::pass_snapshot(
                b.arena,
                root,
                format!(
                    "--- core before\n{}\n--- core after\n{}",
                    pretty(root),
                    pretty(after)
                )
            )
        );
        // Deliberately do not run an infinite program: exact identity proves that
        // the unused recursive call and its strict evaluation remain intact.
        assert!(std::ptr::eq(root, after));
        hygiene::validate(after, &[]).unwrap();
    }
    #[test]
    fn unused_closure_releases_capture() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let x = bind(&b, INT);
        let p = bind(&b, INT);
        let lam = b.lam(&[p], b.var(x.name, INT));
        check(
            "dead_capture",
            &b,
            b.let_(x, b.int(7), b.let_(bind(&b, lam.ty), lam, b.int(42))),
            true,
            false,
        );
    }

    #[test]
    fn accepted_cleanup_removes_safe_ignored_arguments() {
        let a = Arena::new();
        let b = Builder::new(&a);
        for (name, argument) in [
            ("cleanup_safe_argument", b.delay(b.error(INT))),
            (
                "cleanup_strict_argument",
                b.trace(b.lit(Constant::string(&a, "kept")), b.int(9)),
            ),
        ] {
            let p = bind(&b, argument.ty);
            let before =
                nash_ir::anf::normalize(&b, b.app(b.lam(&[p], b.int(42)), &[argument], INT));
            let after = nash_ir::small_inline::simplify(&b, before);
            let baseline = crate::harness::eval_core_raw(&a, before);
            let candidate = crate::harness::eval_core_raw(&a, after);

            insta::assert_snapshot!(
                name,
                crate::harness::pass_snapshot(
                    b.arena,
                    before,
                    format!(
                        "--- core before\n{}\n--- core after\n{}\n--- result\n{}\n--- logs\n{:?}",
                        pretty(before),
                        pretty(after),
                        candidate.result,
                        candidate.logs
                    )
                )
            );
            assert_eq!(baseline.logs, candidate.logs);
            assert_eq!(baseline.observable, candidate.observable);
            nash_ir::anf::validate(after).unwrap();
            hygiene::validate(after, &[]).unwrap();
            assert_eq!(before.ty, after.ty);
            assert_eq!(
                pretty(after),
                pretty(nash_ir::small_inline::simplify(&b, after))
            );
        }
    }
}

mod recursive {
    //! Continuation-rooted recursive group reachability.
    use nash_ir::{
        build::Builder,
        core::*,
        dead_code, hygiene,
        pretty::pretty,
        ty::{ConstTy, TermTy, Ty},
    };
    use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};
    const INT: Ty<'static> = Ty::Const(&ConstTy::Int);
    fn bind<'a>(b: &Builder<'a>, text: &'a str, ty: Ty<'a>) -> Binder<'a> {
        Binder {
            name: b.fresh(text),
            ty,
        }
    }
    fn function<'a>(b: &Builder<'a>, text: &'a str, arity: usize) -> Binder<'a> {
        bind(
            b,
            text,
            Ty::Term(b.arena.alloc(TermTy::Fun(
                b.arena.alloc_slice_copy(&vec![INT; arity]),
                INT,
            ))),
        )
    }
    fn def<'a>(
        b: &Builder<'a>,
        binder: Binder<'a>,
        params: &[Binder<'a>],
        body: &'a Core<'a>,
    ) -> RecBinder<'a> {
        RecBinder {
            binder,
            params: b.arena.alloc_slice_copy(params),
            static_params: &[],
            body,
        }
    }
    fn call<'a>(b: &Builder<'a>, f: Binder<'a>, args: &[&'a Core<'a>]) -> &'a Core<'a> {
        b.app(b.var(f.name, f.ty), args, INT)
    }
    fn trace<'a>(b: &Builder<'a>, message: &'a str, body: &'a Core<'a>) -> &'a Core<'a> {
        b.trace(b.lit(Constant::string(b.arena, message)), body)
    }
    fn check<'a>(name: &str, b: &Builder<'a>, before: &'a Core<'a>, fails: bool) -> &'a Core<'a> {
        let after = dead_code::prune_recursive(b, before);
        let baseline =
            crate::harness::eval_core_raw(b.arena, crate::recursion::rewrite(b, before).unwrap());
        let candidate =
            crate::harness::eval_core_raw(b.arena, crate::recursion::rewrite(b, after).unwrap());

        assert_eq!(candidate.result.starts_with("error:"), fails);
        insta::assert_snapshot!(
            name,
            crate::harness::pass_snapshot(
                b.arena,
                before,
                format!(
                    "--- core before\n{}\n--- uplc before\n{}\n--- core after\n{}\n--- uplc after\n{}\n--- result\n{}\n--- logs\n{:?}",
                    pretty(before),
                    baseline.uplc,
                    pretty(after),
                    candidate.uplc,
                    candidate.result,
                    candidate.logs
                )
            )
        );
        hygiene::validate(before, &[]).unwrap();
        hygiene::validate(after, &[]).unwrap();

        let anf = nash_ir::anf::normalize(b, before);
        nash_ir::anf::validate(dead_code::prune_recursive(b, anf)).unwrap();
        // Properties independent of the expected snapshot.
        assert_eq!(before.ty, after.ty);
        assert!(std::ptr::eq(after, dead_code::prune_recursive(b, after)));
        assert_eq!(baseline.observable, candidate.observable);
        assert_eq!(baseline.logs, candidate.logs);
        after
    }
    #[test]
    fn unused_self_and_mutual_cycles() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "f", 1);
        let g = function(&b, "g", 1);
        let x = bind(&b, "x", INT);
        let y = bind(&b, "y", INT);
        let self_def = def(
            &b,
            f,
            &[x],
            trace(&b, "unreachable", call(&b, f, &[b.var(x.name, INT)])),
        );
        check("unused_self", &b, b.let_rec(&[self_def], b.int(42)), false);
        let fs = def(&b, f, &[x], call(&b, g, &[b.var(x.name, INT)]));
        let gs = def(&b, g, &[y], call(&b, f, &[b.var(y.name, INT)]));
        check(
            "unused_cycle",
            &b,
            b.let_rec(&[fs, gs], trace(&b, "continuation", b.int(42))),
            false,
        );
        check(
            "unused_failure",
            &b,
            b.let_rec(&[def(&b, f, &[x], b.error(INT))], b.int(42)),
            false,
        );
    }
    #[test]
    fn transitive_chain_retains_order_and_drops_cycle() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "f", 1);
        let g = function(&b, "g", 1);
        let dead = function(&b, "dead", 1);
        let x = bind(&b, "x", INT);
        let y = bind(&b, "y", INT);
        let z = bind(&b, "z", INT);
        let root = b.let_rec(
            &[
                def(&b, g, &[y], trace(&b, "g", b.var(y.name, INT))),
                def(&b, dead, &[z], call(&b, dead, &[b.var(z.name, INT)])),
                def(
                    &b,
                    f,
                    &[x],
                    trace(&b, "f", call(&b, g, &[b.var(x.name, INT)])),
                ),
            ],
            call(&b, f, &[b.int(42)]),
        );
        check("transitive", &b, root, false);
    }
    #[test]
    fn returned_partial_and_suspended_references_are_roots() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "f", 2);
        let dead = function(&b, "dead", 1);
        let x = bind(&b, "x", INT);
        let y = bind(&b, "y", INT);
        let z = bind(&b, "z", INT);
        let defs = [
            def(
                &b,
                f,
                &[x, y],
                b.builtin(
                    F::AddInteger,
                    &[b.var(x.name, INT), b.var(y.name, INT)],
                    INT,
                ),
            ),
            def(&b, dead, &[z], b.error(INT)),
        ];
        let returned = b.app(
            b.let_rec(&defs, b.var(f.name, f.ty)),
            &[b.int(40), b.int(2)],
            INT,
        );
        check("returned", &b, returned, false);
        let partial = b.app(
            b.var(f.name, f.ty),
            &[b.int(40)],
            function(&b, "typeOnly", 1).ty,
        );
        check(
            "partial",
            &b,
            b.app(b.let_rec(&defs, partial), &[b.int(2)], INT),
            false,
        );
        let delayed = b.let_rec(&defs, b.delay(call(&b, f, &[b.int(40), b.int(2)])));
        check("delayed_capture", &b, b.force(delayed, INT), false);
        let p = bind(&b, "p", INT);
        let closure = b.let_rec(
            &defs,
            b.lam(&[p], call(&b, f, &[b.int(40), b.var(p.name, INT)])),
        );
        check(
            "lambda_capture",
            &b,
            b.app(closure, &[b.int(2)], INT),
            false,
        );
    }
    #[test]
    fn singleton_preserves_static_metadata_and_captures() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "f", 2);
        let dead = function(&b, "dead", 1);
        let x = bind(&b, "static", INT);
        let n = bind(&b, "n", INT);
        let z = bind(&b, "z", INT);
        let capture = bind(&b, "capture", INT);
        let body = b.if_(
            b.builtin(
                F::EqualsInteger,
                &[b.var(n.name, INT), b.int(0)],
                Ty::Const(&ConstTy::Bool),
            ),
            b.builtin(
                F::AddInteger,
                &[b.var(x.name, INT), b.var(capture.name, INT)],
                INT,
            ),
            call(
                &b,
                f,
                &[
                    b.var(x.name, INT),
                    b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
                ],
            ),
        );
        let member = RecBinder {
            static_params: &[0],
            ..def(&b, f, &[x, n], body)
        };
        let group = b.let_rec(
            &[member, def(&b, dead, &[z], b.error(INT))],
            call(&b, f, &[b.int(40), b.int(3)]),
        );
        let after = check(
            "singleton_metadata",
            &b,
            b.let_(capture, b.int(2), group),
            false,
        );
        let CoreKind::Let { body: after, .. } = after.kind else {
            panic!("capture binding");
        };
        let CoreKind::LetRec {
            binders: [kept], ..
        } = after.kind
        else {
            panic!("singleton");
        };
        assert!(std::ptr::eq(kept.params, member.params));
        assert!(std::ptr::eq(kept.static_params, member.static_params));
        assert_eq!(kept.binder.ty, member.binder.ty);
    }
    #[test]
    fn reachable_failure_stays() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "f", 1);
        let x = bind(&b, "x", INT);
        let root = b.let_rec(
            &[def(&b, f, &[x], trace(&b, "failure", b.error(INT)))],
            call(&b, f, &[b.int(42)]),
        );
        let after = check("reachable_failure", &b, root, true);
        assert!(std::ptr::eq(root, after));
    }
    #[test]
    fn delayed_workers_and_nested_groups() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let value = b.delay(b.int(42));
        let worker = bind(&b, "worker", value.ty);
        let member = def(
            &b,
            worker,
            &[],
            b.delay(b.if_(
                b.lit(Constant::bool(&a, true)),
                b.int(42),
                b.force(b.var(worker.name, worker.ty), INT),
            )),
        );
        check(
            "unused_delayed_worker",
            &b,
            b.let_rec(&[member], b.int(7)),
            false,
        );
        check(
            "live_delayed_worker",
            &b,
            b.let_rec(&[member], b.force(b.var(worker.name, worker.ty), INT)),
            false,
        );
        let f = function(&b, "outer", 1);
        let g = function(&b, "inner", 1);
        let x = bind(&b, "x", INT);
        let y = bind(&b, "y", INT);
        let inner = b.let_rec(
            &[def(&b, g, &[y], call(&b, f, &[b.var(y.name, INT)]))],
            b.int(42),
        );
        let root = b.let_rec(&[def(&b, f, &[x], b.var(x.name, INT))], inner);
        check("nested_dead_capture", &b, root, false);
    }
    #[test]
    fn unsupported_recursive_values_stay_rejected() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let value = bind(&b, "value", INT);
        let root = b.let_rec(&[def(&b, value, &[], b.error(INT))], b.int(42));
        let after = dead_code::prune_recursive(&b, root);
        assert!(std::ptr::eq(root, after));
        assert_eq!(
            crate::recursion::rewrite(&b, after).unwrap_err(),
            crate::recursion::Error::RecursiveValue
        );
    }
    #[test]
    fn reachable_mutual_cycle_survives() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "f", 1);
        let g = function(&b, "g", 1);
        let dead = function(&b, "dead", 1);
        let x = bind(&b, "x", INT);
        let y = bind(&b, "y", INT);
        let z = bind(&b, "z", INT);
        let step = |next, name| {
            b.if_(
                b.builtin(
                    F::EqualsInteger,
                    &[b.var(name, INT), b.int(0)],
                    Ty::Const(&ConstTy::Bool),
                ),
                b.int(42),
                call(
                    &b,
                    next,
                    &[b.builtin(F::SubtractInteger, &[b.var(name, INT), b.int(1)], INT)],
                ),
            )
        };
        let root = b.let_rec(
            &[
                def(&b, f, &[x], step(g, x.name)),
                def(&b, dead, &[z], b.error(INT)),
                def(&b, g, &[y], step(f, y.name)),
            ],
            call(&b, f, &[b.int(4)]),
        );
        check("live_mutual_cycle", &b, root, false);
    }
    #[test]
    fn cold_and_nested_references_remain_live() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let f = function(&b, "outer", 1);
        let g = function(&b, "inner", 1);
        let x = bind(&b, "x", INT);
        let y = bind(&b, "y", INT);
        let outer = def(&b, f, &[x], b.var(x.name, INT));
        let inner = b.let_rec(
            &[def(&b, g, &[y], call(&b, f, &[b.var(y.name, INT)]))],
            call(&b, g, &[b.int(42)]),
        );
        check("nested_live_capture", &b, b.let_rec(&[outer], inner), false);
        let cold = b.let_rec(
            &[def(&b, f, &[x], b.error(INT))],
            b.if_(
                b.lit(Constant::bool(&a, true)),
                b.int(42),
                call(&b, f, &[b.int(0)]),
            ),
        );
        let after = check("cold_reference", &b, cold, false);
        assert!(std::ptr::eq(cold, after));
    }

    #[test]
    fn accepted_cleanup_releases_dead_captures_but_preserves_effects() {
        let a = Arena::new();
        let b = Builder::new(&a);
        for (name, captured, suspended) in [
            ("cleanup_dead_delay_capture", b.delay(b.error(INT)), true),
            ("cleanup_strict_capture", trace(&b, "kept", b.int(7)), false),
        ] {
            let capture = bind(&b, "capture", captured.ty);
            let f = function(&b, "dead", 1);
            let x = bind(&b, "x", INT);
            let value = b.var(capture.name, capture.ty);
            let read = if suspended {
                b.force(value, INT)
            } else {
                value
            };
            let body = b.builtin(F::AddInteger, &[read, read], INT);
            let root = b.let_(
                capture,
                captured,
                b.let_rec(&[def(&b, f, &[x], body)], b.int(42)),
            );
            let before = nash_ir::anf::normalize(&b, root);
            let after = nash_ir::small_inline::simplify(&b, before);
            let baseline =
                crate::harness::eval_core_raw(&a, crate::recursion::rewrite(&b, before).unwrap());
            let candidate =
                crate::harness::eval_core_raw(&a, crate::recursion::rewrite(&b, after).unwrap());

            insta::assert_snapshot!(
                name,
                crate::harness::pass_snapshot(
                    b.arena,
                    before,
                    format!(
                        "--- core before\n{}\n--- core after\n{}\n--- result\n{}\n--- logs\n{:?}",
                        pretty(before),
                        pretty(after),
                        candidate.result,
                        candidate.logs
                    )
                )
            );
            assert_eq!(baseline.logs, candidate.logs);
            assert_eq!(baseline.observable, candidate.observable);
            assert_eq!(before.ty, after.ty);
            nash_ir::anf::validate(after).unwrap();
            hygiene::validate(after, &[]).unwrap();
            assert!(std::ptr::eq(
                after,
                nash_ir::small_inline::simplify(&b, after)
            ));
        }
    }
}
