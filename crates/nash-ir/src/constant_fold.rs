//! Constant-builtin rewriting; evaluation policy belongs to the caller.
use crate::{
    build::Builder,
    core::{Core, CoreKind},
};
use nash_plutus::{builtin::DefaultFunction, constant::Constant};
use std::collections::HashMap;

/// Input names must be globally unique and well scoped. Only literals or aliases
/// of literal bindings are passed to the evaluator. Returning None keeps the call.
/// Strict bindings remain for ordinary cleanup; no computation is copied/moved.
pub fn reduce<'a>(
    b: &Builder<'a>,
    core: &'a Core<'a>,
    evaluate: &mut impl FnMut(DefaultFunction, &[&'a Constant<'a>]) -> Option<&'a Constant<'a>>,
) -> &'a Core<'a> {
    let mut constants = HashMap::new();
    core.walk(&mut |node| {
        if let CoreKind::Let { binder, value, .. } = node.kind {
            let constant = match value.kind {
                CoreKind::Lit(c) => Some(c),
                CoreKind::Var(n) => constants.get(&n.unique).copied(),
                _ => None,
            };
            if let Some(c) = constant {
                constants.insert(binder.name.unique, c);
            }
        }
    });
    core.map(b, &mut |node| {
        let CoreKind::Builtin { func, args } = node.kind else {
            return None;
        };
        if args.len() != func.arity() {
            return None;
        }
        let args: Option<Vec<_>> = args
            .iter()
            .map(|arg| match arg.kind {
                CoreKind::Lit(c) => Some(c),
                CoreKind::Var(n) => constants.get(&n.unique).copied(),
                _ => None,
            })
            .collect();
        let result = evaluate(func, &args?)?;
        Some(b.with_type(b.lit(result), node.ty))
    })
}
