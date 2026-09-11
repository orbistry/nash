//! Assemble reachable Core bindings into a closed Plutus V3 program.
use nash_ir::{
    build::Builder,
    core::{Binder, Core, Module, Name as CoreName},
};
use nash_plutus::{
    arena::Arena,
    binder::{DeBruijn, Name},
    debruijn,
    program::{Program, Version},
    term::Term,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Compiled<'a> {
    pub program: &'a Program<'a, DeBruijn>,
    /// Named UPLC for human-readable build output.
    pub named: &'a Term<'a, Name<'a>>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error("program is not closed: free variable {0:?}")]
    NotClosed(CoreName<'a>),
    #[error("module repeats a top-level binding: {0:?}")]
    DuplicateBinding(CoreName<'a>),
    #[error("{0}")]
    Recursion(#[from] crate::recursion::Error),
    #[error("{0}")]
    Lower(#[from] crate::lower::Error),
    #[error("{0}")]
    DeBruijn(debruijn::FreeVariable<'a>),
}

/// Bindings must be in dependency order, with recursive groups represented by
/// Core::LetRec. Unreachable definitions are omitted before evaluation.
pub fn assemble<'a>(arena: &'a Arena, module: &Module<'a>) -> Result<Compiled<'a>, Error<'a>> {
    let mut names = HashSet::new();
    for (binder, _) in module.bindings {
        if !names.insert(binder.name.unique) {
            return Err(Error::DuplicateBinding(binder.name));
        }
    }
    let build = Builder::new(arena);
    let body = reachable(module.bindings, module.root)
        .iter()
        .rev()
        .fold(module.root, |body, (binder, value)| {
            build.let_(*binder, value, body)
        });
    assemble_core(arena, body)
}

/// Assemble an already wrapped Core root. Expand casts before calling this;
/// recursion is rewritten here. Optimization is deferred to Plan 08.
pub fn assemble_core<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Result<Compiled<'a>, Error<'a>> {
    if let Some(name) = free_variables(core).first() {
        return Err(Error::NotClosed(*name));
    }
    let build = Builder::new(arena);
    let core = crate::recursion::rewrite(&build, core)?;
    let named = crate::lower::lower(arena, core)?;
    let term = debruijn::to_debruijn(arena, named).map_err(Error::DeBruijn)?;
    Ok(Compiled {
        program: Program::new(arena, Version::plutus_v3(arena), term),
        named,
    })
}

/// Select a transitive dependency closure in the original binding order.
/// Top-level binder uniques must be distinct, as checked by `assemble`.
pub fn reachable<'a>(
    bindings: &[(Binder<'a>, &'a Core<'a>)],
    root: &Core<'a>,
) -> Vec<(Binder<'a>, &'a Core<'a>)> {
    let by_name = bindings
        .iter()
        .enumerate()
        .map(|(i, (binder, _))| (binder.name.unique, i))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut pending = free_variables(root);
    while let Some(name) = pending.pop() {
        if let Some(&index) = by_name.get(&name.unique)
            && seen.insert(index)
        {
            pending.extend(free_variables(bindings[index].1));
        }
    }
    bindings
        .iter()
        .enumerate()
        .filter(|(i, _)| seen.contains(i))
        .map(|(_, binding)| *binding)
        .collect()
}

/// Free runtime names in deterministic source traversal order. Name identity
/// follows the unique number, exactly as UPLC De Bruijn conversion does.
pub fn free_variables<'a>(core: &Core<'a>) -> Vec<CoreName<'a>> {
    let mut result = Vec::new();
    free(core, &mut Vec::new(), &mut HashSet::new(), &mut result);
    result
}
fn free<'a>(
    core: &Core<'a>,
    scope: &mut Vec<u32>,
    seen: &mut HashSet<u32>,
    out: &mut Vec<CoreName<'a>>,
) {
    let depth = scope.len();
    match core {
        Core::Var(name) => {
            if !scope.contains(&name.unique) && seen.insert(name.unique) {
                out.push(*name);
            }
        }
        Core::Lam { params, body } => {
            scope.extend(params.iter().map(|p| p.name.unique));
            free(body, scope, seen, out);
        }
        Core::App { func, args } => {
            free(func, scope, seen, out);
            for arg in *args {
                free(arg, scope, seen, out);
            }
        }
        Core::Let {
            binder,
            value,
            body,
        } => {
            free(value, scope, seen, out);
            scope.push(binder.name.unique);
            free(body, scope, seen, out);
        }
        Core::LetRec { binders, body } => {
            scope.extend(binders.iter().map(|rb| rb.binder.name.unique));
            let group_depth = scope.len();
            for rb in *binders {
                scope.extend(rb.params.iter().map(|p| p.name.unique));
                free(rb.body, scope, seen, out);
                scope.truncate(group_depth);
            }
            free(body, scope, seen, out);
        }
        Core::Case {
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
        Core::Constr { fields, .. } => {
            for field in *fields {
                free(field, scope, seen, out);
            }
        }
        Core::Builtin { args, .. } => {
            for arg in *args {
                free(arg, scope, seen, out);
            }
        }
        Core::Field { record, .. } => free(record, scope, seen, out),
        Core::Cast { arg, .. } => free(arg, scope, seen, out),
        Core::Trace { message, body } => {
            free(message, scope, seen, out);
            free(body, scope, seen, out);
        }
        Core::Delay(body) | Core::Force(body) => free(body, scope, seen, out),
        Core::Lit(_) | Core::Error => {}
    }
    scope.truncate(depth);
}

#[cfg(test)]
mod tests;
