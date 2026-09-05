//! Stable arena-node keys shared with Plan 07's code generation.
pub use nash_ast::NodeId;
use nash_ast::{Annotation, Evidence, Type};
use nash_region::Located;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SolvedTypes<'a> {
    /// Filled by Plan 07's expression/pattern type recording.
    pub exprs: HashMap<NodeId, &'a Located<Type<'a>>>,
    pub patterns: HashMap<NodeId, &'a Located<Type<'a>>>,
    pub instances: HashMap<NodeId, Instance<'a>>,
    /// Named definitions and aggregate let-destructuring patterns.
    pub schemes: HashMap<NodeId, Scheme<'a>>,
}

#[derive(Debug)]
pub struct Instance<'a> {
    /// In the called scheme's free_vars order.
    pub type_args: &'a [&'a Located<Type<'a>>],
    /// In the called scheme's context order.
    pub evidence: &'a [Evidence<'a>],
}

#[derive(Debug)]
pub struct Scheme<'a> {
    pub annotation: &'a Annotation<'a>,
    /// Original definition name (first in an untyped recursive group),
    /// or the root pattern of a generalized let-destructuring.
    pub binder: NodeId,
}
