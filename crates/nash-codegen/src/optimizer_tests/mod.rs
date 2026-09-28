//! Cross-phase optimizer semantics. Snapshots pair Core and UPLC before/after.
//! Source-level snapshots live under build/tests/optimizer.rs.
//! Performance assertions belong only in tools/optimizer-perf.
pub(crate) mod anf;
mod beta;
mod builtin_sharing;
mod constant_sharing;
mod dead_bindings;
mod dead_recursive;
mod force_delay;
mod known_bool;
mod known_constr;
mod known_data;
mod known_list;
mod propagate;
mod single_use;
mod small_inline;
mod unused_params;
mod unused_params_pre_anf;
