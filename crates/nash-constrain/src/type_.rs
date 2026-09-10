//! Union-find type descriptors, scheme identities, and literal annotations.
//!
//! `toAnnotation` and `toErrorType` live in `nash-solve` (they are only
//! called by the solver and need `nash-can`'s canonical-type utilities).

use std::collections::BTreeMap;

use nash_ast::{Annotation, ModuleName, NodeId, QualifiedName};
use nash_region::{Located, Region};

use crate::union_find::{UnionFind, Variable};

// CONSTRAINTS

/// Scheme identity is an original definition name or a destructuring pattern.
#[derive(Clone, Copy, Debug)]
pub enum Binder<'a> {
    Named(&'a Located<&'a str>),
    Pattern {
        node: NodeId,
        name: &'a Located<&'a str>,
    },
}

impl<'a> Binder<'a> {
    pub fn node(self) -> NodeId {
        match self {
            Self::Named(name) => NodeId::def(name),
            Self::Pattern { node, .. } => node,
        }
    }

    pub fn name(self) -> &'a Located<&'a str> {
        match self {
            Self::Named(name) | Self::Pattern { name, .. } => name,
        }
    }
}

// TYPE PRIMITIVES

#[derive(Clone, Copy, Debug)]
pub enum FieldContext<'a> {
    Access {
        record_region: Region,
        maybe_name: Option<&'a str>,
    },
    Accessor,
    Update {
        record: &'a str,
    },
    Pattern,
}

/// Elm's `Type.FlatType`. Lives inside descriptors owned by the union-find
/// store (real heap, so owned containers are fine here).
#[derive(Clone, Debug)]
pub enum FlatType<'a> {
    App1(ModuleName<'a>, &'a str, Vec<Variable>),
    AppV1(Variable, Vec<Variable>),
    Fun1(Variable, Variable),
    Record1(BTreeMap<&'a str, Variable>),
    Tuple1(Variable, Variable, Vec<Variable>),
}

/// Flatten application spines whose heads inference has already determined.
/// This does not bind unknown heads or expand aliases.
pub fn normalize_application<'a>(uf: &mut UnionFind<'a>, term: FlatType<'a>) -> FlatType<'a> {
    let FlatType::AppV1(mut head, mut args) = term else {
        return term;
    };
    let mut seen = std::collections::BTreeSet::new();
    while seen.insert(uf.find(head)) {
        match uf.get(head).content.clone() {
            Content::Structure(FlatType::App1(home, name, mut prefix)) => {
                prefix.extend(args);
                return FlatType::App1(home, name, prefix);
            }
            Content::Structure(FlatType::AppV1(inner, mut prefix)) => {
                prefix.extend(args);
                head = inner;
                args = prefix;
            }
            _ => break,
        }
    }
    FlatType::AppV1(head, args)
}

// DESCRIPTORS

/// Index into the solver's predicate store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PredId(pub u32);

#[derive(Clone, Debug)]
pub struct Descriptor<'a> {
    /// Pending predicates mentioning this equivalence class.
    pub preds: Vec<PredId>,
    pub content: Content<'a>,
    pub rank: usize,
    pub mark: Mark,
    pub copy: Option<Variable>,
}

#[derive(Clone, Debug)]
pub enum Content<'a> {
    FlexVar(Option<&'a str>),
    RigidVar(&'a str),
    Structure(FlatType<'a>),
    PartialAlias {
        home: ModuleName<'a>,
        name: &'a str,
        args: Vec<(&'a str, Variable)>,
        remaining: Vec<&'a str>,
        body: &'a Located<nash_ast::Type<'a>>,
    },
    Alias {
        home: ModuleName<'a>,
        name: &'a str,
        args: Vec<(&'a str, Variable)>,
        real: Variable,
        body: &'a Located<nash_ast::Type<'a>>,
    },
    Error,
}

pub fn make_descriptor(content: Content<'_>) -> Descriptor<'_> {
    Descriptor {
        preds: Vec::new(),
        content,
        rank: NO_RANK,
        mark: NO_MARK,
        copy: None,
    }
}

// RANKS

pub const NO_RANK: usize = 0;
pub const OUTERMOST_RANK: usize = 1;

// MARKS

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mark(u32);

pub const NO_MARK: Mark = Mark(2);
pub const OCCURS_MARK: Mark = Mark(1);
/// Reserved in Elm for `getVarNames` visit tracking. Nash's `get_var_names`
/// uses a per-call seen set instead (see `nash-solve/src/annotation.rs`),
/// but the mark stays reserved so the mark space matches Elm's.
pub const GET_VAR_NAMES_MARK: Mark = Mark(0);

impl Mark {
    pub fn next(self) -> Mark {
        Mark(self.0 + 1)
    }
}

// PRIMITIVE TYPES

pub const fn literal_trait(name: &str) -> QualifiedName<'_> {
    QualifiedName {
        home: nash_ast::primitives::literal_home(),
        name,
    }
}

pub const fn eq_trait<'a>() -> QualifiedName<'a> {
    QualifiedName {
        home: ModuleName {
            package: Some(nash_ast::primitives::CORE),
            name: "Eq",
        },
        name: "Eq",
    }
}

/// One scheme and evidence ordering for a literal use, including pattern Eq.
pub fn literal_annotation<'a>(
    bump: &'a bumpalo::Bump,
    traits: &[QualifiedName<'a>],
) -> &'a Annotation<'a> {
    let typ: &'a Located<nash_ast::Type<'a>> =
        bump.alloc(Located::at_zero(nash_ast::Type::Var("a")));
    bump.alloc(Annotation {
        free_vars: &["a"],
        context: bump.alloc_slice_fill_iter(traits.iter().map(|trait_| nash_ast::Pred::Trait {
            trait_: *trait_,
            args: bump.alloc_slice_copy(&[typ]),
        })),
        typ,
    })
}

/// Only the compiler-known literal traits select a little default type.
pub fn literal_default(trait_: nash_ast::QualifiedName<'_>) -> Option<FlatType<'static>> {
    if trait_.home.package != Some(nash_ast::primitives::CORE) || trait_.home.name != "Literal" {
        return None;
    }
    let name = match trait_.name {
        "FromInt" => "int",
        "FromString" => "string",
        "FromBytes" => "bytes",
        _ => return None,
    };
    Some(FlatType::App1(
        nash_ast::primitives::builtin_home(),
        name,
        Vec::new(),
    ))
}

// MAKE FLEX VARIABLES

pub fn mk_flex_var<'a>(uf: &mut UnionFind<'a>) -> Variable {
    uf.fresh(make_descriptor(unnamed_flex_var()))
}

pub const fn unnamed_flex_var<'a>() -> Content<'a> {
    Content::FlexVar(None)
}

// MAKE NAMED VARIABLES

pub fn name_to_flex<'a>(uf: &mut UnionFind<'a>, name: &'a str) -> Variable {
    uf.fresh(make_descriptor(Content::FlexVar(Some(name))))
}

pub fn name_to_rigid<'a>(uf: &mut UnionFind<'a>, name: &'a str) -> Variable {
    uf.fresh(make_descriptor(Content::RigidVar(name)))
}
