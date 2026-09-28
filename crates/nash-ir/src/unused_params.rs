//! Shorten nonrecursive helper signatures before ANF, with exact direct calls only.
use crate::{
    analysis, anf,
    build::Builder,
    core::{Binder, Core, CoreKind},
    ty::{RuntimeTy, TermTy, Ty},
};
use std::collections::HashSet;

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
/// Bindings stay call-local; this pass does not remove them or change LetRec
/// parameters. All-unused helpers become delays, forced separately at each call.
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
            let func = b.var(
                name,
                signature(b, func.ty, &keep).expect("prechecked signature"),
            );
            let mut prefix = Vec::new();
            let mut retained = Vec::new();
            for (arg, keep) in args.iter().zip(&keep) {
                let argument = if anf::is_atom(arg) {
                    *arg
                } else {
                    let name = loop {
                        let name = b.fresh("arg");
                        if occupied.insert(name.unique) { break name; }
                    };
                    let binder = Binder { name, ty: arg.ty };
                    prefix.push((binder, *arg));
                    b.var(name, arg.ty)
                };
                if *keep { retained.push(argument); }
            }
            let mut result = if retained.is_empty() {
                b.force(func, call.ty)
            } else {
                b.app(func, &retained, call.ty)
            };
            for (binder, value) in prefix.into_iter().rev() {
                result = b.let_(binder, value, result);
            }
            Some(result)
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
