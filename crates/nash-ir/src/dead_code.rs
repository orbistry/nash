//! Remove safely discardable unused bindings and unreachable recursive members.
use crate::{
    analysis,
    build::Builder,
    core::{Core, CoreKind},
};
use std::{
    collections::{HashMap, HashSet},
    ptr,
};

/// Requires globally unique, well-scoped binders. Iterate so discarding an unused
/// closure or alias can expose dead bindings that supplied its captured values.
/// Calls, recursive groups and parameters are not eliminated by this pass.
pub fn simplify_bindings<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        let mut used = HashSet::new();
        core.walk(&mut |node| {
            if let CoreKind::Var(name) = node.kind {
                used.insert(name.unique);
            }
        });
        let next = core.map(b, &mut |node| {
            if let CoreKind::Let {
                binder,
                value,
                body,
            } = node.kind
                && !used.contains(&binder.name.unique)
                && analysis::safe_to_discard(value)
            {
                return Some(b.with_type(body, node.ty));
            }
            None
        });
        if ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

fn mark(
    core: &Core<'_>,
    indices: &HashMap<u32, usize>,
    live: &mut [bool],
    pending: &mut Vec<usize>,
) {
    core.walk(&mut |node| {
        if let CoreKind::Var(name) = node.kind
            && let Some(&index) = indices.get(&name.unique)
            && !live[index]
        {
            live[index] = true;
            pending.push(index);
        }
    });
}

/// Input must have globally unique, well-scoped binders. Every reference counts,
/// including partial applications, escaping values and suspended captures.
/// Function bodies are deferred; delayed workers are too.
/// No call-shape analysis, static-parameter inference or parameter removal occurs.
pub fn prune_recursive<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    core.map(b, &mut |node| {
        let CoreKind::LetRec { binders, body } = node.kind else {
            return None;
        };
        // Other zero-parameter groups are unsupported recursive values, not
        // suspended function definitions. Preserve their lowering error.
        if binders
            .iter()
            .any(|r| r.params.is_empty() && !matches!(r.body.kind, CoreKind::Delay(_)))
        {
            return None;
        }
        let indices = binders
            .iter()
            .enumerate()
            .map(|(i, r)| (r.binder.name.unique, i))
            .collect();
        let mut live = vec![false; binders.len()];
        let mut pending = Vec::new();
        mark(body, &indices, &mut live, &mut pending);
        while let Some(index) = pending.pop() {
            mark(binders[index].body, &indices, &mut live, &mut pending);
        }
        let kept: Vec<_> = binders
            .iter()
            .zip(&live)
            .filter_map(|(r, live)| live.then_some(*r))
            .collect();
        if kept.len() == binders.len() && !kept.is_empty() {
            return None;
        }
        Some(b.with_type(
            if kept.is_empty() {
                body
            } else {
                b.let_rec(&kept, body)
            },
            node.ty,
        ))
    })
}
