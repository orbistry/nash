//! Shorten helper signatures before ANF, with exact direct calls only.
use crate::{
    analysis, anf,
    build::Builder,
    core::{Binder, Core, CoreKind, RecBinder},
    ty::{RuntimeTy, TermTy, Ty},
};
use std::collections::{HashMap, HashSet};

fn signature<'a>(b: &Builder<'a>, ty: Ty<'a>, keep: &[bool]) -> Option<Ty<'a>> {
    let Ty::Term(TermTy::Fun(params, result)) = ty else {
        return None;
    };
    if params.len() != keep.len() {
        return None;
    }
    let params: Vec<_> = params
        .iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(*p))
        .collect();
    Some(if params.is_empty() {
        Ty::Runtime(b.arena.alloc(RuntimeTy::Delay(*result)))
    } else {
        Ty::Term(
            b.arena
                .alloc(TermTy::Fun(b.arena.alloc_slice_copy(&params), *result)),
        )
    })
}

/// Requires typed Core and globally unique, well-scoped binders. Every use must
/// be a direct application with exactly the original arity. Partial, escaping,
/// oversaturated or unsupported signature views leave the helper unchanged.
/// Every non-atomic argument gets a strict binding in source order before the
/// shortened call, including discarded arguments. Exact calls to a known lambda
/// cannot run its body during partial application. Atoms are safe to discard.
/// Bindings stay call-local. Recursive parameter liveness follows bare-variable
/// forwarding dependencies to a fixed point; compound arguments stay strict.
/// All-unused helpers become delays, forced separately at each call.
/// Use the enclosing program Builder when optimizing a subtree for reinsertion.
pub fn reduce<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let report = analysis::occurrences(core);
    let mut occupied: HashSet<_> = report
        .bindings
        .iter()
        .map(|x| x.name.unique)
        .chain(report.uses.iter().map(|x| x.name.unique))
        .collect();
    core.map(b, &mut |node| {
        if let CoreKind::LetRec { binders, body } = node.kind {
            return recursive(b, node, binders, body, &mut occupied);
        }
        let CoreKind::Let {
            binder,
            value,
            body,
        } = node.kind
        else {
            return None;
        };
        let CoreKind::Lam {
            params,
            body: lambda_body,
        } = value.kind
        else {
            return None;
        };
        let mut used = HashSet::new();
        lambda_body.walk(&mut |node| {
            if let CoreKind::Var(name) = node.kind {
                used.insert(name.unique);
            }
        });
        let keep: Vec<_> = params
            .iter()
            .map(|p| used.contains(&p.name.unique))
            .collect();
        if keep.iter().all(|keep| *keep) {
            return None;
        }
        let (mut uses, mut calls) = (0usize, 0usize);
        body.walk(&mut |node| match node.kind {
            CoreKind::Var(name) if name.unique == binder.name.unique => uses += 1,
            CoreKind::App { func, args }
                if matches!(func.kind, CoreKind::Var(name) if name.unique == binder.name.unique)
                    && args.len() == params.len()
                    && matches!(func.ty, Ty::Term(TermTy::Fun(types, _)) if types.len() == keep.len()) =>
            {
                calls += 1
            }
            _ => {}
        });
        if calls == 0 || calls != uses {
            return None;
        }
        let binder_ty = signature(b, binder.ty, &keep)?;
        let value_ty = signature(b, value.ty, &keep)?;
        let params: Vec<_> = params
            .iter()
            .zip(&keep)
            .filter_map(|(p, keep)| keep.then_some(*p))
            .collect();
        let value = b.with_type(
            if params.is_empty() {
                b.delay(lambda_body)
            } else {
                b.lam(&params, lambda_body)
            },
            value_ty,
        );
        let body = body.map(b, &mut |call| {
            let CoreKind::App { func, args } = call.kind else {
                return None;
            };
            let CoreKind::Var(name) = func.kind else {
                return None;
            };
            if name.unique != binder.name.unique {
                return None;
            }
            Some(shorten_call(b, call, func, args, &keep, &mut occupied))
        });
        Some(b.with_type(
            b.let_(
                Binder {
                    ty: binder_ty,
                    ..binder
                },
                value,
                body,
            ),
            node.ty,
        ))
    })
}

fn shorten_call<'a>(
    b: &Builder<'a>,
    call: &'a Core<'a>,
    func: &'a Core<'a>,
    args: &[&'a Core<'a>],
    keep: &[bool],
    occupied: &mut HashSet<u32>,
) -> &'a Core<'a> {
    let CoreKind::Var(name) = func.kind else {
        unreachable!("prechecked direct call")
    };
    let func = b.var(
        name,
        signature(b, func.ty, keep).expect("prechecked signature"),
    );
    let mut prefix = Vec::new();
    let mut retained = Vec::new();
    for (arg, keep) in args.iter().zip(keep) {
        let argument = if anf::is_atom(arg) {
            *arg
        } else {
            let name = loop {
                let name = b.fresh("arg");
                if occupied.insert(name.unique) {
                    break name;
                }
            };
            let binder = Binder { name, ty: arg.ty };
            prefix.push((binder, *arg));
            b.var(name, arg.ty)
        };
        if *keep {
            retained.push(argument);
        }
    }
    let mut result = if retained.is_empty() {
        b.force(func, call.ty)
    } else {
        b.app(func, &retained, call.ty)
    };
    for (binder, value) in prefix.into_iter().rev() {
        result = b.let_(binder, value, result);
    }
    result
}

fn recursive<'a>(
    b: &Builder<'a>,
    node: &'a Core<'a>,
    binders: &'a [RecBinder<'a>],
    body: &'a Core<'a>,
    occupied: &mut HashSet<u32>,
) -> Option<&'a Core<'a>> {
    // Existing delayed workers and unsupported views are left alone. This also
    // makes a group containing a newly delayed worker stable on a second pass.
    if binders.is_empty()
        || binders.iter().any(|r| {
            r.params.is_empty()
                || !matches!(r.binder.ty, Ty::Term(TermTy::Fun(ps,_)) if ps.len() == r.params.len())
        })
    {
        return None;
    }
    let members: HashMap<_, _> = binders
        .iter()
        .enumerate()
        .map(|(i, r)| (r.binder.name.unique, i))
        .collect();
    let mut offsets = Vec::new();
    let mut slots = HashMap::new();
    for r in binders {
        offsets.push(slots.len());
        for p in r.params {
            slots.insert(p.name.unique, slots.len());
        }
    }
    let mut live = vec![false; slots.len()];
    let mut dependencies = vec![Vec::new(); slots.len()];
    let mut uses = vec![0; binders.len()];
    let mut calls = vec![0; binders.len()];
    // A bare variable argument is the only evaluation we can remove outright.
    // Every variable inside other arguments remains live because their strict
    // evaluation is retained, even when the receiving parameter is removed.
    let mut pending: Vec<_> = binders.iter().map(|r| r.body).chain([body]).collect();
    while let Some(term) = pending.pop() {
        match term.kind {
            CoreKind::Var(name) => {
                if let Some(&i) = members.get(&name.unique) {
                    uses[i] += 1;
                }
                if let Some(&i) = slots.get(&name.unique) {
                    live[i] = true;
                }
            }
            CoreKind::App { func, args } if matches!(func.kind,CoreKind::Var(name) if members.contains_key(&name.unique)) =>
            {
                let CoreKind::Var(name) = func.kind else {
                    unreachable!()
                };
                let i = members[&name.unique];
                uses[i] += 1;
                if args.len() != binders[i].params.len()
                    || !matches!(func.ty,Ty::Term(TermTy::Fun(ps,_)) if ps.len()==args.len())
                {
                    return None;
                }
                calls[i] += 1;
                for (slot, arg) in args.iter().enumerate() {
                    if let CoreKind::Var(name) = arg.kind
                        && let Some(&source) = slots.get(&name.unique)
                    {
                        dependencies[offsets[i] + slot].push(source);
                        continue;
                    }
                    pending.push(arg);
                }
            }
            _ => term.push_children_reversed(&mut pending),
        }
    }
    if uses != calls || calls.iter().all(|n| *n == 0) {
        return None;
    }
    let mut pending: Vec<_> = live
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.then_some(i))
        .collect();
    while let Some(target) = pending.pop() {
        for &source in &dependencies[target] {
            if !live[source] {
                live[source] = true;
                pending.push(source);
            }
        }
    }
    if live.iter().all(|v| *v) {
        return None;
    }
    let masks: Vec<_> = binders
        .iter()
        .enumerate()
        .map(|(i, r)| &live[offsets[i]..offsets[i] + r.params.len()])
        .collect();
    let mut rewrite = |core: &'a Core<'a>| {
        core.map(b, &mut |call| {
            let CoreKind::App { func, args } = call.kind else {
                return None;
            };
            let CoreKind::Var(name) = func.kind else {
                return None;
            };
            let &i = members.get(&name.unique)?;
            let keep = masks[i];
            if keep.iter().all(|v| *v) {
                return None;
            }
            Some(shorten_call(b, call, func, args, keep, occupied))
        })
    };
    let mut group = Vec::new();
    for (i, r) in binders.iter().enumerate() {
        let keep = masks[i];
        let params: Vec<_> = r
            .params
            .iter()
            .zip(keep)
            .filter_map(|(p, k)| k.then_some(*p))
            .collect();
        let statics: Vec<_> = r
            .static_params
            .iter()
            .filter_map(|&old| {
                let old = usize::from(old);
                keep.get(old)
                    .copied()
                    .unwrap_or(false)
                    .then(|| u16::try_from(keep[..old].iter().filter(|v| **v).count()).ok())
                    .flatten()
            })
            .collect();
        let body = rewrite(r.body);
        group.push(RecBinder {
            binder: Binder {
                ty: signature(b, r.binder.ty, keep)?,
                ..r.binder
            },
            params: b.arena.alloc_slice_copy(&params),
            static_params: b.arena.alloc_slice_copy(&statics),
            body: if params.is_empty() {
                b.delay(body)
            } else {
                body
            },
        });
    }
    let body = rewrite(body);
    Some(b.with_type(b.let_rec(&group, body), node.ty))
}
