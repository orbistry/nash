//! Candidate-only semantic checks. Production assembly remains unchanged.
use nash_ir::{
    anf,
    build::Builder,
    core::*,
    hygiene,
    pretty::pretty,
    ty::{ConstTy, RuntimeTy, TermTy, Ty},
};
use nash_plutus::{arena::Arena, builtin::DefaultFunction as F, constant::Constant};

const INT: Ty<'static> = Ty::Const(&ConstTy::Int);

fn binder<'a>(b: &Builder<'a>, name: &'a str, ty: Ty<'a>) -> Binder<'a> {
    Binder {
        name: b.fresh(name),
        ty,
    }
}
fn traced<'a>(b: &Builder<'a>, message: &'a str, value: &'a Core<'a>) -> &'a Core<'a> {
    b.trace(b.lit(Constant::string(b.arena, message)), value)
}
fn constr<'a>(b: &Builder<'a>, tag: u16, fields: &[&'a Core<'a>]) -> &'a Core<'a> {
    let types: Vec<_> = fields.iter().map(|field| field.ty).collect();
    b.constr(
        tag,
        fields,
        Ty::Runtime(b.arena.alloc(RuntimeTy::Constr {
            tag,
            fields: b.arena.alloc_slice_copy(&types),
        })),
    )
}

use crate::harness::candidate;

fn check(name: &str, b: &Builder<'_>, core: &Core<'_>, fails: bool) {
    let before = crate::recursion::rewrite(b, core).unwrap();
    let fresh = hygiene::freshen(b, core);
    let lifted = nash_ir::static_lift::lift(b, fresh);
    let first_anf = anf::normalize(b, lifted);
    let rewritten = crate::recursion::rewrite(b, first_anf).unwrap();
    let after = hygiene::freshen(b, rewritten);

    let baseline = crate::harness::eval_core_raw(b.arena, before);
    let normalized = crate::harness::eval_core_raw(b.arena, after);

    assert_eq!(normalized.result.starts_with("error:"), fails);
    insta::assert_snapshot!(
        name,
        crate::harness::pass_snapshot(
            b.arena,
            core,
            format!(
                "--- core before static lifting\n{}\n--- core after static lifting / before ANF\n{}\n--- core after ANF\n{}\n--- rewritten core (no second ANF)\n{}\n--- baseline\n{}\n--- candidate\n{}",
                pretty(fresh),
                pretty(lifted),
                pretty(first_anf),
                pretty(after),
                semantic_output(&baseline),
                semantic_output(&normalized),
            )
        )
    );
    for phase in [fresh, lifted, first_anf, after] {
        hygiene::validate(phase, &[]).unwrap();
    }
    anf::validate(first_anf).unwrap();
    // Properties independent of the expected snapshot.
    assert_eq!(core.ty, after.ty);
    assert_eq!(baseline.observable, normalized.observable);
    assert_eq!(baseline.logs, normalized.logs);
}

fn semantic_output(value: &crate::harness::Evaluated) -> String {
    format!(
        "--- uplc\n{}\n--- result\n{}\n--- logs\n{:?}",
        value.uplc, value.result, value.logs
    )
}

#[test]
fn staged_application_runs_intermediate_body_before_later_argument() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "x", INT);
    let y = binder(&b, "y", INT);
    let f = b.lam(
        &[x],
        traced(&b, "intermediate", b.lam(&[y], b.var(y.name, y.ty))),
    );
    let core = b.app(
        traced(&b, "function", f),
        &[
            traced(&b, "first", b.int(1)),
            traced(&b, "second", b.int(42)),
        ],
        INT,
    );
    check("staged_application", &b, core, false);
}

#[test]
fn intermediate_failure_prevents_later_argument() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "x", INT);
    let result_fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[INT]), INT)));
    let f = b.lam(&[x], traced(&b, "intermediate", b.error(result_fun)));
    let core = b.app(
        f,
        &[
            traced(&b, "first", b.int(1)),
            traced(&b, "unreachable", b.int(2)),
        ],
        INT,
    );
    check("intermediate_failure", &b, core, true);
}

#[test]
fn saturated_builtin_failure_prevents_oversaturated_argument() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    // The first application already fails; the invalid extra application is
    // unreachable. This probes sequencing independently of source typechecking.
    let core = b.app(
        b.builtin(
            F::DivideInteger,
            &[traced(&b, "numerator", b.int(1)), b.int(0)],
            INT,
        ),
        &[traced(&b, "unreachable", b.int(2))],
        INT,
    );
    check("builtin_intermediate_failure", &b, core, true);
}

#[test]
fn trace_message_is_evaluated_before_emit_and_body() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let message = traced(
        &b,
        "message evaluation",
        b.lit(Constant::string(&arena, "outer")),
    );
    let core = b.trace(message, traced(&b, "body", b.error(INT)));
    check("trace_timing", &b, core, true);
}

#[test]
fn tag_subject_is_evaluated_once_and_branch_work_stays_selected() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "field", INT);
    let core = b.case(
        CaseKind::Tag,
        traced(
            &b,
            "subject",
            constr(&b, 1, &[traced(&b, "field", b.int(42))]),
        ),
        &[
            Branch {
                test: Test::Tag(0),
                binders: &[],
                body: traced(&b, "unselected", b.error(INT)),
            },
            Branch {
                test: Test::Tag(1),
                binders: arena.alloc_slice_copy(&[x]),
                body: traced(&b, "selected", b.var(x.name, x.ty)),
            },
        ],
        None,
        INT,
    );
    check("tag_subject_once", &b, core, false);
}

#[test]
fn ignored_constructor_field_stays_strict() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let core = b.field(
        constr(
            &b,
            0,
            &[b.int(42), traced(&b, "ignored field", b.error(INT))],
        ),
        0,
        2,
        INT,
    );
    check("ignored_field_strict", &b, core, true);
}

#[test]
fn unused_lambda_and_delay_keep_their_effects_suspended() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let x = binder(&b, "x", INT);
    let fun = b.lam(&[x], traced(&b, "lambda body", b.error(INT)));
    let delayed = b.delay(traced(&b, "delay body", b.error(INT)));
    let f = binder(&b, "unused function", fun.ty);
    let d = binder(&b, "unused delay", delayed.ty);
    let core = b.let_(f, fun, b.let_(d, delayed, b.int(42)));
    check("suspended_scopes", &b, core, false);
}

#[test]
fn force_runs_delayed_work_at_the_force_site() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let delayed = b.delay(traced(&b, "delayed body", b.int(42)));
    let d = binder(&b, "delay", delayed.ty);
    let core = b.let_(
        d,
        delayed,
        traced(&b, "before force", b.force(b.var(d.name, d.ty), INT)),
    );
    check("force_timing", &b, core, false);
}

#[test]
fn explicit_recursion_preserves_order() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let function_ty = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[INT]), INT)));
    let f = binder(&b, "countdown", function_ty);
    let n = binder(&b, "n", INT);
    let value = b.var(n.name, n.ty);
    let body = traced(
        &b,
        "step",
        b.if_(
            b.builtin(
                F::EqualsInteger,
                &[value, b.int(0)],
                Ty::Const(&ConstTy::Bool),
            ),
            b.int(42),
            b.app(
                b.var(f.name, f.ty),
                &[b.builtin(F::SubtractInteger, &[value, b.int(1)], INT)],
                INT,
            ),
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[n]),
            static_params: &[],
            body,
        }],
        b.app(b.var(f.name, f.ty), &[b.int(2)], INT),
    );
    check("recursive_countdown", &b, core, false);
}

#[test]
fn multiargument_recursion_keeps_static_values_and_argument_order() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let function_ty = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[INT, INT]), INT)));
    let f = binder(&b, "countdown", function_ty);
    let fixed = binder(&b, "fixed", INT);
    let n = binder(&b, "n", INT);
    let value = b.var(n.name, n.ty);
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[value, b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        b.var(fixed.name, fixed.ty),
        b.app(
            b.var(f.name, f.ty),
            &[
                b.var(fixed.name, fixed.ty),
                traced(
                    &b,
                    "next",
                    b.builtin(F::SubtractInteger, &[value, b.int(1)], INT),
                ),
            ],
            INT,
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[fixed, n]),
            static_params: &[0],
            body,
        }],
        b.app(
            b.var(f.name, f.ty),
            &[
                traced(&b, "fixed", b.int(42)),
                traced(&b, "initial", b.int(2)),
            ],
            INT,
        ),
    );
    check("multiargument_recursion", &b, core, false);
}

fn function_ty<'a>(arena: &'a Arena, params: &[Ty<'a>], result: Ty<'a>) -> Ty<'a> {
    Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(params), result)))
}

#[test]
fn static_last_parameter_and_nested_capture_survive_lifting() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let outer = binder(&b, "outer", INT);
    let f = binder(&b, "countdown", function_ty(&arena, &[INT, INT], INT));
    let n = binder(&b, "n", INT);
    let fixed = binder(&b, "fixed", INT);
    let result = b.builtin(
        F::AddInteger,
        &[b.var(fixed.name, INT), b.var(outer.name, INT)],
        INT,
    );
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[b.var(n.name, INT), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        result,
        b.app(
            b.var(f.name, f.ty),
            &[
                traced(
                    &b,
                    "next",
                    b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
                ),
                b.var(fixed.name, INT),
            ],
            INT,
        ),
    );
    let core = b.let_(
        outer,
        traced(&b, "capture", b.int(2)),
        b.let_rec(
            &[RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[n, fixed]),
                static_params: &[1],
                body,
            }],
            b.app(
                b.var(f.name, f.ty),
                &[b.int(2), traced(&b, "fixed", b.int(40))],
                INT,
            ),
        ),
    );
    check("static_last_nested_capture", &b, core, false);
}

#[test]
fn all_static_worker_is_not_forced_in_a_dead_branch() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let f = binder(&b, "same", function_ty(&arena, &[INT], INT));
    let x = binder(&b, "x", INT);
    let body = b.if_(
        b.lit(Constant::bool(&arena, true)),
        traced(&b, "selected", b.var(x.name, INT)),
        b.app(b.var(f.name, f.ty), &[b.var(x.name, INT)], INT),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[x]),
            static_params: &[0],
            body,
        }],
        b.app(
            b.var(f.name, f.ty),
            &[traced(&b, "argument", b.int(42))],
            INT,
        ),
    );
    check("all_static_dead_recursion", &b, core, false);
}

#[test]
fn all_static_worker_preserves_failure_before_live_recursion() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let result_ty = function_ty(&arena, &[INT], INT);
    let f = binder(&b, "same", function_ty(&arena, &[INT], result_ty));
    let x = binder(&b, "x", INT);
    let failed = binder(&b, "failed", INT);
    let body = b.let_(
        failed,
        traced(&b, "failure", b.error(INT)),
        b.app(b.var(f.name, f.ty), &[b.var(x.name, INT)], result_ty),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[x]),
            static_params: &[0],
            body,
        }],
        b.app(
            b.var(f.name, f.ty),
            &[
                traced(&b, "argument", b.int(42)),
                traced(&b, "unreachable extra", b.int(1)),
            ],
            INT,
        ),
    );
    check("all_static_failure_timing", &b, core, true);
}

#[test]
fn genuine_partial_recursive_use_remains_supported() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let f = binder(&b, "countdown", function_ty(&arena, &[INT, INT], INT));
    let x = binder(&b, "x", INT);
    let n = binder(&b, "n", INT);
    let partial = binder(&b, "partial", function_ty(&arena, &[INT], INT));
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[b.var(n.name, INT), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        b.var(x.name, INT),
        b.let_(
            partial,
            b.app(b.var(f.name, f.ty), &[b.var(x.name, INT)], partial.ty),
            b.app(
                b.var(partial.name, partial.ty),
                &[traced(
                    &b,
                    "next",
                    b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
                )],
                INT,
            ),
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[x, n]),
            static_params: &[],
            body,
        }],
        b.app(b.var(f.name, f.ty), &[b.int(42), b.int(2)], INT),
    );
    let fresh = hygiene::freshen(&b, core);
    assert_eq!(pretty(fresh), pretty(nash_ir::static_lift::lift(&b, fresh)));
    check("genuine_partial_recursion", &b, core, false);
}

#[test]
fn oversaturated_recursive_result_keeps_argument_effect_order() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let result_ty = function_ty(&arena, &[INT], INT);
    let f = binder(&b, "countdown", function_ty(&arena, &[INT, INT], result_ty));
    let x = binder(&b, "x", INT);
    let n = binder(&b, "n", INT);
    let y = binder(&b, "y", INT);
    let body = traced(
        &b,
        "step",
        b.if_(
            b.builtin(
                F::EqualsInteger,
                &[b.var(n.name, INT), b.int(0)],
                Ty::Const(&ConstTy::Bool),
            ),
            b.lam(
                &[y],
                traced(
                    &b,
                    "returned function",
                    b.builtin(
                        F::AddInteger,
                        &[b.var(x.name, INT), b.var(y.name, INT)],
                        INT,
                    ),
                ),
            ),
            b.app(
                b.var(f.name, f.ty),
                &[
                    b.var(x.name, INT),
                    b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
                ],
                result_ty,
            ),
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[x, n]),
            static_params: &[0],
            body,
        }],
        b.app(
            b.var(f.name, f.ty),
            &[
                traced(&b, "fixed", b.int(40)),
                traced(&b, "initial", b.int(2)),
                traced(&b, "extra", b.int(2)),
            ],
            INT,
        ),
    );
    check("static_lift_oversaturation", &b, core, false);
}

#[test]
fn mutually_recursive_group_is_left_for_dispatch_rewriting() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let ty = function_ty(&arena, &[INT, INT], INT);
    let f = binder(&b, "even", ty);
    let g = binder(&b, "odd", ty);
    let x = binder(&b, "x", INT);
    let n = binder(&b, "n", INT);
    let y = binder(&b, "y", INT);
    let m = binder(&b, "m", INT);
    fn body<'a>(
        b: &Builder<'a>,
        other: Binder<'a>,
        fixed: Binder<'a>,
        count: Binder<'a>,
    ) -> &'a Core<'a> {
        b.if_(
            b.builtin(
                F::EqualsInteger,
                &[b.var(count.name, INT), b.int(0)],
                Ty::Const(&ConstTy::Bool),
            ),
            b.var(fixed.name, INT),
            b.app(
                b.var(other.name, other.ty),
                &[
                    b.var(fixed.name, INT),
                    b.builtin(F::SubtractInteger, &[b.var(count.name, INT), b.int(1)], INT),
                ],
                INT,
            ),
        )
    }
    let core = b.let_rec(
        &[
            RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[x, n]),
                static_params: &[],
                body: body(&b, g, x, n),
            },
            RecBinder {
                binder: g,
                params: arena.alloc_slice_copy(&[y, m]),
                static_params: &[],
                body: body(&b, f, y, m),
            },
        ],
        b.app(b.var(f.name, ty), &[b.int(42), b.int(3)], INT),
    );
    let fresh = hygiene::freshen(&b, core);
    assert_eq!(pretty(fresh), pretty(nash_ir::static_lift::lift(&b, fresh)));
    check("mutual_static_lift_fallback", &b, core, false);
}

#[test]
fn escaping_recursive_function_keeps_the_original_interface() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let ty = function_ty(&arena, &[INT, INT], INT);
    let f = binder(&b, "countdown", ty);
    let x = binder(&b, "x", INT);
    let n = binder(&b, "n", INT);
    let alias = binder(&b, "escaped", ty);
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[b.var(n.name, INT), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        b.var(x.name, INT),
        b.let_(
            alias,
            b.var(f.name, ty),
            b.app(
                b.var(alias.name, ty),
                &[
                    b.var(x.name, INT),
                    b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
                ],
                INT,
            ),
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[x, n]),
            static_params: &[],
            body,
        }],
        b.app(b.var(f.name, ty), &[b.int(42), b.int(2)], INT),
    );
    let fresh = hygiene::freshen(&b, core);
    assert_eq!(pretty(fresh), pretty(nash_ir::static_lift::lift(&b, fresh)));
    check("escaping_recursion_fallback", &b, core, false);
}

#[test]
fn lifted_wrapper_evaluates_unused_static_argument_before_dynamic_failure() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let ty = function_ty(&arena, &[INT, INT], INT);
    let f = binder(&b, "countdown", ty);
    let x = binder(&b, "x", INT);
    let n = binder(&b, "n", INT);
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[b.var(n.name, INT), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        b.int(42),
        b.app(
            b.var(f.name, ty),
            &[
                b.var(x.name, INT),
                b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
            ],
            INT,
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[x, n]),
            static_params: &[0],
            body,
        }],
        b.app(
            b.var(f.name, ty),
            &[
                traced(&b, "static argument", b.int(1)),
                traced(&b, "dynamic failure", b.error(INT)),
            ],
            INT,
        ),
    );
    check("static_argument_strictness", &b, core, true);
}

#[test]
fn multiple_static_parameters_preserve_dynamic_and_initial_argument_order() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let ty = function_ty(&arena, &[INT, INT, INT, INT], INT);
    let f = binder(&b, "countdown", ty);
    let config = binder(&b, "config", INT);
    let n = binder(&b, "n", INT);
    let limit = binder(&b, "limit", INT);
    let acc = binder(&b, "acc", INT);
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[b.var(n.name, INT), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        b.builtin(
            F::AddInteger,
            &[
                b.var(config.name, INT),
                b.builtin(
                    F::AddInteger,
                    &[b.var(limit.name, INT), b.var(acc.name, INT)],
                    INT,
                ),
            ],
            INT,
        ),
        b.app(
            b.var(f.name, ty),
            &[
                b.var(config.name, INT),
                traced(
                    &b,
                    "next n",
                    b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
                ),
                b.var(limit.name, INT),
                traced(
                    &b,
                    "next acc",
                    b.builtin(F::AddInteger, &[b.var(acc.name, INT), b.int(1)], INT),
                ),
            ],
            INT,
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[config, n, limit, acc]),
            static_params: &[0, 2],
            body,
        }],
        b.app(
            b.var(f.name, ty),
            &[
                traced(&b, "config", b.int(40)),
                traced(&b, "n", b.int(2)),
                traced(&b, "limit", b.int(2)),
                traced(&b, "acc", b.int(0)),
            ],
            INT,
        ),
    );
    check("interleaved_static_parameters", &b, core, false);
}

#[test]
fn optimized_recursion_lowers_without_renormalizing_self_application() {
    let arena = Arena::new();
    let b = Builder::new(&arena);
    let f = binder(&b, "countdown", function_ty(&arena, &[INT], INT));
    let n = binder(&b, "n", INT);
    let body = b.if_(
        b.builtin(
            F::EqualsInteger,
            &[b.var(n.name, INT), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        ),
        b.int(42),
        b.app(
            b.var(f.name, f.ty),
            &[traced(
                &b,
                "step",
                b.builtin(F::SubtractInteger, &[b.var(n.name, INT), b.int(1)], INT),
            )],
            INT,
        ),
    );
    let core = b.let_rec(
        &[RecBinder {
            binder: f,
            params: arena.alloc_slice_copy(&[n]),
            static_params: &[],
            body,
        }],
        b.app(b.var(f.name, f.ty), &[b.int(2)], INT),
    );
    let before = crate::recursion::rewrite(&b, core).unwrap();
    let after = candidate(&arena, core);
    let baseline = crate::harness::eval_core_raw(&arena, before);
    let optimized = crate::harness::eval_core_raw(&arena, after);
    assert!(
        !optimized.result.starts_with("error:"),
        "{}",
        optimized.result
    );

    insta::assert_snapshot!(crate::harness::pass_snapshot(
        b.arena,
        core,
        format!(
            "--- core before\n{}\n--- core after\n{}\n--- baseline\n{}\n--- optimized\n{}",
            pretty(core),
            pretty(after),
            semantic_output(&baseline),
            semantic_output(&optimized),
        )
    ));
    assert_eq!(baseline.logs, optimized.logs);
    assert_eq!(baseline.observable, optimized.observable);
    // Generated self-application remains nested. Lowering accepts this shape.
    assert!(anf::validate(after).is_err());
}
