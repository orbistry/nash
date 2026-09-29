//! Shared, conservative Core analyses. These report facts; they enable no rewrite.
use crate::core::{Binder, Core, CoreKind, Name};
use std::collections::HashSet;

/// IDs are preorder node occurrences, not pointers: shared subtrees get distinct IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    Lambda(usize),
    Delay(usize),
    RecursiveBody {
        node: usize,
        index: usize,
    },
    /// `None` identifies the default, otherwise the index in the branch table.
    Branch {
        node: usize,
        index: Option<usize>,
    },
}

#[derive(Debug)]
pub struct Binding<'a> {
    pub name: Name<'a>,
    pub execution_scope: Vec<Boundary>,
}

#[derive(Debug)]
pub struct Occurrence<'a> {
    pub name: Name<'a>,
    /// Index in `Occurrences::bindings`, or `None` for a free variable.
    pub binding: Option<usize>,
    pub node: usize,
    pub scope: Vec<usize>,
    pub execution_scope: Vec<Boundary>,
}

#[derive(Debug, Default)]
pub struct Occurrences<'a> {
    pub bindings: Vec<Binding<'a>>,
    pub uses: Vec<Occurrence<'a>>,
}

/// Resolve each use to its lexical declaration using numeric name identity.
/// Execution boundaries describe where a use can run, not how often it runs.
/// This report alone is not a proof that moving a computation is safe.
pub fn occurrences<'a>(core: &Core<'a>) -> Occurrences<'a> {
    let mut visitor = Collector {
        report: Occurrences::default(),
        scope: Vec::new(),
        boundaries: Vec::new(),
        next_node: 0,
    };
    visitor.visit(core);
    visitor.report
}

struct Collector<'a> {
    report: Occurrences<'a>,
    scope: Vec<usize>,
    boundaries: Vec<Boundary>,
    next_node: usize,
}

impl<'a> Collector<'a> {
    fn bind(&mut self, binder: Binder<'a>) {
        let id = self.report.bindings.len();
        self.report.bindings.push(Binding {
            name: binder.name,
            execution_scope: self.boundaries.clone(),
        });
        self.scope.push(id);
    }

    fn visit(&mut self, core: &Core<'a>) {
        enum Task<'t, 'a> {
            Visit(&'t Core<'a>),
            Bind(Binder<'a>),
            Boundary(Boundary),
            Restore(usize, usize),
        }
        let mut pending = vec![Task::Visit(core)];
        let mut children = Vec::new();
        while let Some(task) = pending.pop() {
            let core = match task {
                Task::Visit(core) => core,
                Task::Bind(binder) => {
                    self.bind(binder);
                    continue;
                }
                Task::Boundary(boundary) => {
                    self.boundaries.push(boundary);
                    continue;
                }
                Task::Restore(scope, boundaries) => {
                    self.scope.truncate(scope);
                    self.boundaries.truncate(boundaries);
                    continue;
                }
            };
            let node = self.next_node;
            self.next_node += 1;
            let depth = self.scope.len();
            let boundaries = self.boundaries.len();
            pending.push(Task::Restore(depth, boundaries));
            match &core.kind {
                CoreKind::Var(name) => {
                    let binding = self
                        .scope
                        .iter()
                        .rev()
                        .copied()
                        .find(|&id| self.report.bindings[id].name.unique == name.unique);
                    self.report.uses.push(Occurrence {
                        name: *name,
                        binding,
                        node,
                        scope: self.scope.clone(),
                        execution_scope: self.boundaries.clone(),
                    });
                }
                CoreKind::Lam { params, body } => {
                    if !params.is_empty() {
                        self.boundaries.push(Boundary::Lambda(node));
                    }
                    for param in *params {
                        self.bind(*param);
                    }
                    pending.push(Task::Visit(body));
                }
                CoreKind::Let {
                    binder,
                    value,
                    body,
                } => {
                    pending.push(Task::Visit(body));
                    pending.push(Task::Bind(*binder));
                    pending.push(Task::Visit(value));
                }
                CoreKind::LetRec { binders, body } => {
                    for rb in *binders {
                        self.bind(rb.binder);
                    }
                    let group_depth = self.scope.len();
                    pending.push(Task::Visit(body));
                    for (index, rb) in binders.iter().enumerate().rev() {
                        pending.push(Task::Restore(group_depth, boundaries));
                        pending.push(Task::Visit(rb.body));
                        pending.extend(rb.params.iter().rev().map(|p| Task::Bind(*p)));
                        pending.push(Task::Boundary(Boundary::RecursiveBody { node, index }));
                    }
                }
                CoreKind::Case {
                    scrutinee,
                    branches,
                    default,
                    ..
                } => {
                    if let Some(body) = default {
                        pending.push(Task::Restore(depth, boundaries));
                        pending.push(Task::Visit(body));
                        pending.push(Task::Boundary(Boundary::Branch { node, index: None }));
                    }
                    for (index, branch) in branches.iter().enumerate().rev() {
                        pending.push(Task::Restore(depth, boundaries));
                        pending.push(Task::Visit(branch.body));
                        pending.extend(branch.binders.iter().rev().map(|p| Task::Bind(*p)));
                        pending.push(Task::Boundary(Boundary::Branch {
                            node,
                            index: Some(index),
                        }));
                    }
                    pending.push(Task::Visit(scrutinee));
                }
                CoreKind::Delay(body) => {
                    self.boundaries.push(Boundary::Delay(node));
                    pending.push(Task::Visit(body));
                }
                _ => {
                    children.clear();
                    core.push_children_reversed(&mut children);
                    pending.extend(children.iter().map(|child| Task::Visit(child)));
                }
            }
        }
    }
}

/// Whether evaluation is known to terminate without a trace or failure.
/// Requires well-formed, well-scoped Core: every free variable must denote a value
/// in the surrounding environment. Does not prove duplication or motion safe.
/// Saturated builtins and arbitrary calls are deliberately not evaluated here.
pub fn safe_to_discard(core: &Core<'_>) -> bool {
    let mut pending = vec![core];
    while let Some(core) = pending.pop() {
        match &core.kind {
            CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Delay(_) => {}
            CoreKind::Lam { params, body } => {
                if params.is_empty() {
                    pending.push(body);
                }
            }
            CoreKind::Builtin { func, args } if args.len() < func.arity() => {
                pending.extend(args.iter().rev().copied())
            }
            CoreKind::Constr { fields, .. } => pending.extend(fields.iter().rev().copied()),
            CoreKind::Let { value, body, .. } => {
                pending.push(body);
                pending.push(value);
            }
            _ => return false,
        }
    }
    true
}

/// Structural size only. Counts repeated subtree occurrences, includes LetRec,
/// and excludes types and literal payloads. A large literal counts like a small
/// one. This is insufficient to decide profitability; measure emitted Flat size.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CoreSize {
    pub nodes: usize,
    pub binders: usize,
}

pub fn size_estimate(core: &Core<'_>) -> CoreSize {
    let mut size = CoreSize::default();
    core.walk(&mut |node| {
        size.nodes += 1;
        size.binders += match &node.kind {
            CoreKind::Lam { params, .. } => params.len(),
            CoreKind::Let { .. } => 1,
            CoreKind::LetRec { binders, .. } => binders.iter().map(|b| 1 + b.params.len()).sum(),
            CoreKind::Case { branches, .. } => branches.iter().map(|b| b.binders.len()).sum(),
            _ => 0,
        };
    });
    size
}

/// Free runtime names in deterministic source traversal order. Name identity
/// follows the unique number, exactly as UPLC De Bruijn conversion does.
pub fn free_variables<'a>(core: &Core<'a>) -> Vec<Name<'a>> {
    enum Task<'t, 'a> {
        Visit(&'t Core<'a>),
        Bind(u32),
        Restore(usize),
    }
    let mut pending = vec![Task::Visit(core)];
    let mut scope = Vec::new();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let mut children = Vec::new();
    while let Some(task) = pending.pop() {
        let core = match task {
            Task::Visit(core) => core,
            Task::Bind(id) => {
                scope.push(id);
                continue;
            }
            Task::Restore(depth) => {
                scope.truncate(depth);
                continue;
            }
        };
        let depth = scope.len();
        pending.push(Task::Restore(depth));
        match &core.kind {
            CoreKind::Var(name) => {
                if !scope.contains(&name.unique) && seen.insert(name.unique) {
                    result.push(*name);
                }
            }
            CoreKind::Lam { params, body } => {
                scope.extend(params.iter().map(|p| p.name.unique));
                pending.push(Task::Visit(body));
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                pending.push(Task::Visit(body));
                pending.push(Task::Bind(binder.name.unique));
                pending.push(Task::Visit(value));
            }
            CoreKind::LetRec { binders, body } => {
                scope.extend(binders.iter().map(|b| b.binder.name.unique));
                let group_depth = scope.len();
                pending.push(Task::Visit(body));
                for rb in binders.iter().rev() {
                    pending.push(Task::Restore(group_depth));
                    pending.push(Task::Visit(rb.body));
                    pending.extend(rb.params.iter().rev().map(|p| Task::Bind(p.name.unique)));
                }
            }
            CoreKind::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                if let Some(body) = default {
                    pending.push(Task::Visit(body));
                }
                for branch in branches.iter().rev() {
                    pending.push(Task::Restore(depth));
                    pending.push(Task::Visit(branch.body));
                    pending.extend(
                        branch
                            .binders
                            .iter()
                            .rev()
                            .map(|p| Task::Bind(p.name.unique)),
                    );
                }
                pending.push(Task::Visit(scrutinee));
            }
            _ => {
                children.clear();
                core.push_children_reversed(&mut children);
                pending.extend(children.iter().map(|child| Task::Visit(child)));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
