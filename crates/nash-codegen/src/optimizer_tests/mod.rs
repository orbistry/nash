//! Cross-phase optimizer semantics. Snapshots pair Core and UPLC before/after.
//! Source-level snapshots live under build/tests/optimizer.rs.
//! Performance assertions belong only in tools/optimizer-perf.
mod anf;
mod beta;
mod builtin_sharing;
mod constant_sharing;
mod dead_code;
mod force_delay;
mod known_case;
mod propagate;
mod single_use;
mod small_inline;
mod unused_params;

mod constant_fold;
mod inverse;

mod pair_projection;

mod recursive_params;
