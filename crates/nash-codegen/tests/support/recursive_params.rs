//! Recursive parameter fixtures shared with the explicit performance workspace.
use nash_ir::{
    build::Builder,
    core::*,
    ty::{ConstTy, TermTy, Ty},
};
use nash_plutus::{builtin::DefaultFunction as F, constant::Constant};

pub fn cases<'a>(b: &Builder<'a>) -> Vec<(&'static str, &'a Core<'a>, bool)> {
    let a = b.arena;
    let int = b.int(0).ty;
    let bool_ = Ty::Const(&ConstTy::Bool);
    let fun = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[int, int]), int)));
    let bind = |text, ty| Binder {
        name: b.fresh(text),
        ty,
    };
    let trace = |text, value| b.trace(b.lit(Constant::string(a, text)), value);
    let mut cases = Vec::new();
    for (name, mutual, consumed, effects, failure) in [
        ("self_forwarding", false, false, false, false),
        ("mutual_forwarding", true, false, false, false),
        ("mutual_consumed", true, true, false, false),
        ("recursive_trace_order", true, false, true, false),
        ("recursive_failure_order", true, false, true, true),
    ] {
        let f = bind("f", fun);
        let g = bind("g", fun);
        let x = bind("x", int);
        let n = bind("n", int);
        let m = bind("m", int);
        let y = bind("y", int);
        let step = |target: Binder<'a>, unused: Binder<'a>, count: Binder<'a>, swapped: bool| {
            let next = b.builtin(F::SubtractInteger, &[b.var(count.name, int), b.int(1)], int);
            let discarded = if failure {
                trace("discarded", b.error(int))
            } else if effects {
                trace("discarded", b.int(7))
            } else {
                b.var(unused.name, int)
            };
            let next = if effects {
                trace("retained", next)
            } else {
                next
            };
            let args = if swapped {
                [next, discarded]
            } else {
                [discarded, next]
            };
            b.if_(
                b.builtin(F::EqualsInteger, &[b.var(count.name, int), b.int(0)], bool_),
                if consumed {
                    b.var(unused.name, int)
                } else {
                    b.int(42)
                },
                b.app(b.var(target.name, fun), &args, int),
            )
        };
        let mut group = vec![RecBinder {
            binder: f,
            params: a.alloc_slice_copy(&[x, n]),
            static_params: &[],
            body: step(if mutual { g } else { f }, x, n, mutual),
        }];
        if mutual {
            group.push(RecBinder {
                binder: g,
                params: a.alloc_slice_copy(&[m, y]),
                static_params: &[],
                body: step(f, y, m, false),
            });
        }
        cases.push((
            name,
            b.let_rec(
                &group,
                b.app(b.var(f.name, fun), &[b.int(7), b.int(3)], int),
            ),
            failure,
        ));
    }
    // Removing every parameter must keep definitions cold and execute each call.
    for (name, mutual, cold) in [
        ("all_unused_self", false, false),
        ("all_unused_mutual", true, false),
        ("all_unused_cold", true, true),
    ] {
        let unary = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[int]), int)));
        let f = bind("f", unary);
        let g = bind("g", unary);
        let x = bind("x", int);
        let y = bind("y", int);
        let invoke = |target: Binder<'a>, arg| b.app(b.var(target.name, unary), &[arg], int);
        let body = b.if_(
            b.lit(Constant::bool(a, true)),
            trace("body", b.int(21)),
            invoke(if mutual { g } else { f }, b.var(x.name, int)),
        );
        let mut group = vec![RecBinder {
            binder: f,
            params: a.alloc_slice_copy(&[x]),
            static_params: &[],
            body,
        }];
        if mutual {
            group.push(RecBinder {
                binder: g,
                params: a.alloc_slice_copy(&[y]),
                static_params: &[],
                body: invoke(f, b.var(y.name, int)),
            });
        }
        let calls = b.builtin(
            F::AddInteger,
            &[
                invoke(f, trace("entry1", b.int(0))),
                invoke(if mutual { g } else { f }, trace("entry2", b.int(1))),
            ],
            int,
        );
        let calls = if cold {
            b.if_(b.lit(Constant::bool(a, true)), b.int(42), calls)
        } else {
            calls
        };
        cases.push((name, b.let_rec(&group, calls), false));
    }

    for name in [
        "entry_failure",
        "partial",
        "escaping",
        "oversaturated",
        "compound_dependency",
        "mixed_delayed",
    ] {
        let f = bind("f", fun);
        let g = bind("g", fun);
        let x = bind("x", int);
        let n = bind("n", int);
        let y = bind("y", int);
        let m = bind("m", int);
        let unary = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[int]), int)));
        let next = b.builtin(F::SubtractInteger, &[b.var(n.name, int), b.int(1)], int);
        let forwarded = if name == "compound_dependency" {
            b.builtin(F::AddInteger, &[b.var(x.name, int), b.int(1)], int)
        } else {
            b.var(x.name, int)
        };
        let body = b.if_(
            b.builtin(F::EqualsInteger, &[b.var(n.name, int), b.int(0)], bool_),
            b.int(42),
            b.app(b.var(f.name, fun), &[forwarded, next], int),
        );
        let mut group = vec![RecBinder {
            binder: f,
            params: a.alloc_slice_copy(&[x, n]),
            static_params: &[],
            body,
        }];
        let invoke = b.app(b.var(f.name, fun), &[b.int(7), b.int(2)], int);
        let entry = match name {
            "entry_failure" => b.app(
                b.var(f.name, fun),
                &[trace("first", b.error(int)), trace("later", b.int(2))],
                int,
            ),
            "partial" => b.app(
                b.app(b.var(f.name, fun), &[b.int(7)], unary),
                &[b.int(2)],
                int,
            ),
            "escaping" => {
                let alias = bind("alias", fun);
                b.let_(
                    alias,
                    b.var(f.name, fun),
                    b.app(b.var(alias.name, fun), &[b.int(7), b.int(2)], int),
                )
            }
            "oversaturated" => {
                let z = bind("z", int);
                let ty = Ty::Term(a.alloc(TermTy::Fun(a.alloc_slice_copy(&[int, int]), unary)));
                group[0].binder.ty = ty;
                group[0].body = b.lam(&[z], b.var(z.name, int));
                b.app(b.var(f.name, ty), &[b.int(0), b.int(0), b.int(42)], int)
            }
            "mixed_delayed" => {
                group.push(RecBinder {
                    binder: g,
                    params: a.alloc_slice_copy(&[y, m]),
                    static_params: &[],
                    body: trace(
                        "delayed",
                        b.app(b.var(f.name, fun), &[b.int(7), b.int(2)], int),
                    ),
                });
                b.builtin(
                    F::AddInteger,
                    &[
                        invoke,
                        b.app(b.var(g.name, fun), &[b.int(0), b.int(0)], int),
                    ],
                    int,
                )
            }
            _ => invoke,
        };
        cases.push((name, b.let_rec(&group, entry), name == "entry_failure"));
    }
    cases
}
