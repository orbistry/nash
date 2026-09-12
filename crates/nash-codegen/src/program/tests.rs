use super::*;
use crate::comptime::{self, ComptimeError};
use nash_ir::{
    core::{Branch, CaseKind, RecBinder, Test},
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant, pretty};

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
        (y, b.builtin(F::AddInteger, &[b.var(x.name), b.int(2)])),
        (unused, b.error()),
    ];
    let compiled = assemble(
        &a,
        &Module {
            bindings: a.alloc_slice_copy(bindings),
            root: b.var(y.name),
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
    let bindings = &[(x, b.int(1)), (y, b.var(x.name)), (dead, b.error())];
    assert_eq!(
        reachable(bindings, b.var(y.name))
            .iter()
            .map(|(binder, _)| binder.name)
            .collect::<Vec<_>>(),
        vec![x.name, y.name]
    );
    let cycle = &[(x, b.var(y.name)), (y, b.var(x.name)), (dead, b.error())];
    assert_eq!(reachable(cycle, b.var(x.name)).len(), 2);
}
#[test]
fn lexical_binders_do_not_reach_shadowed_globals() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    let bindings = &[(x, b.error())];
    assert!(reachable(bindings, b.lam(&[x], b.var(x.name))).is_empty());
    assert!(
        reachable(
            bindings,
            b.case(
                CaseKind::Tag,
                b.constr(0, &[b.int(1)]),
                &[Branch {
                    test: Test::Tag(0),
                    binders: a.alloc_slice_copy(&[x]),
                    body: b.var(x.name)
                }],
                None
            )
        )
        .is_empty()
    );
    // A strict let's value is outside the new binder's scope.
    assert_eq!(
        reachable(bindings, b.let_(x, b.var(x.name), b.var(x.name))).len(),
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
                body: b.app(b.var(g.name), &[b.var(n.name)]),
            },
            RecBinder {
                binder: g,
                params: a.alloc_slice_copy(&[n]),
                static_params: &[],
                body: b.var(outside.name),
            },
        ],
        b.app(b.var(f.name), &[b.int(0)]),
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
    assert!(matches!(assemble_core(&a,b.var(x.name)),Err(Error::NotClosed(n)) if n==x.name));
    let module = Module {
        bindings: a.alloc_slice_copy(&[(x, b.int(1)), (x, b.int(2))]),
        root: b.var(x.name),
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
            b.builtin(F::EqualsInteger, &[b.var(threshold.name), b.int(41)]),
            b.lit(Constant::unit(&a)),
            b.error(),
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
    let core = b.builtin(F::AddInteger, &[b.var(x.name), b.int(2)]);
    let c = comptime::eval_closed(&a, &[(x, b.int(40)), (dead, b.error())], core).unwrap();
    assert_eq!(nash_ir::pretty::pretty(b.lit(c)), "42");
}
#[test]
fn comptime_reports_open_terms_errors_and_nonconstants() {
    let a = Arena::new();
    let b = Builder::new(&a);
    let x = binder(&b, "x");
    assert!(
        matches!(comptime::eval_closed(&a,&[],b.var(x.name)),Err(ComptimeError::NotClosed(n)) if n==x.name)
    );
    assert!(matches!(
        comptime::eval_closed(&a, &[], b.lam(&[x], b.var(x.name))),
        Err(ComptimeError::NotAConstant)
    ));
    let fail = b.trace(b.lit(Constant::string(&a, "x")), b.error());
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
            body: b.app(b.var(f.name), &[b.var(n.name)]),
        }],
        b.app(b.var(f.name), &[b.int(0)]),
    );
    let Err(ComptimeError::Evaluation(reason)) = comptime::eval_closed(&a, &[], core) else {
        panic!("expected bounded evaluation failure")
    };
    assert!(reason.contains("OutOfExError"), "{reason}");
}
