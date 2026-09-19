//! Solver inference tests that snapshot rendered type errors.
//!
//! They live here rather than in `nash-solve` or `nash-report` so that neither
//! crate dev-depends on the other: Sampo publishes crate by crate in its own
//! order, and a versioned dev-dependency on a not-yet-published crate fails.
mod snapshot_support;

mod inference;
