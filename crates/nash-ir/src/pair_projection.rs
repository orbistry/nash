//! Single-field native pair projection only when constructor inverse cleanup cancels it.
use crate::{
    build::Builder,
    core::{CaseKind, Core, CoreKind, Test},
    ty::{ConstTy, Ty},
};
use nash_plutus::builtin::DefaultFunction as F;
use std::collections::{HashMap, HashSet};

fn resolve<'a>(mut node: &'a Core<'a>, bindings: &HashMap<u32, &'a Core<'a>>) -> &'a Core<'a> {
    while let CoreKind::Var(name) = node.kind {
        let Some(value) = bindings.get(&name.unique) else {
            break;
        };
        node = value;
    }
    node
}

/// Requires globally unique, scoped names. Retain both strict producer bindings.
pub fn reduce<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let mut bindings = HashMap::new();
    let mut used = HashSet::new();
    core.walk(&mut |node| match node.kind {
        CoreKind::Let { binder, value, .. } => {
            bindings.insert(binder.name.unique, value);
        }
        CoreKind::Var(name) => {
            used.insert(name.unique);
        }
        _ => {}
    });
    core.map(b, &mut |node| {
        let CoreKind::Case {
            kind: CaseKind::Pair,
            scrutinee,
            branches: [branch],
            default: None,
        } = node.kind
        else {
            return None;
        };
        let [left, right] = branch.binders else {
            return None;
        };
        if branch.test != Test::Pair || left.name.unique == right.name.unique {
            return None;
        }
        let Ty::Const(ConstTy::Pair(a, c)) = scrutinee.ty else {
            return None;
        };
        if left.ty != *a || right.ty != *c {
            return None;
        }
        let selected = match (
            used.contains(&left.name.unique),
            used.contains(&right.name.unique),
        ) {
            (true, false) => (left, F::FstPair),
            (false, true) => (right, F::SndPair),
            _ => return None,
        };
        // Match inverse::constructor's exact proof. Trace wrappers are not
        // skipped here, and neither decoder nor constructor is discarded.
        if !matches!(scrutinee.kind, CoreKind::Var(_)) {
            return None;
        }
        let CoreKind::Builtin {
            func: F::UnConstrData,
            args: [data],
        } = resolve(scrutinee, &bindings).kind
        else {
            return None;
        };
        if !matches!(data.kind, CoreKind::Var(_)) {
            return None;
        }
        let CoreKind::Builtin {
            func: F::ConstrData,
            args: [tag, fields],
        } = resolve(data, &bindings).kind
        else {
            return None;
        };
        let field = if selected.1 == F::FstPair {
            tag
        } else {
            fields
        };
        if !matches!(field.kind, CoreKind::Var(_)) {
            return None;
        }
        let projection = b.builtin(selected.1, &[scrutinee], selected.0.ty);
        Some(b.with_type(b.let_(*selected.0, projection, branch.body), node.ty))
    })
}
