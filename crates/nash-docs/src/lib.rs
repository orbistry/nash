//! Owned documentation extracted from source comments and solved interfaces.
mod highlight;
pub mod project;
mod render;
pub use render::{Format, render};
mod extract;
mod types;
pub use extract::{extract, primitives};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDocs {
    pub name: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize)]
pub enum Block {
    Text(String),
    Declaration(Declaration),
}

#[derive(Debug, Clone, Serialize)]
pub struct Declaration {
    pub name: String,
    pub kind: DeclarationKind,
    pub signature: String,
    pub type_kind: Option<String>,
    pub doc: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum DeclarationKind {
    Value,
    Union,
    Alias,
    Operator,
    Trait,
    Implementation,
    Builtin,
    Primitive,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocsWarning {
    pub module: String,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Extraction {
    pub module: ModuleDocs,
    pub warnings: Vec<DocsWarning>,
}

#[cfg(test)]
mod tests;
