//! Lexical binding checks and capture-free rebuilding for Core.
//!
//! Numeric `Name::unique` is binding identity, as in UPLC lowering; text is a
//! display label. Validation requires globally distinct binder IDs, even in
//! disjoint scopes. Freshening can repair lexical shadowing, but cannot repair an
//! already incorrect variable reference or prove transformation equivalence.
use crate::{analysis::occurrences, build::Builder, core::*};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HygieneError<'a> {
    DuplicateBinder(Name<'a>),
    UnboundVariable(Name<'a>),
}

/// Check binder uniqueness and lexical scope. External names are supplied by the
/// enclosing scope and must also have distinct IDs. Does not check Core types,
/// case exhaustiveness, builtin arities, or recursive static-parameter metadata.
pub fn validate<'a>(core: &Core<'a>, external: &[Name<'a>]) -> Result<(), Vec<HygieneError<'a>>> {
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    let external_ids: HashSet<_> = external.iter().map(|n| n.unique).collect();
    let report = occurrences(core);
    for name in external
        .iter()
        .copied()
        .chain(report.bindings.iter().map(|b| b.name))
    {
        if !seen.insert(name.unique) {
            errors.push(HygieneError::DuplicateBinder(name));
        }
    }
    for usage in report.uses {
        if usage.binding.is_none() && !external_ids.contains(&usage.name.unique) {
            errors.push(HygieneError::UnboundVariable(usage.name));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Alpha-rename every binder occurrence, preserving lexical shadowing and free
/// variables. A subtree used twice gets separate binder IDs for each occurrence.
/// Generated IDs avoid every ID in `core`, including its free variables, even
/// when `build` is a new name supply. IDs outside this root are unknown: use the
/// whole program's shared Builder when reinserting into an enclosing program.
/// Validate separately to diagnose bad input.
pub fn freshen<'a>(build: &Builder<'a>, core: &'a Core<'a>) -> &'a Core<'a> {
    Rewriter::new(build, &[core]).term(core, None)
}

/// Replace free occurrences of `target` with `replacement`. Binders in both the
/// recipient and each inserted copy get fresh IDs, so neither free replacement
/// variables nor duplicated binders can be captured. A binder with the target ID
/// shadows substitution. The replacement is not recursively substituted. Use the
/// whole program's shared Builder to avoid IDs outside these two input trees.
///
/// The replacement must have the target binding's type. Explicitly coerced views
/// on its occurrences are retained; substitution does not re-typecheck the
/// expression. This is syntactic substitution, not permission to
/// inline: callers must prove preservation of strict evaluation, effects,
/// termination and use counts.
pub fn substitute<'a>(
    build: &Builder<'a>,
    core: &'a Core<'a>,
    target: u32,
    replacement: &'a Core<'a>,
) -> &'a Core<'a> {
    Rewriter::new(build, &[core, replacement]).term(core, Some((target, replacement)))
}

struct Rewriter<'b, 'a> {
    build: &'b Builder<'a>,
    used: HashSet<u32>,
}

impl<'b, 'a> Rewriter<'b, 'a> {
    fn new(build: &'b Builder<'a>, roots: &[&Core<'a>]) -> Self {
        let mut used = HashSet::new();
        for root in roots {
            root.walk(&mut |node| match &node.kind {
                CoreKind::Var(name) => {
                    used.insert(name.unique);
                }
                CoreKind::Lam { params, .. } => used.extend(params.iter().map(|b| b.name.unique)),
                CoreKind::Let { binder, .. } => {
                    used.insert(binder.name.unique);
                }
                CoreKind::LetRec { binders, .. } => {
                    for rec in *binders {
                        used.insert(rec.binder.name.unique);
                        used.extend(rec.params.iter().map(|b| b.name.unique));
                    }
                }
                CoreKind::Case { branches, .. } => {
                    for branch in *branches {
                        used.extend(branch.binders.iter().map(|b| b.name.unique));
                    }
                }
                _ => {}
            });
        }
        Self { build, used }
    }

    fn binder(&mut self, binder: Binder<'a>, scope: &mut HashMap<u32, Name<'a>>) -> Binder<'a> {
        let name = loop {
            let name = self.build.fresh(binder.name.text);
            if self.used.insert(name.unique) {
                break name;
            }
        };
        scope.insert(binder.name.unique, name);
        Binder { name, ..binder }
    }

    fn binders(
        &mut self,
        binders: &[Binder<'a>],
        scope: &mut HashMap<u32, Name<'a>>,
    ) -> &'a [Binder<'a>] {
        let binders: Vec<_> = binders.iter().map(|b| self.binder(*b, scope)).collect();
        self.build.arena.alloc_slice_copy(&binders)
    }

    fn term(
        &mut self,
        root: &'a Core<'a>,
        substitution: Option<(u32, &'a Core<'a>)>,
    ) -> &'a Core<'a> {
        enum Task<'a> {
            Visit(&'a Core<'a>, Option<(u32, &'a Core<'a>)>),
            Finish(&'a Core<'a>, usize),
            LetBody(&'a Core<'a>, usize, Option<(u32, &'a Core<'a>)>),
            Restore(HashMap<u32, Name<'a>>),
            Retype(crate::ty::Ty<'a>),
            RecNext {
                core: &'a Core<'a>,
                names: Vec<Binder<'a>>,
                recs: Vec<RecBinder<'a>>,
                sub: Option<(u32, &'a Core<'a>)>,
            },
            RecBody {
                core: &'a Core<'a>,
                names: Vec<Binder<'a>>,
                recs: Vec<RecBinder<'a>>,
                params: &'a [Binder<'a>],
                sub: Option<(u32, &'a Core<'a>)>,
            },
            RecFinish {
                core: &'a Core<'a>,
                recs: Vec<RecBinder<'a>>,
            },
            CaseNext {
                core: &'a Core<'a>,
                scrutinee: Option<&'a Core<'a>>,
                branches: Vec<Branch<'a>>,
                sub: Option<(u32, &'a Core<'a>)>,
            },
            CaseBody {
                core: &'a Core<'a>,
                scrutinee: &'a Core<'a>,
                branches: Vec<Branch<'a>>,
                binders: &'a [Binder<'a>],
                sub: Option<(u32, &'a Core<'a>)>,
            },
            CaseFinish {
                core: &'a Core<'a>,
                scrutinee: &'a Core<'a>,
                branches: Vec<Branch<'a>>,
            },
        }
        let b = self.build;
        let mut scope = HashMap::new();
        let mut pending = vec![Task::Visit(root, substitution)];
        let mut results = Vec::new();
        let mut children = Vec::new();
        while let Some(task) = pending.pop() {
            match task {
                Task::Restore(previous) => scope = previous,
                Task::Retype(ty) => {
                    let value = results.pop().unwrap();
                    results.push(b.with_type(value, ty));
                }
                Task::Finish(core, start) => {
                    let rebuilt =
                        crate::traverse::rebuild(b, core, &mut results.drain(start..), &mut |_| {
                            None
                        });
                    results.push(rebuilt);
                }
                Task::LetBody(core, start, sub) => {
                    let CoreKind::Let {
                        binder,
                        value,
                        body,
                    } = core.kind
                    else {
                        unreachable!()
                    };
                    let previous = scope.clone();
                    let binder = self.binder(binder, &mut scope);
                    pending.push(Task::Restore(previous));
                    pending.push(Task::Finish(
                        b.alloc(
                            core.ty,
                            CoreKind::Let {
                                binder,
                                value,
                                body,
                            },
                        ),
                        start,
                    ));
                    pending.push(Task::Visit(body, sub));
                }
                Task::RecNext {
                    core,
                    names,
                    recs,
                    sub,
                } => {
                    let CoreKind::LetRec { binders, body } = core.kind else {
                        unreachable!()
                    };
                    if recs.len() == binders.len() {
                        pending.push(Task::RecFinish { core, recs });
                        pending.push(Task::Visit(body, sub));
                    } else {
                        let rec = binders[recs.len()];
                        let previous = scope.clone();
                        let params = self.binders(rec.params, &mut scope);
                        pending.push(Task::RecBody {
                            core,
                            names,
                            recs,
                            params,
                            sub,
                        });
                        pending.push(Task::Restore(previous));
                        pending.push(Task::Visit(rec.body, sub));
                    }
                }
                Task::RecBody {
                    core,
                    names,
                    mut recs,
                    params,
                    sub,
                } => {
                    let CoreKind::LetRec { binders, .. } = core.kind else {
                        unreachable!()
                    };
                    recs.push(RecBinder {
                        binder: names[recs.len()],
                        params,
                        body: results.pop().unwrap(),
                        ..binders[recs.len()]
                    });
                    pending.push(Task::RecNext {
                        core,
                        names,
                        recs,
                        sub,
                    });
                }
                Task::RecFinish { core, recs } => {
                    let body = results.pop().unwrap();
                    results.push(b.with_type(b.let_rec(&recs, body), core.ty));
                }
                Task::CaseNext {
                    core,
                    scrutinee,
                    branches,
                    sub,
                } => {
                    let scrutinee = scrutinee.unwrap_or_else(|| results.pop().unwrap());
                    let CoreKind::Case {
                        branches: original,
                        default,
                        ..
                    } = core.kind
                    else {
                        unreachable!()
                    };
                    if branches.len() == original.len() {
                        pending.push(Task::CaseFinish {
                            core,
                            scrutinee,
                            branches,
                        });
                        if let Some(body) = default {
                            pending.push(Task::Visit(body, sub));
                        }
                    } else {
                        let branch = original[branches.len()];
                        let previous = scope.clone();
                        let binders = self.binders(branch.binders, &mut scope);
                        pending.push(Task::CaseBody {
                            core,
                            scrutinee,
                            branches,
                            binders,
                            sub,
                        });
                        pending.push(Task::Restore(previous));
                        pending.push(Task::Visit(branch.body, sub));
                    }
                }
                Task::CaseBody {
                    core,
                    scrutinee,
                    mut branches,
                    binders,
                    sub,
                } => {
                    let CoreKind::Case {
                        branches: original, ..
                    } = core.kind
                    else {
                        unreachable!()
                    };
                    branches.push(Branch {
                        binders,
                        body: results.pop().unwrap(),
                        ..original[branches.len()]
                    });
                    pending.push(Task::CaseNext {
                        core,
                        scrutinee: Some(scrutinee),
                        branches,
                        sub,
                    });
                }
                Task::CaseFinish {
                    core,
                    scrutinee,
                    branches,
                } => {
                    let CoreKind::Case { kind, default, .. } = core.kind else {
                        unreachable!()
                    };
                    let default = default.map(|_| results.pop().unwrap());
                    results.push(b.case(kind, scrutinee, &branches, default, core.ty));
                }
                Task::Visit(core, sub) => match core.kind {
                    CoreKind::Var(name) => {
                        if let Some(renamed) = scope.get(&name.unique) {
                            results.push(b.var(*renamed, core.ty));
                        } else if let Some((target, replacement)) = sub
                            && name.unique == target
                        {
                            pending.push(Task::Restore(std::mem::take(&mut scope)));
                            pending.push(Task::Retype(core.ty));
                            pending.push(Task::Visit(replacement, None));
                        } else {
                            results.push(core);
                        }
                    }
                    CoreKind::Lit(_) | CoreKind::Error => results.push(core),
                    CoreKind::Lam { params, body } => {
                        let previous = scope.clone();
                        let params = self.binders(params, &mut scope);
                        pending.push(Task::Restore(previous));
                        pending.push(Task::Finish(
                            b.alloc(core.ty, CoreKind::Lam { params, body }),
                            results.len(),
                        ));
                        pending.push(Task::Visit(body, sub));
                    }
                    CoreKind::Let { value, .. } => {
                        pending.push(Task::LetBody(core, results.len(), sub));
                        pending.push(Task::Visit(value, sub));
                    }
                    CoreKind::LetRec { binders, .. } => {
                        let previous = scope.clone();
                        let names = binders
                            .iter()
                            .map(|rec| self.binder(rec.binder, &mut scope))
                            .collect();
                        pending.push(Task::Restore(previous));
                        pending.push(Task::RecNext {
                            core,
                            names,
                            recs: Vec::new(),
                            sub,
                        });
                    }
                    CoreKind::Case { scrutinee, .. } => {
                        pending.push(Task::CaseNext {
                            core,
                            scrutinee: None,
                            branches: Vec::new(),
                            sub,
                        });
                        pending.push(Task::Visit(scrutinee, sub));
                    }
                    _ => {
                        pending.push(Task::Finish(core, results.len()));
                        children.clear();
                        core.push_children_reversed(&mut children);
                        pending.extend(children.iter().map(|child| Task::Visit(child, sub)));
                    }
                },
            }
        }
        results.pop().unwrap()
    }
}

#[cfg(test)]
mod tests;
