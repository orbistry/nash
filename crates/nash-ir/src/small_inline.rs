//! Rule 4: duplicate only identity and single-builtin wrappers at full direct calls.
use crate::{
    analysis,
    build::Builder,
    core::{Binder, Core, CoreKind},
    dead_code, force_delay, known_case, propagate, single_use,
};
use std::{collections::HashSet, ptr};

/// Compose rules 1–4, dead-code cleanup, force/delay cancellation and known Boolean/literal folding.
/// Input is typed ANF with globally unique binders.
/// Conditional bodies, partial calls and indirect calls are not selected by rule 4.
pub fn simplify<'a>(b: &Builder<'a>, mut core: &'a Core<'a>) -> &'a Core<'a> {
    loop {
        // Beta cleanup flattens lets exposed by cancellation before other ANF rules.
        let core_without_delays = force_delay::reduce(b, crate::inverse::reduce(b, core));
        let selected = known_case::reduce_bool(b, core_without_delays);
        let selected = known_case::reduce_literals(b, selected);
        let next = dead_code::simplify_bindings(b, inline(b, single_use::simplify(b, selected)));
        let next = dead_code::prune_recursive(b, next);
        if ptr::eq(core, next) {
            return next;
        }
        core = next;
    }
}

fn eligible(params: &[Binder<'_>], body: &Core<'_>) -> bool {
    match body.kind {
        CoreKind::Var(name) => params.len() == 1 && params[0].name.unique == name.unique,
        CoreKind::Builtin { func, args } => {
            args.len() == func.arity()
                && args.iter().all(|arg| match arg.kind {
                    CoreKind::Var(_) => true,
                    CoreKind::Lit(value) => propagate::can_duplicate(value),
                    _ => false,
                })
        }
        _ => false,
    }
}

/// Replace selected direct applications with fresh lambdas. The existing beta
/// pass performs strict argument binding and ANF-preserving reduction afterwards.
/// No function-name whitelist or body-size threshold is used. When optimizing a
/// subtree for reinsertion, use the enclosing program's shared Builder; IDs
/// outside the supplied root cannot be discovered here.
pub fn inline<'a>(b: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    let report = analysis::occurrences(core);
    let mut used: HashSet<_> = report
        .bindings
        .iter()
        .map(|v| v.name.unique)
        .chain(report.uses.iter().map(|v| v.name.unique))
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
        if !eligible(params, lambda_body) {
            return None;
        }
        let mut changed = false;
        let body = body.map(b, &mut |call| {
            let CoreKind::App { func, args } = call.kind else {
                return None;
            };
            if !matches!(func.kind, CoreKind::Var(name) if name.unique == binder.name.unique)
                || args.len() != params.len()
            {
                return None;
            }
            // Eligible bodies contain no binders. Rename parameters only, while
            // avoiding all IDs in the whole input even with a fresh Builder.
            let fresh: Vec<_> = params
                .iter()
                .map(|param| {
                    let name = loop {
                        let name = b.fresh(param.name.text);
                        if used.insert(name.unique) {
                            break name;
                        }
                    };
                    Binder { name, ty: param.ty }
                })
                .collect();
            let copied = lambda_body.map(b, &mut |term| {
                let CoreKind::Var(name) = term.kind else {
                    return None;
                };
                let index = params
                    .iter()
                    .position(|param| param.name.unique == name.unique)?;
                Some(b.var(fresh[index].name, term.ty))
            });
            changed = true;
            Some(b.app(b.with_type(b.lam(&fresh, copied), func.ty), args, call.ty))
        });
        if !changed {
            return None;
        }
        let body = if analysis::free_variables(body)
            .iter()
            .any(|name| name.unique == binder.name.unique)
        {
            b.let_(binder, value, body)
        } else {
            body
        };
        Some(b.with_type(body, node.ty))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hygiene,
        ty::{ConstTy, Ty},
    };
    use nash_plutus::arena::Arena;
    #[test]
    fn explicit_type_views_survive_copying() {
        let a = Arena::new();
        let b = Builder::new(&a);
        let int = Ty::Const(&ConstTy::Int);
        let bytes = Ty::Const(&ConstTy::Bytes);
        let x = Binder {
            name: b.fresh("x"),
            ty: int,
        };
        let value = b.lam(&[x], b.var(x.name, bytes));
        let f = Binder {
            name: b.fresh("identity"),
            ty: value.ty,
        };
        let root = b.with_type(
            b.let_(
                f,
                value,
                b.app(b.with_type(b.var(f.name, f.ty), int), &[b.int(42)], bytes),
            ),
            int,
        );
        let after = inline(&Builder::new(&a), root);
        hygiene::validate(after, &[]).unwrap();
        assert_eq!(after.ty, int);
        let CoreKind::App { func, .. } = after.kind else {
            panic!("direct lambda")
        };
        assert_eq!(func.ty, int);
        let CoreKind::Lam { params, body } = func.kind else {
            panic!("copied lambda")
        };
        assert_eq!(params[0].ty, int);
        assert_eq!(body.ty, bytes);
    }
}
