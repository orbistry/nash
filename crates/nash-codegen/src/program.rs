//! Assemble reachable Core bindings into a closed Plutus V3 program.
use nash_ir::{
    build::Builder,
    core::{Binder, Core, Module, Name as CoreName},
};
use nash_plutus::{
    arena::Arena,
    binder::{DeBruijn, Name},
    debruijn,
    machine::PlutusVersion,
    program::{Program, Version},
    script,
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
    #[error("{0}")]
    Target(#[from] script::TargetError),
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

/// Assemble an already wrapped Core root through the O0 pipeline.
pub fn assemble_core<'a>(arena: &'a Arena, core: &'a Core<'a>) -> Result<Compiled<'a>, Error<'a>> {
    assemble_core_for_version(arena, core, PlutusVersion::V3)
}

/// Assemble and validate for the selected ledger language at the PV11 baseline.
pub fn assemble_core_for_version<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
    version: PlutusVersion,
) -> Result<Compiled<'a>, Error<'a>> {
    assemble_core_with_options(arena, core, version, nash_config::OptimizationLevel::O0)
}

/// Assemble at the selected optimization level without changing trace policy.
pub fn assemble_core_with_options<'a>(
    arena: &'a Arena,
    core: &'a Core<'a>,
    version: PlutusVersion,
    optimize: nash_config::OptimizationLevel,
) -> Result<Compiled<'a>, Error<'a>> {
    if let Some(name) = free_variables(core).first() {
        return Err(Error::NotClosed(*name));
    }
    let core = match optimize {
        nash_config::OptimizationLevel::O0 => core,
        nash_config::OptimizationLevel::O1 => crate::optimizer::optimize(arena, core),
    };
    let build = Builder::new(arena);
    let core = crate::recursion::rewrite(&build, core)?;
    let named = match optimize {
        nash_config::OptimizationLevel::O0 => crate::lower::lower(arena, core)?,
        nash_config::OptimizationLevel::O1 => {
            let core = nash_ir::hygiene::freshen(&build, core);
            crate::lower::lower_with_constant_sharing(arena, core)?
        }
    };
    let term = debruijn::to_debruijn(arena, named).map_err(Error::DeBruijn)?;
    let uplc_version = Version::plutus_v3(arena);
    let program = Program::new(arena, uplc_version, term);
    script::validate_program(program, version)?;
    Ok(Compiled { program, named })
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

// Preserve the codegen API while sharing the existing scope analysis with IR passes.
pub use nash_ir::analysis::free_variables;

#[cfg(test)]
mod tests;
