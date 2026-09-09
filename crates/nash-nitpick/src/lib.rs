//! Pattern coverage checking, ported from Elm's `Nitpick/PatternMatches.hs`.

pub mod pattern;
pub mod render;

pub use pattern::{Context, Error, Literal, Pattern};

#[cfg(test)]
mod render_tests;
