//! Source-let reductions are deliberately restricted to explicitly registered
//! binder identities. Native bindings and strict pattern boundaries are not
//! candidates for these source-language evaluation rules.
use std::collections::HashSet;

use nash_ir::{build::Builder, core::*};
use nash_plutus::constant::Constant;

pub(super) fn optimize<'a>(
    build: &Builder<'a>,
    mut root: &'a Core<'a>,
    bindings: &HashSet<u32>,
    barriers: &HashSet<u32>,
) -> &'a Core<'a> {
    if bindings.is_empty() {
        return root;
    }
    loop {
        let mut changed = false;
        root = root.map(build, &mut |node| {
            if let Core::Case {
                kind: CaseKind::Bool,
                scrutinee,
                branches,
                default,
            } = node
                && contains_binding(scrutinee, bindings)
                && carries_result(scrutinee)
            {
                changed = true;
                return Some(carry_result(build, scrutinee, branches, *default));
            }
            let Core::Let {
                binder,
                value,
                body,
            } = node
            else {
                return None;
            };
            if !bindings.contains(&binder.name.unique) {
                return None;
            }
            let usage = lookup(body, binder.name, 0, 0, barriers);
            let inert = matches!(
                value,
                Core::Var(_)
                    | Core::Lit(_)
                    | Core::Evaluated { .. }
                    | Core::Lam { .. }
                    | Core::Delay(_)
                    | Core::Builtin { args: [], .. }
            );
            if usage.count == 1 && ((!usage.delayed && !usage.barrier) || inert) {
                changed = true;
                return Some(body.map(build, &mut |node| match node {
                    Core::Var(name) if *name == binder.name => Some(*value),
                    _ => None,
                }));
            }
            if usage.count == 0 && inert {
                changed = true;
                return Some(*body);
            }
            None
        });
        if !changed {
            return root;
        }
    }
}

/// Decision-tree projections belong to the source binding, but its compiled
/// continuation can contain native or explicitly strict bindings of its own.
pub(crate) fn mark_pattern_bindings(
    root: &Core<'_>,
    continuation: &Core<'_>,
    bindings: &mut HashSet<u32>,
) {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if std::ptr::eq(node, continuation) {
            continue;
        }
        match node {
            Core::Let { binder, .. } => {
                bindings.insert(binder.name.unique);
            }
            Core::Lam { params, .. } => bindings.extend(params.iter().map(|p| p.name.unique)),
            Core::Case { branches, .. } => {
                bindings.extend(
                    branches
                        .iter()
                        .flat_map(|b| b.binders.iter())
                        .map(|b| b.name.unique),
                );
            }
            _ => {}
        }
        children(node, &mut pending);
    }
}

fn contains_binding(root: &Core<'_>, bindings: &HashSet<u32>) -> bool {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if matches!(node, Core::Let { binder, .. } if bindings.contains(&binder.name.unique)) {
            return true;
        }
        children(node, &mut pending);
    }
    false
}

fn carries_result(node: &Core<'_>) -> bool {
    matches!(
        node,
        Core::Let { .. } | Core::LetRec { .. } | Core::Case { .. } | Core::Trace { .. }
    ) || matches!(
        node,
        Core::App {
            func: Core::Lam { .. },
            ..
        }
    )
}

/// Carry an enclosing Boolean consumer through evaluation-preserving control
/// flow. This exposes the final validator's False -> Error continuation before
/// the occurrence query decides whether a branch is genuinely delayed.
fn carry_result<'a>(
    build: &Builder<'a>,
    value: &'a Core<'a>,
    branches: &'a [Branch<'a>],
    default: Option<&'a Core<'a>>,
) -> &'a Core<'a> {
    match value {
        Core::Lit(Constant::Boolean(value)) => branches
            .iter()
            .find(|branch| {
                matches!(
                    (branch.test, *value),
                    (Test::True, true) | (Test::False, false)
                )
            })
            .map(|branch| branch.body)
            .or(default)
            .unwrap_or_else(|| build.error()),
        Core::Error => value,
        Core::Let {
            binder,
            value,
            body,
        } => build.let_(*binder, value, carry_result(build, body, branches, default)),
        Core::LetRec { binders, body } => {
            build.let_rec(binders, carry_result(build, body, branches, default))
        }
        Core::App {
            func: Core::Lam { params, body },
            args,
        } => build.app(
            build.lam(params, carry_result(build, body, branches, default)),
            args,
        ),
        Core::Case {
            kind,
            scrutinee,
            branches: inner,
            default: inner_default,
        } => {
            let inner = inner
                .iter()
                .map(|branch| Branch {
                    body: carry_result(build, branch.body, branches, default),
                    ..*branch
                })
                .collect::<Vec<_>>();
            build.case(
                *kind,
                scrutinee,
                &inner,
                inner_default.map(|body| carry_result(build, body, branches, default)),
            )
        }
        Core::Trace { message, body } => {
            build.trace(message, carry_result(build, body, branches, default))
        }
        _ => build.case(CaseKind::Bool, value, branches, default),
    }
}

#[derive(Default)]
struct Usage {
    count: u32,
    delayed: bool,
    barrier: bool,
}
impl Usage {
    fn add(&mut self, other: Self) {
        self.count += other.count;
        self.delayed |= other.delayed;
        self.barrier |= other.barrier;
    }
    fn delay(mut self) -> Self {
        self.delayed |= self.count != 0;
        self
    }
}

fn lookup(
    node: &Core<'_>,
    target: Name<'_>,
    applied: usize,
    forced: usize,
    barriers: &HashSet<u32>,
) -> Usage {
    let mut usage = Usage::default();
    match node {
        Core::Var(name) => usage.count = u32::from(*name == target),
        Core::Lam { params, body } => {
            if params.iter().any(|param| param.name == target) {
                return usage;
            }
            usage = lookup(
                body,
                target,
                applied.saturating_sub(params.len()),
                forced,
                barriers,
            );
            if applied < params.len() {
                usage = usage.delay();
            }
            usage.barrier |= usage.count != 0
                && params
                    .iter()
                    .any(|param| barriers.contains(&param.name.unique));
        }
        Core::App { func, args } => {
            usage = lookup(func, target, applied + args.len(), forced, barriers);
            for arg in *args {
                usage.add(lookup(arg, target, 0, 0, barriers));
            }
        }
        Core::Let {
            binder,
            value,
            body,
        } => {
            usage = lookup(value, target, 0, 0, barriers);
            if binder.name != target {
                usage.add(lookup(body, target, applied, forced, barriers));
            }
        }
        Core::LetRec { binders, body } => {
            for binder in *binders {
                let mut inner = lookup(binder.body, target, 0, 0, barriers).delay();
                inner.barrier |= inner.count != 0;
                usage.add(inner);
            }
            usage.add(lookup(body, target, applied, forced, barriers));
        }
        Core::Delay(body) => {
            usage = lookup(body, target, applied, forced.saturating_sub(1), barriers);
            if forced == 0 {
                usage = usage.delay();
            }
        }
        Core::Force(body) => usage = lookup(body, target, applied, forced + 1, barriers),
        Core::Case {
            kind,
            scrutinee,
            branches,
            default,
        } => {
            usage = lookup(scrutinee, target, 0, 0, barriers);
            let live = branches
                .iter()
                .filter(|branch| !matches!(branch.body, Core::Error))
                .count()
                + usize::from(default.is_some_and(|body| !matches!(body, Core::Error)));
            let refuting = matches!(kind, CaseKind::Bool | CaseKind::List) && live == 1;
            for branch in *branches {
                let inner = if refuting {
                    lookup(branch.body, target, applied, forced, barriers)
                } else {
                    lookup(branch.body, target, 0, 0, barriers).delay()
                };
                usage.add(inner);
            }
            if let Some(body) = default {
                usage.add(if refuting {
                    lookup(body, target, applied, forced, barriers)
                } else {
                    lookup(body, target, 0, 0, barriers).delay()
                });
            }
        }
        Core::Constr { fields, .. } | Core::Builtin { args: fields, .. } => {
            for field in *fields {
                usage.add(lookup(field, target, 0, 0, barriers));
            }
        }
        Core::Field { record, .. } | Core::Cast { arg: record, .. } => {
            usage = lookup(record, target, 0, 0, barriers);
        }
        Core::Trace { message, body } => {
            usage = lookup(message, target, 0, 0, barriers);
            usage.add(lookup(body, target, 0, 0, barriers).delay());
        }
        Core::Lit(_) | Core::Evaluated { .. } | Core::Error => {}
    }
    usage
}

fn children<'t, 'a>(node: &'t Core<'a>, pending: &mut Vec<&'t Core<'a>>) {
    match node {
        Core::Lam { body, .. } | Core::Delay(body) | Core::Force(body) => pending.push(body),
        Core::App { func, args } => {
            pending.push(func);
            pending.extend_from_slice(args);
        }
        Core::Let { value, body, .. } => pending.extend([*value, *body]),
        Core::LetRec { binders, body } => {
            pending.push(body);
            pending.extend(binders.iter().map(|binder| binder.body));
        }
        Core::Case {
            scrutinee,
            branches,
            default,
            ..
        } => {
            pending.push(scrutinee);
            pending.extend(branches.iter().map(|branch| branch.body));
            pending.extend(default.iter().copied());
        }
        Core::Constr { fields, .. } | Core::Builtin { args: fields, .. } => {
            pending.extend_from_slice(fields)
        }
        Core::Field { record, .. } | Core::Cast { arg: record, .. } => pending.push(record),
        Core::Trace { message, body } => pending.extend([*message, *body]),
        Core::Var(_) | Core::Lit(_) | Core::Evaluated { .. } | Core::Error => {}
    }
}
