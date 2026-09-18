//! Solver inference tests that snapshot rendered type errors.
//!
//! They live here rather than in `nash-solve` so that crate has no
//! dev-dependency on its reporter: a versioned dev-dependency cycle blocks
//! `cargo publish --workspace`.
mod snapshot_support;

mod inference;
