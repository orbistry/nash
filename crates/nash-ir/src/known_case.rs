//! Known-subject case folding: Boolean cleanup and pre-ANF native constructors.
use crate::{
    build::Builder,
    core::{Binder, Branch, CaseKind, Core, CoreKind, Test},
};
use nash_plutus::constant::Constant;
use std::collections::{HashMap, HashSet};

/// No evaluation or substitution: the literal subject is already a value.
/// Preserve malformed tables and missing matches for the lowerer/runtime.
/// Keep the enclosing result type view when selecting a branch or default.
pub fn reduce_bool<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    core.map(b, &mut |node| {
        let CoreKind::Case {
            kind: CaseKind::Bool,
            scrutinee,
            branches,
            default,
        } = node.kind
        else {
            return None;
        };
        let CoreKind::Lit(Constant::Boolean(value)) = scrutinee.kind else {
            return None;
        };
        let (mut yes, mut no) = (None, None);
        for branch in branches {
            if !branch.binders.is_empty() {
                return None;
            }
            let slot = match branch.test {
                Test::True => &mut yes,
                Test::False => &mut no,
                _ => return None,
            };
            if slot.is_some() {
                return None;
            }
            *slot = Some(branch.body);
        }
        let selected = (if *value { yes } else { no }).or(default)?;
        Some(b.with_type(selected, node.ty))
    })
}

/// Input binders must be globally unique and well scoped, as for beta reduction.
/// Keep every field strict, in source order, even when its binder is unused.
/// Leave invalid tables, absent tags and application arity mismatches untouched.
pub fn reduce_constr<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    core.map(b, &mut |node| {
        let CoreKind::Case {
            kind: CaseKind::Tag,
            scrutinee,
            branches,
            default: None,
        } = node.kind
        else {
            return None;
        };
        let CoreKind::Constr { tag, fields } = scrutinee.kind else {
            return None;
        };
        let selected = select_constr(branches, tag, fields.len())?;
        let mut body = selected.body;
        for (&binder, &field) in selected.binders.iter().zip(fields).rev() {
            body = b.let_(binder, field, body);
        }
        Some(b.with_type(body, node.ty))
    })
}

/// Trial: fold matches on let-bound constructors without moving field evaluation.
/// Requires ANF with globally unique, well-scoped binders and the same name supply.
/// Keep constructor bindings for escaping uses; ordinary cleanup removes dead ones.
/// Run binding-splice cleanup afterwards: a selected branch can contain lets.
/// Newly exposed aliases can enable another iteration with that cleanup.
pub fn reduce_bound_constr<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let facts = constructor_bindings(core);
    let mut matched = HashSet::new();
    core.walk(&mut |node| {
        if let Some((root, _, _)) = bound_match(node, &facts) {
            matched.insert(root);
        }
    });
    if matched.is_empty() {
        return core;
    }
    // Give each field a stable reference at the original construction site.
    // In particular, copying an ANF lambda/delay would duplicate its binders.
    // Naming literals also leaves the existing propagation size policy in charge.
    let named = core.map(b, &mut |node| {
        let CoreKind::Let {
            binder,
            value,
            body,
        } = node.kind
        else {
            return None;
        };
        if !matched.contains(&binder.name.unique) {
            return None;
        }
        let CoreKind::Constr { tag, fields } = value.kind else {
            return None;
        };
        let mut bindings = Vec::new();
        let refs: Vec<_> = fields
            .iter()
            .map(|&field| {
                if matches!(field.kind, CoreKind::Var(_)) {
                    return field;
                }
                let binder = Binder {
                    name: b.fresh("field"),
                    ty: field.ty,
                };
                bindings.push((binder, field));
                b.var(binder.name, binder.ty)
            })
            .collect();
        if bindings.is_empty() {
            return None;
        }
        let mut result = b.let_(binder, b.constr(tag, &refs, value.ty), body);
        for (binder, field) in bindings.into_iter().rev() {
            result = b.let_(binder, field, result);
        }
        Some(b.with_type(result, node.ty))
    });
    let facts = constructor_bindings(named);
    named.map(b, &mut |node| {
        let (_, fields, selected) = bound_match(node, &facts)?;
        let mut body = selected.body;
        for (&binder, &field) in selected.binders.iter().zip(fields).rev() {
            body = b.let_(binder, field, body);
        }
        Some(b.with_type(body, node.ty))
    })
}

type ConstructorBindings<'a> = HashMap<u32, (u32, &'a Core<'a>)>;

fn constructor_bindings<'a>(core: &'a Core<'a>) -> ConstructorBindings<'a> {
    let mut facts = HashMap::new();
    // Preorder sees a lexical binding before its uses. Unique names let this
    // table include captured values without a separate environment per scope.
    core.walk(&mut |node| {
        let CoreKind::Let { binder, value, .. } = node.kind else {
            return;
        };
        let fact = match value.kind {
            CoreKind::Constr { .. } => Some((binder.name.unique, value)),
            CoreKind::Var(name) => facts.get(&name.unique).copied(),
            _ => None,
        };
        if let Some(fact) = fact {
            facts.insert(binder.name.unique, fact);
        }
    });
    facts
}

fn bound_match<'a>(
    node: &'a Core<'a>,
    facts: &ConstructorBindings<'a>,
) -> Option<(u32, &'a [&'a Core<'a>], &'a Branch<'a>)> {
    let CoreKind::Case {
        kind: CaseKind::Tag,
        scrutinee,
        branches,
        default: None,
    } = node.kind
    else {
        return None;
    };
    let CoreKind::Var(name) = scrutinee.kind else {
        return None;
    };
    let &(root, value) = facts.get(&name.unique)?;
    let CoreKind::Constr { tag, fields } = value.kind else {
        return None;
    };
    Some((root, fields, select_constr(branches, tag, fields.len())?))
}

fn select_constr<'a>(branches: &'a [Branch<'a>], tag: u16, arity: usize) -> Option<&'a Branch<'a>> {
    let mut seen = vec![false; branches.len()];
    for branch in branches {
        let Test::Tag(tag) = branch.test else {
            return None;
        };
        let slot = seen.get_mut(usize::from(tag))?;
        if *slot {
            return None;
        }
        *slot = true;
    }
    let selected = branches
        .iter()
        .find(|branch| branch.test == Test::Tag(tag))?;
    (selected.binders.len() == arity).then_some(selected)
}
