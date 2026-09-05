//! The entry point of nash's type inference: a port of Elm's `Type.Solve`,
//! `Type.Unify`, and `Type.Occurs`, plus the `toAnnotation`/`toErrorType`
//! half of `Type.Type`.
//!
//! The driver runs `nash_constrain::constrain` to build a constraint tree
//! (filling a `UnionFind` store with fresh variables), then [`run`] to solve
//! it. Success returns the annotations used by `nash_can::from_module`
//! together with definition schemes and use-site instances in [`SolvedTypes`].

mod annotation;
mod occurs;
pub mod preds;
mod resolve;
mod solve;
pub mod solved;
mod unify;

pub use crate::annotation::{to_annotation, to_annotation_with_context, to_error_type};
pub use crate::solve::run;
pub use crate::solved::SolvedTypes;
pub use crate::unify::{Answer, unify};
