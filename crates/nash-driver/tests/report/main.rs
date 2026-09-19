//! Reporter tests that need the real solver.
//!
//! They live here rather than in `nash-report` so that crate has no
//! dev-dependency on `nash-solve`: Sampo publishes crate by crate in its own
//! order, and a versioned dev-dependency on a not-yet-published crate fails.
mod pattern;
mod type_;
mod type_diff;
