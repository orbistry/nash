//! Canonicalizer behaviour tests that snapshot rendered diagnostics.
//!
//! They live here rather than in `nash-can` so that crate has no
//! dev-dependency on its own reporter: a versioned dev-dependency cycle blocks
//! `cargo publish --workspace`.
mod snapshot_support;

mod core_casts;
mod do_notation;
mod impls;
mod kind_predicates;
mod kinds;
mod module;
mod pattern;
mod structural_eq;
mod traits;
mod twins;
mod types;
