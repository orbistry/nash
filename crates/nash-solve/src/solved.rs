//! Stable arena-node keys shared with Plan 07's code generation.
pub use nash_ast::NodeId;
use nash_ast::{Annotation, Evidence, Type};
use nash_region::Located;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SolvedTypes<'a> {
    /// Solved types of original canonical nodes, named in their owner's scope.
    pub exprs: HashMap<NodeId, &'a Located<Type<'a>>>,
    pub patterns: HashMap<NodeId, &'a Located<Type<'a>>>,
    pub instances: HashMap<NodeId, Instance<'a>>,
    /// Named definitions and aggregate let-destructuring patterns.
    pub schemes: HashMap<NodeId, Scheme<'a>>,
    /// Inference-selected operations for source-site-sensitive ascriptions.
    pub conversions: HashMap<NodeId, nash_ast::ConversionKind>,
    /// Whether a type-directed pipeline inserts its input into the argument group.
    pub pipe_insertions: HashMap<NodeId, bool>,
    /// Record-field precedence over a same-named imported module.
    pub field_selections: HashMap<NodeId, bool>,
    /// Declaration-order indices for calls resolved after field/module selection.
    pub call_orders: HashMap<NodeId, &'a [usize]>,
    /// Owned refinements awaiting whole-module acceptance, including nitpick.
    pub declared_refinements: Vec<(
        nash_ast::DeclaredHoleId,
        Option<nash_ast::declared::OwnedType>,
    )>,
}

impl SolvedTypes<'_> {
    pub fn commit_declared_holes(&mut self, store: &nash_ast::declared::DeclaredStore) {
        store.commit(self.declared_refinements.drain(..));
    }
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
