//! Parser error reports rendered from real parse failures.
//!
//! They live here rather than in `nash-parse` so that crate has no
//! dev-dependency on its reporter: a versioned dev-dependency cycle blocks
//! `cargo publish --workspace`. Successful-parse AST snapshots stay in `nash-parse`.
mod support;

mod declaration;
mod exposing;
mod expression;
mod module;
mod pattern;
mod tests_block;
mod type_;
