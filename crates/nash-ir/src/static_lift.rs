//! Lift unchanged self-call parameters while retaining explicit recursion.
//!
//! Use globally unique binders. Only singleton recursive groups
//! are lifted; mutual recursion retains its existing dispatcher treatment.
use crate::{
    build::Builder,
    core::*,
    ty::{RuntimeTy, TermTy, Ty},
};
use std::collections::{HashMap, HashSet};

/// Find unchanged parameters across every saturated self-call. First-class and
/// partial uses prevent lifting. Input binding IDs must be globally unique.
pub fn static_params(f: Name<'_>, params: &[Binder<'_>], body: &Core<'_>) -> Vec<usize> {
    let mut candidates: Vec<_> = (0..params.len()).collect();
    let (mut calls, mut uses) = (0, 0);
    body.walk(&mut |node| match node.kind {
        CoreKind::App { func: Core { kind: CoreKind::Var(g), .. }, args } if g.unique == f.unique => {
            calls += 1;
            if args.len() < params.len() { candidates.clear(); }
            candidates.retain(|&i| matches!(args.get(i), Some(Core { kind: CoreKind::Var(v), .. }) if v.unique == params[i].name.unique));
        }
        CoreKind::Var(g) if g.unique == f.unique => uses += 1,
        _ => {}
    });
    if calls > 0 && calls == uses {
        candidates
    } else {
        Vec::new()
    }
}

/// Capture static parameters in an ordinary wrapper and introduce a recursive
/// worker accepting only dynamic parameters. All-static workers are delayed
/// recursive values, forced at each original call (never memoized).
/// On ANF input, generated calls retain atomic operands. Cleanup must flatten
/// binding prefixes introduced inside existing strict binding values.
pub fn lift<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let report = crate::analysis::occurrences(core);
    let mut used: HashSet<_> = report
        .bindings
        .iter()
        .map(|x| x.name.unique)
        .chain(report.uses.iter().map(|x| x.name.unique))
        .collect();
    let mut fresh = |text, ty| loop {
        let name = b.fresh(text);
        if used.insert(name.unique) {
            break Binder { name, ty };
        }
    };
    core.map(b, &mut |node| {
        let CoreKind::LetRec {
            binders: [single],
            body,
        } = node.kind
        else {
            return None;
        };
        let statics = static_params(single.binder.name, single.params, single.body);
        if statics.is_empty() {
            return None;
        }
        let dynamic: Vec<_> = single
            .params
            .iter()
            .enumerate()
            .filter(|(i, _)| !statics.contains(i))
            .map(|(_, p)| *p)
            .collect();
        let worker_params: Vec<_> = dynamic.iter().map(|p| fresh(p.name.text, p.ty)).collect();
        let renamed: HashMap<_, _> = dynamic
            .iter()
            .zip(&worker_params)
            .map(|(a, b)| (a.name.unique, *b))
            .collect();
        let result = single.body.ty;
        let worker_ty = if dynamic.is_empty() {
            Ty::Runtime(b.arena.alloc(RuntimeTy::Delay(result)))
        } else {
            let types: Vec<_> = dynamic.iter().map(|p| p.ty).collect();
            Ty::Term(
                b.arena
                    .alloc(TermTy::Fun(b.arena.alloc_slice_copy(&types), result)),
            )
        };
        let worker = fresh("worker", worker_ty);
        let mut call = |args: &[&'a Core<'a>], ty| {
            let target = b.var(worker.name, worker.ty);
            if dynamic.is_empty() {
                let forced = b.force(target, result);
                if args.is_empty() {
                    b.with_type(forced, ty)
                } else {
                    let value = fresh("forced", result);
                    b.let_(value, forced, b.app(b.var(value.name, result), args, ty))
                }
            } else {
                b.app(target, args, ty)
            }
        };
        let worker_body = single.body.map(b, &mut |node| match node.kind {
            CoreKind::Var(n) => renamed.get(&n.unique).map(|p| b.var(p.name, node.ty)),
            CoreKind::App {
                func:
                    Core {
                        kind: CoreKind::Var(n),
                        ..
                    },
                args,
            } if n.unique == single.binder.name.unique => {
                let args: Vec<_> = args
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !statics.contains(i))
                    .map(|(_, a)| *a)
                    .collect();
                Some(call(&args, node.ty))
            }
            _ => None,
        });
        let args: Vec<_> = dynamic.iter().map(|p| b.var(p.name, p.ty)).collect();
        let definition = b.lam(
            single.params,
            b.let_rec(
                &[RecBinder {
                    binder: worker,
                    params: b.arena.alloc_slice_copy(&worker_params),
                    static_params: &[],
                    body: if dynamic.is_empty() {
                        b.delay(worker_body)
                    } else {
                        worker_body
                    },
                }],
                call(&args, result),
            ),
        );
        Some(b.with_type(
            b.let_(
                single.binder,
                b.with_type(definition, single.binder.ty),
                body,
            ),
            node.ty,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hygiene, pretty::pretty, ty::ConstTy};
    use nash_plutus::{arena::Arena, builtin::DefaultFunction};
    fn assert_lift_snapshot(name: &str, all: bool, escape: bool) {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let int = Ty::Const(&ConstTy::Int);
        let fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int, int]), int)));
        let f = Binder {
            name: b.fresh("f"),
            ty: fun,
        };
        let x = Binder {
            name: b.fresh("fixed"),
            ty: int,
        };
        let n = Binder {
            name: b.fresh("n"),
            ty: int,
        };
        let next = if all {
            b.var(n.name, int)
        } else {
            b.builtin(
                DefaultFunction::SubtractInteger,
                &[b.var(n.name, int), b.int(1)],
                int,
            )
        };
        let body = if escape {
            let residual = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int]), int)));
            let partial = Binder {
                name: b.fresh("partial"),
                ty: residual,
            };
            b.let_(
                partial,
                b.app(b.var(f.name, fun), &[b.var(x.name, int)], residual),
                b.int(42),
            )
        } else {
            b.app(b.var(f.name, fun), &[b.var(x.name, int), next], int)
        };
        let before = b.let_rec(
            &[RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[x, n]),
                static_params: &[],
                body,
            }],
            b.var(f.name, fun),
        );
        let after = lift(&b, before);
        insta::assert_snapshot!(
            name,
            format!(
                "--- core before\n{}\n--- core after\n{}",
                pretty(before),
                pretty(after)
            )
        );
        assert_eq!(hygiene::validate(after, &[]), Ok(()));
        assert_eq!(pretty(after), pretty(lift(&b, after)));
        if escape {
            assert!(std::ptr::eq(before, after));
        }
    }
    #[test]
    fn captures_static_parameter() {
        assert_lift_snapshot("captures_static_parameter", false, false);
    }
    #[test]
    fn all_static_worker_is_delayed() {
        assert_lift_snapshot("all_static_worker_is_delayed", true, false);
    }
    #[test]
    fn partial_self_call_is_unchanged() {
        assert_lift_snapshot("partial_self_call_is_unchanged", false, true);
    }
    #[test]
    fn nested_worker_captures_outer_dynamic_parameter() {
        let arena = Arena::new();
        let b = Builder::new(&arena);
        let int = Ty::Const(&ConstTy::Int);
        let fun = Ty::Term(arena.alloc(TermTy::Fun(arena.alloc_slice_copy(&[int, int]), int)));
        let binder = |name, ty| Binder {
            name: b.fresh(name),
            ty,
        };
        let f = binder("outer", fun);
        let fixed = binder("fixed", int);
        let n = binder("n", int);
        let g = binder("inner", fun);
        let capture = binder("capture", int);
        let m = binder("m", int);
        let condition = b.builtin(
            DefaultFunction::EqualsInteger,
            &[b.var(m.name, int), b.int(0)],
            Ty::Const(&ConstTy::Bool),
        );
        let inner_body = b.if_(
            condition,
            b.var(n.name, int),
            b.app(
                b.var(g.name, fun),
                &[
                    b.var(capture.name, int),
                    b.builtin(
                        DefaultFunction::SubtractInteger,
                        &[b.var(m.name, int), b.int(1)],
                        int,
                    ),
                ],
                int,
            ),
        );
        let ignored = binder("ignored", int);
        let outer_body = b.let_rec(
            &[RecBinder {
                binder: g,
                params: arena.alloc_slice_copy(&[capture, m]),
                static_params: &[],
                body: inner_body,
            }],
            b.let_(
                ignored,
                b.app(b.var(g.name, fun), &[b.var(fixed.name, int), b.int(0)], int),
                b.app(
                    b.var(f.name, fun),
                    &[
                        b.var(fixed.name, int),
                        b.builtin(
                            DefaultFunction::SubtractInteger,
                            &[b.var(n.name, int), b.int(1)],
                            int,
                        ),
                    ],
                    int,
                ),
            ),
        );
        let before = b.let_rec(
            &[RecBinder {
                binder: f,
                params: arena.alloc_slice_copy(&[fixed, n]),
                static_params: &[],
                body: outer_body,
            }],
            b.var(f.name, fun),
        );
        let after = lift(&b, before);
        insta::assert_snapshot!(format!(
            "--- core before\n{}\n--- core after\n{}",
            pretty(before),
            pretty(after)
        ));
        assert_eq!(hygiene::validate(after, &[]), Ok(()));
        assert_eq!(pretty(after), pretty(lift(&b, after)));
    }
}
