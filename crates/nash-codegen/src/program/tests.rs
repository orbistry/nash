use super::*;
use crate::comptime::{self, ComptimeError};
use nash_ir::{
    core::{Branch, CaseKind, RecBinder, Test},
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant, pretty};

#[test]
fn optimizer_substitution_preserves_free_names_at_runtime() {
    use nash_ir::hygiene;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let y = binder(&b, "y");
    let target = binder(&b, "target");
    // The replacement's free y must remain outside the recipient's local y.
    let recipient = b.let_(
        y,
        b.int(10),
        b.builtin(
            F::AddInteger,
            &[b.var(target.name, target.ty), b.var(y.name, y.ty)],
            Ty::Const(&ConstTy::Int),
        ),
    );
    let replaced = hygiene::substitute(&b, recipient, target.name.unique, b.var(y.name, y.ty));
    let candidate = b.let_(y, b.int(20), replaced);
    hygiene::validate(candidate, &[]).unwrap();
    let reference = b.let_(y, b.int(20), b.let_(target, b.var(y.name, y.ty), recipient));
    let prepared = crate::snapshot_optimizer::prepare(&arena, reference);
    let original_program = &prepared.before;
    let changed_program = assemble_core(&arena, candidate).unwrap();
    insta::assert_snapshot!(format!(
        "{}\n--- isolated pass\n--- Core before substitution\n{}\n--- UPLC before substitution\n{}\n--- Core after substitution\n{}\n--- UPLC after substitution\n{}",
        prepared.snapshot(),
        nash_ir::pretty::pretty(reference),
        pretty::term(original_program.named),
        nash_ir::pretty::pretty(candidate),
        pretty::term(changed_program.named),
    ));
    let original = original_program.program.eval(&arena);
    let changed = changed_program.program.eval(&arena);
    assert_eq!(
        pretty::term(original.term.as_ref().unwrap()),
        pretty::term(changed.term.as_ref().unwrap())
    );
    insta::assert_snapshot!(pretty::term(changed.term.unwrap()), @"(con integer 30)");
    assert_eq!(original.info.logs, changed.info.logs);
}

#[test]
fn optimizer_substitution_freshens_each_inserted_function() {
    use nash_ir::hygiene;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let target = function(&b, "target");
    let x = binder(&b, "x");
    let replacement = b.lam(
        &[x],
        b.trace(
            b.lit(Constant::string(&arena, "called")),
            b.builtin(
                F::AddInteger,
                &[b.var(x.name, x.ty), b.int(1)],
                Ty::Const(&ConstTy::Int),
            ),
        ),
    );
    let recipient = b.constr(
        0,
        &[
            b.app(
                b.var(target.name, target.ty),
                &[b.int(20)],
                Ty::Const(&ConstTy::Int),
            ),
            b.app(
                b.var(target.name, target.ty),
                &[b.int(21)],
                Ty::Const(&ConstTy::Int),
            ),
        ],
        Ty::Runtime(
            b.arena.alloc(nash_ir::ty::RuntimeTy::Constr {
                tag: 0,
                fields: b
                    .arena
                    .alloc_slice_copy(&[Ty::Const(&ConstTy::Int), Ty::Const(&ConstTy::Int)]),
            }),
        ),
    );
    let candidate = hygiene::substitute(&b, recipient, target.name.unique, replacement);
    hygiene::validate(candidate, &[]).unwrap();
    let reference = b.let_(target, replacement, recipient);
    let prepared = crate::snapshot_optimizer::prepare(&arena, reference);
    let original = prepared.before.program.eval(&arena);
    let changed_program = assemble_core(&arena, candidate).unwrap();
    let changed = changed_program.program.eval(&arena);
    insta::assert_snapshot!(format!(
        "{}\n--- isolated pass\n--- Core after substitution\n{}\n--- UPLC after substitution\n{}\n--- result\n{}\n--- logs\n{:?}",
        prepared.snapshot(),
        nash_ir::pretty::pretty(candidate),
        pretty::term(changed_program.named),
        pretty::term(changed.term.as_ref().unwrap()),
        changed.info.logs,
    ));
    assert_eq!(
        pretty::term(original.term.as_ref().unwrap()),
        pretty::term(changed.term.as_ref().unwrap())
    );
    assert_eq!(original.info.logs, changed.info.logs);
}

#[test]
fn optimizer_discard_analysis_respects_runtime_staging() {
    use nash_ir::analysis::safe_to_discard;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let unused = binder(&b, "unused");
    let traced = b.trace(b.lit(Constant::string(&arena, "strict")), b.int(1));
    let fixtures = [
        (
            "partial builtin",
            b.builtin(
                F::AddInteger,
                &[b.int(1)],
                Ty::Term(&TermTy::Fun(
                    &[Ty::Const(&ConstTy::Int)],
                    Ty::Const(&ConstTy::Int),
                )),
            ),
        ),
        (
            "strict partial argument",
            b.builtin(
                F::AddInteger,
                &[traced],
                Ty::Term(&TermTy::Fun(
                    &[Ty::Const(&ConstTy::Int)],
                    Ty::Const(&ConstTy::Int),
                )),
            ),
        ),
        (
            "delayed failure",
            b.delay(b.error(Ty::Const(&ConstTy::Int))),
        ),
        (
            "empty lambda",
            b.lam(&[], b.error(Ty::Const(&ConstTy::Int))),
        ),
        (
            "strict constructor field",
            b.constr(
                0,
                &[traced],
                Ty::Runtime(b.arena.alloc(nash_ir::ty::RuntimeTy::Constr {
                    tag: 0,
                    fields: b.arena.alloc_slice_copy(&[Ty::Const(&ConstTy::Int)]),
                })),
            ),
        ),
        (
            "failing constructor field",
            b.constr(
                0,
                &[b.error(Ty::Const(&ConstTy::Int))],
                Ty::Runtime(b.arena.alloc(nash_ir::ty::RuntimeTy::Constr {
                    tag: 0,
                    fields: b.arena.alloc_slice_copy(&[Ty::Const(&ConstTy::Int)]),
                })),
            ),
        ),
    ];
    let mut results = Vec::new();
    for (label, value) in fixtures {
        let unused = Binder {
            ty: value.ty,
            ..unused
        };
        let core = b.let_(unused, value, b.int(42));
        let prepared = crate::snapshot_optimizer::prepare(&arena, core);
        let evaluated = prepared.before.program.eval(&arena);
        let result = evaluated
            .term
            .map(pretty::term)
            .map_err(|e| format!("{e:?}"));
        if safe_to_discard(value) {
            assert_eq!(result.as_deref(), Ok("(con integer 42)"));
            assert!(evaluated.info.logs.is_empty());
        }
        results.push(format!("--- fixture\n{label}\n{}\n--- safe to discard\n{}\n--- result\n{result:?}\n--- logs\n{:?}",
            prepared.snapshot(), safe_to_discard(value), evaluated.info.logs));
    }
    insta::assert_snapshot!(results.join("\n"));
}

fn binder<'a>(b: &Builder<'a>, text: &'a str) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty: Ty::Const(&ConstTy::Int),
    }
}
fn function<'a>(b: &Builder<'a>, text: &'a str) -> Binder<'a> {
    Binder {
        name: b.fresh(text),
        ty: Ty::Term(&TermTy::Fun(
            &[Ty::Const(&ConstTy::Int)],
            Ty::Const(&ConstTy::Int),
        )),
    }
}
#[test]
fn assemble_lets_chain() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    let y = binder(&b, "y");
    let unused = binder(&b, "unused");
    let bindings = &[
        (x, b.int(40)),
        (
            y,
            b.builtin(
                F::AddInteger,
                &[b.var(x.name, x.ty), b.int(2)],
                Ty::Const(&ConstTy::Int),
            ),
        ),
        (unused, b.error(Ty::Const(&ConstTy::Int))),
    ];
    let compiled = assemble(
        &a,
        &Module {
            bindings: a.alloc_slice_copy(bindings),
            root: b.var(y.name, y.ty),
        },
    )
    .unwrap();
    assert!(compiled.program.version.is_v1_1_0());
    assert_eq!(
        pretty::term(compiled.program.eval(&a).term.unwrap()),
        "(con integer 42)"
    );
}
#[test]
fn reachable_keeps_transitive_bindings_and_cycles() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    let y = binder(&b, "y");
    let dead = binder(&b, "dead");
    let bindings = &[
        (x, b.int(1)),
        (y, b.var(x.name, x.ty)),
        (dead, b.error(Ty::Const(&ConstTy::Int))),
    ];
    assert_eq!(
        reachable(bindings, b.var(y.name, y.ty))
            .iter()
            .map(|(binder, _)| binder.name)
            .collect::<Vec<_>>(),
        vec![x.name, y.name]
    );
    let cycle = &[
        (x, b.var(y.name, y.ty)),
        (y, b.var(x.name, x.ty)),
        (dead, b.error(Ty::Const(&ConstTy::Int))),
    ];
    assert_eq!(reachable(cycle, b.var(x.name, x.ty)).len(), 2);
}
#[test]
fn lexical_binders_do_not_reach_shadowed_globals() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    let bindings = &[(x, b.error(Ty::Const(&ConstTy::Int)))];
    assert!(reachable(bindings, b.lam(&[x], b.var(x.name, x.ty))).is_empty());
    assert!(
        reachable(
            bindings,
            b.case(
                CaseKind::Tag,
                b.constr(
                    0,
                    &[b.int(1)],
                    Ty::Runtime(b.arena.alloc(nash_ir::ty::RuntimeTy::Constr {
                        tag: 0,
                        fields: b.arena.alloc_slice_copy(&[Ty::Const(&ConstTy::Int)])
                    }))
                ),
                &[Branch {
                    test: Test::Tag(0),
                    binders: a.alloc_slice_copy(&[x]),
                    body: b.var(x.name, x.ty)
                }],
                None,
                Ty::Const(&ConstTy::Int)
            )
        )
        .is_empty()
    );
    // A strict let's value is outside the new binder's scope.
    assert_eq!(
        reachable(
            bindings,
            b.let_(x, b.var(x.name, x.ty), b.var(x.name, x.ty))
        )
        .len(),
        1
    );
}
#[test]
fn recursive_scopes_bind_group_and_parameters() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "f");
    let g = function(&b, "g");
    let n = binder(&b, "n");
    let outside = binder(&b, "outside");
    let group = b.let_rec(
        &[
            RecBinder {
                binder: f,
                params: a.alloc_slice_copy(&[n]),
                static_params: &[],
                body: b.app(
                    b.var(g.name, g.ty),
                    &[b.var(n.name, n.ty)],
                    Ty::Const(&ConstTy::Int),
                ),
            },
            RecBinder {
                binder: g,
                params: a.alloc_slice_copy(&[n]),
                static_params: &[],
                body: b.var(outside.name, outside.ty),
            },
        ],
        b.app(b.var(f.name, f.ty), &[b.int(0)], Ty::Const(&ConstTy::Int)),
    );
    assert_eq!(free_variables(group), vec![outside.name]);
    let compiled = assemble(
        &a,
        &Module {
            bindings: a.alloc_slice_copy(&[(outside, b.int(9))]),
            root: group,
        },
    )
    .unwrap();
    assert_eq!(
        pretty::term(compiled.program.eval(&a).term.unwrap()),
        "(con integer 9)"
    );
}
#[test]
fn closedness_and_duplicate_bindings_are_reported() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "missing");
    assert!(matches!(assemble_core(&a,b.var(x.name, x.ty)),Err(Error::NotClosed(n)) if n==x.name));
    let module = Module {
        bindings: a.alloc_slice_copy(&[(x, b.int(1)), (x, b.int(2))]),
        root: b.var(x.name, x.ty),
    };
    assert!(matches!(assemble(&a,&module),Err(Error::DuplicateBinding(n)) if n==x.name));
}
#[test]
fn validator_style_lambda_arguments_remain_callable() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let threshold = binder(&b, "threshold");
    let datum = Binder {
        name: b.fresh("datum"),
        ty: Ty::Big(&BigTy::Data),
    };
    let root = b.lam(
        &[threshold, datum],
        b.if_(
            b.builtin(
                F::EqualsInteger,
                &[b.var(threshold.name, threshold.ty), b.int(41)],
                Ty::Const(&ConstTy::Bool),
            ),
            b.lit(Constant::unit(&a)),
            b.error(Ty::Const(&ConstTy::Unit)),
        ),
    );
    let compiled = assemble_core(&a, root).unwrap();
    let program = compiled
        .program
        .apply(&a, Term::integer_from(&a, 41))
        .apply(&a, Term::data_integer_from(&a, 7));
    assert_eq!(
        pretty::term(program.eval(&a).term.unwrap()),
        "(con unit ())"
    );
}
#[test]
fn comptime_folds_with_reachable_dependencies() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    let dead = binder(&b, "dead");
    let core = b.builtin(
        F::AddInteger,
        &[b.var(x.name, x.ty), b.int(2)],
        Ty::Const(&ConstTy::Int),
    );
    let c = comptime::eval_closed(
        &a,
        &[(x, b.int(40)), (dead, b.error(Ty::Const(&ConstTy::Int)))],
        core,
    )
    .unwrap();
    assert_eq!(nash_ir::pretty::pretty(b.lit(c)), "42");
}
#[test]
fn comptime_reports_open_terms_errors_and_nonconstants() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    assert!(
        matches!(comptime::eval_closed(&a,&[],b.var(x.name, x.ty)),Err(ComptimeError::NotClosed(n)) if n==x.name)
    );
    assert!(matches!(
        comptime::eval_closed(&a, &[], b.lam(&[x], b.var(x.name, x.ty))),
        Err(ComptimeError::NotAConstant)
    ));
    let fail = b.trace(
        b.lit(Constant::string(&a, "x")),
        b.error(Ty::Const(&ConstTy::Int)),
    );
    assert_eq!(
        comptime::eval_closed(&a, &[], fail)
            .unwrap_err()
            .to_string(),
        "comptime evaluation failed: ExplicitErrorTerm\ntraces: x"
    );
}
#[test]
fn comptime_infinite_recursion_exhausts_default_budget() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let f = function(&b, "loop");
    let n = binder(&b, "n");
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: a.alloc_slice_copy(&[n]),
            static_params: &[0],
            body: b.app(
                b.var(f.name, f.ty),
                &[b.var(n.name, n.ty)],
                Ty::Const(&ConstTy::Int),
            ),
        }],
        b.app(b.var(f.name, f.ty), &[b.int(0)], Ty::Const(&ConstTy::Int)),
    );
    let Err(ComptimeError::Evaluation(reason)) = comptime::eval_closed(&a, &[], core) else {
        panic!("expected bounded evaluation failure")
    };
    assert!(reason.contains("OutOfExError"), "{reason}");
}

#[test]
fn assemble_targets_plutus_v3() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let compiled = assemble_core(&a, b.int(42)).unwrap();
    assert!(compiled.program.version.is_v1_1_0());
    assert_eq!(
        pretty::term(
            compiled
                .program
                .eval_version(&a, PlutusVersion::V3)
                .term
                .unwrap()
        ),
        "(con integer 42)"
    );
    let constr = b.constr(
        0,
        &[b.int(1)],
        Ty::Runtime(b.arena.alloc(nash_ir::ty::RuntimeTy::Constr {
            tag: 0,
            fields: b.arena.alloc_slice_copy(&[Ty::Const(&ConstTy::Int)]),
        })),
    );
    assert!(assemble_core(&a, constr).is_ok());
    let newer = b.builtin(
        F::ExpModInteger,
        &[b.int(2), b.int(3), b.int(5)],
        Ty::Const(&ConstTy::Int),
    );
    assert!(assemble_core(&a, newer).is_ok());
}

#[test]
fn o1_assembly_matches_snapshot_pipeline_and_preserves_traces() {
    use nash_config::OptimizationLevel;
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let int = Ty::Const(&ConstTy::Int);
    let data = Ty::Big(&BigTy::Data);
    for fails in [false, true] {
        let value = b.builtin(F::UnIData, &[b.builtin(F::IData, &[b.int(42)], data)], int);
        let root = b.trace(
            b.lit(Constant::string(&arena, "before")),
            if fails { b.error(int) } else { value },
        );
        let prepared = crate::snapshot_optimizer::prepare(&arena, root);
        let o1 = assemble_core_with_options(&arena, root, OptimizationLevel::O1).unwrap();
        let before = prepared.before.program.eval(&arena);
        let after = o1.program.eval(&arena);
        insta::assert_snapshot!(
            format!("o1_traces_failure_{fails}"),
            format!(
                "{}\n--- O0 outcome\n{:?}\n--- O0 logs\n{:?}\n--- O1 outcome\n{:?}\n--- O1 logs\n{:?}",
                prepared.snapshot(),
                before.term.as_ref().map(|t| pretty::term(t)),
                before.info.logs,
                after.term.as_ref().map(|t| pretty::term(t)),
                after.info.logs
            )
        );
        assert_eq!(
            nash_plutus::flat::encode(prepared.after.program).unwrap(),
            nash_plutus::flat::encode(o1.program).unwrap()
        );
        assert_eq!(
            before
                .term
                .as_ref()
                .map(|t| pretty::term(t))
                .map_err(|e| format!("{e:?}")),
            after
                .term
                .as_ref()
                .map(|t| pretty::term(t))
                .map_err(|e| format!("{e:?}"))
        );
        assert_eq!(before.info.logs, after.info.logs);
    }
}

#[test]
fn deep_assembly_uses_heap_work_lists() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let arena = Arena::new();
            let b = Builder::new(&arena);
            let message = b.lit(Constant::string(&arena, "tick"));
            let mut core = b.int(1);
            for _ in 0..4096 {
                core = b.trace(message, core);
            }
            let mut outcomes = Vec::new();
            for level in [
                nash_config::OptimizationLevel::O0,
                nash_config::OptimizationLevel::O1,
            ] {
                let compiled = assemble_core_with_options(&arena, core, level).unwrap();
                nash_plutus::flat::encode(compiled.program).unwrap();
                let result = compiled.program.eval(&arena);
                outcomes.push((pretty::term(result.term.unwrap()), result.info.logs));
            }
            assert_eq!(outcomes[0], outcomes[1]);
        })
        .unwrap()
        .join()
        .unwrap();
}
