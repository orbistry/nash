//! Bounded compile-time evaluation of closed Core to a UPLC constant.
use crate::program;
use nash_ir::core::{Binder, Core, Module, Name};
use nash_plutus::{
    arena::Arena,
    constant::Constant,
    machine::{ExBudget, PlutusVersion},
    term::Term,
};

#[derive(Debug, thiserror::Error)]
pub enum ComptimeError<'a> {
    #[error("comptime expression is not closed: {0:?}")]
    NotClosed(Name<'a>),
    #[error("comptime evaluation failed: {0}")]
    Evaluation(String),
    #[error("comptime expression did not produce a constant")]
    NotAConstant,
    #[error("could not assemble comptime expression: {0}")]
    Assembly(program::Error<'a>),
}

/// Include only dependencies reachable from `core`, then use the normal finite
/// CEK budget. The caller must first expand representation casts.
pub fn eval_closed<'a>(
    arena: &'a Arena,
    bindings: &[(Binder<'a>, &'a Core<'a>)],
    core: &'a Core<'a>,
) -> Result<&'a Constant<'a>, ComptimeError<'a>> {
    let module = Module {
        bindings: arena.alloc_slice_copy(bindings),
        root: core,
    };
    let compiled = program::assemble(arena, &module).map_err(|error| match error {
        program::Error::NotClosed(name) => ComptimeError::NotClosed(name),
        other => ComptimeError::Assembly(other),
    })?;
    let evaluation =
        compiled
            .program
            .eval_version_budget(arena, PlutusVersion::V3, ExBudget::default());
    match evaluation.term {
        Ok(Term::Constant(constant)) => Ok(constant),
        Ok(_) => Err(ComptimeError::NotAConstant),
        Err(error) => {
            let mut reason = format!("{error:?}");
            if !evaluation.info.logs.is_empty() {
                reason.push_str("\ntraces: ");
                reason.push_str(&evaluation.info.logs.join("\n"));
            }
            Err(ComptimeError::Evaluation(reason))
        }
    }
}
