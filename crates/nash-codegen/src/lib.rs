//! Compile-time specialization and lowering to Untyped Plutus Core.
pub mod lower;
pub mod recursion;

#[cfg(test)]
pub(crate) mod harness;

pub mod builtins;

pub mod ty_of;

pub mod casts;
pub mod demand;

pub mod decision_tree;

pub mod evidence;
