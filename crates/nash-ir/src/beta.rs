//! Direct lambda application over hygienic, typed ANF. No body duplication.
use crate::{
    anf,
    build::Builder,
    core::{Binder, Core, CoreKind},
    propagate,
};
use std::{collections::HashSet, ptr};

/// Apply rules 1 and 2 until neither rewrites anything. Both passes preserve
/// unchanged pointers, so pointer identity is an actual change flag, not a
/// node-count heuristic. Beta consumes lambda parameters; propagation introduces none.
pub fn simplify<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let next = reduce(b, propagate::propagate(b, core));
        if ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

/// Reduce direct applications, retaining strict argument bindings. Input must
/// be ANF with globally unique, well-scoped binders. Bound functions are not
/// inlined. Newly exposed applications are handled on the next iteration.
pub fn reduce<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let report = crate::analysis::occurrences(core);
    let mut used: HashSet<_> = report
        .bindings
        .iter()
        .map(|v| v.name.unique)
        .chain(report.uses.iter().map(|v| v.name.unique))
        .collect();
    core.map(b, &mut |node| match node.kind {
        CoreKind::App { func, args } => {
            let CoreKind::Lam { params, body } = func.kind else {
                return None;
            };
            let count = params.len().min(args.len());
            let mut result = if count < params.len() {
                b.lam(&params[count..], body)
            } else if count == args.len() {
                body
            } else {
                splice(b, body, &mut |value| {
                    if anf::is_atom(value) {
                        b.app(value, &args[count..], node.ty)
                    } else {
                        let name = loop {
                            let name = b.fresh("beta");
                            if used.insert(name.unique) {
                                break name;
                            }
                        };
                        b.let_(
                            Binder { name, ty: value.ty },
                            value,
                            b.app(b.var(name, value.ty), &args[count..], node.ty),
                        )
                    }
                })
            };
            for (param, arg) in params[..count].iter().zip(&args[..count]).rev() {
                result = b.let_(*param, arg, result);
            }
            Some(b.with_type(result, node.ty))
        }
        CoreKind::Let {
            binder,
            value,
            body,
        } if matches!(value.kind, CoreKind::Let { .. } | CoreKind::LetRec { .. }) => {
            Some(b.with_type(
                splice(b, value, &mut |value| b.let_(binder, value, body)),
                node.ty,
            ))
        }
        _ => None,
    })
}

/// Move only the leading binding sequence into its enclosing strict context.
/// Keep the expression's type view on its final value when peeling bindings.
fn splice<'a>(
    b: &Builder<'a>,
    value: &'a Core<'a>,
    finish: &mut impl FnMut(&'a Core<'a>) -> &'a Core<'a>,
) -> &'a Core<'a> {
    match value.kind {
        CoreKind::Let {
            binder,
            value: rhs,
            body,
        } => b.let_(binder, rhs, splice(b, b.with_type(body, value.ty), finish)),
        CoreKind::LetRec { binders, body } => {
            b.let_rec(binders, splice(b, b.with_type(body, value.ty), finish))
        }
        _ => finish(value),
    }
}

#[cfg(test)]
mod tests;
