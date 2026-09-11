//! Shared union-find types, canonical instantiation, and inference diagnostics.

pub mod error;
pub mod error_type;
pub mod instantiate;
pub mod module;
pub mod type_;

mod union_find;

pub use crate::error::{
    Category, Context, Error, Expected, MaybeName, PCategory, PContext, PExpected, SubContext,
};
pub use crate::error_type::ErrorType;
pub use crate::type_::{Content, Descriptor, FlatType, Mark};
pub use crate::union_find::{UnionFind, Variable};
