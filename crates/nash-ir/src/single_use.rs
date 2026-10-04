//! Single-use value substitution and immediate computed returns in typed ANF.
use crate::{
    anf, beta,
    build::Builder,
    core::{Core, CoreKind},
};
use std::{collections::HashMap, ptr};

/// Compose the accepted rules 1, 2 and 3 until nothing changes.
/// Substitution moves values once; it never duplicates lambda or delay bodies.
pub fn simplify<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let next = inline(b, beta::simplify(b, core));
        if ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

/// Input must be typed ANF with globally unique, well-scoped binders.
/// Values can move to their sole use, including into a suspended scope.
/// Forced builtin references stay bound for top-level sharing, even at one use.
/// Computations can replace an immediate return or fuse adjacent application
/// stages. Other computed function operands stay bound to preserve ANF.
pub fn inline<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let mut uses: HashMap<u32, usize> = HashMap::new();
    core.walk(&mut |node| {
        if let CoreKind::Var(name) = node.kind {
            *uses.entry(name.unique).or_default() += 1;
        }
    });
    core.map(b, &mut |node| {
        let CoreKind::Let {
            binder,
            value,
            body,
        } = node.kind
        else {
            return None;
        };
        // A forced builtin reference must remain shared, even when returned
        // directly. The builtin-sharing pass owns its top-level placement.
        if matches!(value.kind, CoreKind::Builtin { func, args: [] } if func.force_count() > 0) {
            return None;
        }
        if matches!(body.kind,CoreKind::Var(name) if name.unique==binder.name.unique) {
            return Some(b.with_type(value, node.ty));
        }
        if uses.get(&binder.name.unique) == Some(&1)
            && let CoreKind::App { func, args } = value.kind
            && let CoreKind::App {
                func: next,
                args: later,
            } = body.kind
            && matches!(next.kind, CoreKind::Var(name) if name.unique == binder.name.unique)
            && value.ty == binder.ty
            && next.ty == binder.ty
            && body.ty == node.ty
            && !args.is_empty()
            && !later.is_empty()
            && anf::is_atom(func)
            && args.iter().chain(later).all(|arg| anf::is_atom(arg))
        {
            // App lowers left-associatively: concatenation keeps the earlier
            // application stage before the later one. The sole occurrence also
            // excludes references to this binder inside any later argument.
            let args = args.iter().chain(later).copied().collect::<Vec<_>>();
            return Some(b.app(func, &args, node.ty));
        }
        if uses.get(&binder.name.unique) != Some(&1) || !anf::is_atom(value) {
            return None;
        }
        // Use mapped children: original RHS pointers could resurrect already removed
        // bindings. Unique IDs prevent capture; the sole use prevents duplication.
        let body = body.map(b, &mut |occurrence| match occurrence.kind {
            CoreKind::Var(name) if name.unique == binder.name.unique => {
                Some(b.with_type(value, occurrence.ty))
            }
            _ => None,
        });
        Some(b.with_type(body, node.ty))
    })
}
#[cfg(test)]
mod tests;
