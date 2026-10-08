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
    Rewriter::new(build, &[core]).term(core, &HashMap::new(), None)
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
    Rewriter::new(build, &[core, replacement]).term(
        core,
        &HashMap::new(),
        Some((target, replacement)),
    )
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

    fn terms(
        &mut self,
        terms: &[&'a Core<'a>],
        scope: &HashMap<u32, Name<'a>>,
        substitution: Option<(u32, &'a Core<'a>)>,
    ) -> Vec<&'a Core<'a>> {
        terms
            .iter()
            .map(|t| self.term(t, scope, substitution))
            .collect()
    }

    fn term(
        &mut self,
        core: &'a Core<'a>,
        scope: &HashMap<u32, Name<'a>>,
        substitution: Option<(u32, &'a Core<'a>)>,
    ) -> &'a Core<'a> {
        let b = self.build;
        let rewritten = match &core.kind {
            CoreKind::Var(name) => {
                if let Some(renamed) = scope.get(&name.unique) {
                    return b.var(*renamed, core.ty);
                }
                if let Some((target, replacement)) = substitution
                    && name.unique == target
                {
                    let replacement = self.term(replacement, &HashMap::new(), None);
                    return b.with_type(replacement, core.ty);
                }
                core
            }
            CoreKind::Lit(_) | CoreKind::Error => core,
            CoreKind::Lam { params, body } => {
                let mut inner = scope.clone();
                let params = self.binders(params, &mut inner);
                let body = self.term(body, &inner, substitution);
                b.lam(params, body)
            }
            CoreKind::Let {
                binder,
                value,
                body,
            } => {
                let value = self.term(value, scope, substitution);
                let mut inner = scope.clone();
                let binder = self.binder(*binder, &mut inner);
                let body = self.term(body, &inner, substitution);
                b.let_(binder, value, body)
            }
            CoreKind::LetRec { binders, body } => {
                let mut group = scope.clone();
                let names: Vec<_> = binders
                    .iter()
                    .map(|r| self.binder(r.binder, &mut group))
                    .collect();
                let recs: Vec<_> = binders
                    .iter()
                    .zip(names)
                    .map(|(rec, binder)| {
                        let mut inner = group.clone();
                        let params = self.binders(rec.params, &mut inner);
                        let body = self.term(rec.body, &inner, substitution);
                        RecBinder {
                            binder,
                            params,
                            body,
                            ..*rec
                        }
                    })
                    .collect();
                let body = self.term(body, &group, substitution);
                b.let_rec(&recs, body)
            }
            CoreKind::Case {
                kind,
                scrutinee,
                branches,
                default,
            } => {
                let scrutinee = self.term(scrutinee, scope, substitution);
                let branches: Vec<_> = branches
                    .iter()
                    .map(|branch| {
                        let mut inner = scope.clone();
                        let binders = self.binders(branch.binders, &mut inner);
                        let body = self.term(branch.body, &inner, substitution);
                        Branch {
                            binders,
                            body,
                            ..*branch
                        }
                    })
                    .collect();
                let default = default.map(|d| self.term(d, scope, substitution));
                b.case(*kind, scrutinee, &branches, default, core.ty)
            }
            CoreKind::App { func, args } => {
                let func = self.term(func, scope, substitution);
                let args = self.terms(args, scope, substitution);
                b.app(func, &args, core.ty)
            }
            CoreKind::Constr { tag, fields } => {
                let fields = self.terms(fields, scope, substitution);
                b.constr(*tag, &fields, core.ty)
            }
            CoreKind::Builtin { func, args } => {
                let args = self.terms(args, scope, substitution);
                b.builtin(*func, &args, core.ty)
            }
            CoreKind::Field {
                record,
                index,
                arity,
            } => {
                let record = self.term(record, scope, substitution);
                b.field(record, *index, *arity, core.ty)
            }
            CoreKind::Trace { message, body } => {
                let message = self.term(message, scope, substitution);
                let body = self.term(body, scope, substitution);
                b.trace(message, body)
            }
            CoreKind::Delay(body) => {
                let body = self.term(body, scope, substitution);
                b.delay(body)
            }
            CoreKind::Force(body) => {
                let body = self.term(body, scope, substitution);
                b.force(body, core.ty)
            }
        };
        b.with_type(rewritten, core.ty)
    }
}

#[cfg(test)]
mod tests;
