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

    fn within(&mut self, boundary: Boundary, body: &Core<'a>) {
        self.boundaries.push(boundary);
        self.visit(body);
        self.boundaries.pop();
    }

    fn visit(&mut self, core: &Core<'a>) {
        let node = self.next_node;
        self.next_node += 1;
        let depth = self.scope.len();
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
                if params.is_empty() {
                    self.visit(body);
                } else {
                    self.boundaries.push(Boundary::Lambda(node));
                    for p in *params {
                        self.bind(*p);
                    }
                    self.visit(body);
                    self.boundaries.pop();
                }
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                self.visit(value);
                self.bind(*binder);
                self.visit(body);
            }
            CoreKind::LetRec { binders, body } => {
                for rb in *binders {
                    self.bind(rb.binder);
                }
                let group_depth = self.scope.len();
                for (index, rb) in binders.iter().enumerate() {
                    self.boundaries
                        .push(Boundary::RecursiveBody { node, index });
                    for p in rb.params {
                        self.bind(*p);
                    }
                    self.visit(rb.body);
                    self.boundaries.pop();
                    self.scope.truncate(group_depth);
                }
                self.visit(body);
            }
            CoreKind::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                self.visit(scrutinee);
                for (index, branch) in branches.iter().enumerate() {
                    self.boundaries.push(Boundary::Branch {
                        node,
                        index: Some(index),
                    });
                    for p in branch.binders {
                        self.bind(*p);
                    }
                    self.visit(branch.body);
                    self.boundaries.pop();
                    self.scope.truncate(depth);
                }
                if let Some(body) = default {
                    self.within(Boundary::Branch { node, index: None }, body);
                }
            }
            CoreKind::App { func, args } => {
                self.visit(func);
                for arg in *args {
                    self.visit(arg);
                }
            }
            CoreKind::Constr { fields: args, .. } | CoreKind::Builtin { args, .. } => {
                for arg in *args {
                    self.visit(arg);
                }
            }
            CoreKind::Field { record, .. } => self.visit(record),
            CoreKind::Trace { message, body } => {
                self.visit(message);
                self.visit(body);
            }
            CoreKind::Delay(body) => self.within(Boundary::Delay(node), body),
            CoreKind::Force(body) => self.visit(body),
            CoreKind::Lit(_) | CoreKind::Error => {}
        }
        self.scope.truncate(depth);
    }
}

/// Whether evaluation is known to terminate without a trace or failure.
/// Requires well-formed, well-scoped Core: every free variable must denote a value
/// in the surrounding environment. Does not prove duplication or motion safe.
/// Saturated builtins and arbitrary calls are deliberately not evaluated here.
pub fn safe_to_discard(core: &Core<'_>) -> bool {
    match &core.kind {
        CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Delay(_) => true,
        CoreKind::Lam { params, body } => !params.is_empty() || safe_to_discard(body),
        CoreKind::Builtin { func, args } => {
            args.len() < func.arity() && args.iter().all(|a| safe_to_discard(a))
        }
        CoreKind::Constr { fields, .. } => fields.iter().all(|a| safe_to_discard(a)),
        CoreKind::Let { value, body, .. } => safe_to_discard(value) && safe_to_discard(body),
        CoreKind::App { .. }
        | CoreKind::LetRec { .. }
        | CoreKind::Case { .. }
        | CoreKind::Field { .. }
        | CoreKind::Trace { .. }
        | CoreKind::Error
        | CoreKind::Force(_) => false,
    }
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
    let mut result = Vec::new();
    free(core, &mut Vec::new(), &mut HashSet::new(), &mut result);
    result
}
fn free<'a>(
    core: &Core<'a>,
    scope: &mut Vec<u32>,
    seen: &mut HashSet<u32>,
    out: &mut Vec<Name<'a>>,
) {
    let depth = scope.len();
    match &core.kind {
        CoreKind::Var(name) => {
            if !scope.contains(&name.unique) && seen.insert(name.unique) {
                out.push(*name);
            }
        }
        CoreKind::Lam { params, body } => {
            scope.extend(params.iter().map(|p| p.name.unique));
            free(body, scope, seen, out);
        }
        CoreKind::App { func, args } => {
            free(func, scope, seen, out);
            for arg in *args {
                free(arg, scope, seen, out);
            }
        }
        CoreKind::Let {
            binder,
            value,
            body,
        } => {
            free(value, scope, seen, out);
            scope.push(binder.name.unique);
            free(body, scope, seen, out);
        }
        CoreKind::LetRec { binders, body } => {
            scope.extend(binders.iter().map(|rb| rb.binder.name.unique));
            let group_depth = scope.len();
            for rb in *binders {
                scope.extend(rb.params.iter().map(|p| p.name.unique));
                free(rb.body, scope, seen, out);
                scope.truncate(group_depth);
            }
            free(body, scope, seen, out);
        }
        CoreKind::Case {
            scrutinee,
            branches,
            default,
            ..
        } => {
            free(scrutinee, scope, seen, out);
            for branch in *branches {
                scope.extend(branch.binders.iter().map(|p| p.name.unique));
                free(branch.body, scope, seen, out);
                scope.truncate(depth);
            }
            if let Some(body) = default {
                free(body, scope, seen, out);
            }
        }
        CoreKind::Constr { fields, .. } => {
            for field in *fields {
                free(field, scope, seen, out);
            }
        }
        CoreKind::Builtin { args, .. } => {
            for arg in *args {
                free(arg, scope, seen, out);
            }
        }
        CoreKind::Field { record, .. } => free(record, scope, seen, out),
        CoreKind::Trace { message, body } => {
            free(message, scope, seen, out);
            free(body, scope, seen, out);
        }
        CoreKind::Delay(body) | CoreKind::Force(body) => free(body, scope, seen, out),
        CoreKind::Lit(_) | CoreKind::Error => {}
    }
    scope.truncate(depth);
}

#[cfg(test)]
mod tests;
