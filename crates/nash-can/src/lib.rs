mod accumulate;
mod entailment;
pub mod environment;
mod error;
pub mod expression;
mod impls;
mod interface;
pub mod kinds;
mod module;
pub mod pattern;
mod scc;
mod traits;
pub mod types;
pub mod warning;

pub use crate::entailment::Failure as EntailmentFailure;
pub use crate::error::{
    BadArityContext, BadHead, DuplicatePatternContext, Error, KindContext, PossibleNames, VarKind,
};
pub use crate::interface::{
    AliasVisibility, Annotations, Interface, InterfaceAlias, InterfaceBinop, InterfaceMethod,
    InterfaceTrait, InterfaceUnion, InterfaceValue, UnionVisibility, from_module,
};
pub use crate::module::{CanResult, Context, canonicalize};
pub use crate::warning::{Warning, WarningContext};
