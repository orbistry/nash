//! Port of the data half of Elm's `Type.Type`: constraints, the inference
//! `Type` language, and unification variable descriptors.
//!
//! `toAnnotation` and `toErrorType` live in `nash-solve` (they are only
//! called by the solver and need `nash-can`'s canonical-type utilities).

use std::collections::BTreeMap;

use nash_ast::{Annotation, ModuleName, NodeId, QualifiedName};
use nash_region::{Located, Region};

use crate::error::{Category, Expected, PCategory, PExpected};
use crate::union_find::{UnionFind, Variable};

// CONSTRAINTS

/// An annotation predicate instantiated over the definition's rigid variables.
#[derive(Clone, Copy, Debug)]
pub enum Pred<'a> {
    Trait {
        trait_: QualifiedName<'a>,
        args: &'a [&'a Type<'a>],
        hidden: bool,
    },
    Apply {
        head: &'a Type<'a>,
        args: &'a [&'a Type<'a>],
    },
}

impl<'a> Pred<'a> {
    pub fn types(self) -> impl Iterator<Item = &'a Type<'a>> {
        let (head, args) = match self {
            Self::Trait { args, .. } => (None, args),
            Self::Apply { head, args } => (Some(head), args),
        };
        head.into_iter().chain(args.iter().copied())
    }
}

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

/// Preserve the original scheme identity and full type independently of lexical scope.
#[derive(Clone, Copy, Debug)]
pub struct Definition<'a> {
    pub site: Binder<'a>,
    pub typ: &'a Type<'a>,
    /// `Some`, including an empty slice, distinguishes a declared scheme.
    pub context: Option<&'a [Pred<'a>]>,
}

/// Elm's `Type.Constraint`. Allocated in a bump arena, so collections are
/// slices, not owned containers.
#[derive(Debug)]
pub enum Constraint<'a> {
    True,
    SaveTheEnvironment,
    Equal(
        Region,
        Category<'a>,
        &'a Type<'a>,
        Expected<'a, &'a Type<'a>>,
    ),
    Local(Region, NodeId, &'a str, Expected<'a, &'a Type<'a>>),
    Foreign(
        Region,
        NodeId,
        &'a str,
        &'a Annotation<'a>,
        Expected<'a, &'a Type<'a>>,
    ),
    Pattern(
        Region,
        PCategory<'a>,
        &'a Type<'a>,
        PExpected<'a, &'a Type<'a>>,
    ),
    And(&'a [Constraint<'a>]),
    Let {
        /// Recursive binding identities published before checking group bodies.
        /// Annotated declarations also supply their final contexts immediately.
        declarations: &'a [Definition<'a>],
        /// Assumed while checking the definition body, over its rigid variables.
        given: &'a [Pred<'a>],
        /// Evidence owner; the first untyped member for a recursive group.
        binder: Option<Binder<'a>>,
        /// All definitions generalized here, even when no lexical name is bound.
        definitions: &'a [Definition<'a>],
        rigid_vars: &'a [Variable],
        flex_vars: &'a [Variable],
        /// Name-sorted, mirroring Elm's `Map.Map Name (A.Located Type)`.
        header: &'a [(&'a str, Located<&'a Type<'a>>)],
        header_con: &'a Constraint<'a>,
        body_con: &'a Constraint<'a>,
    },
}

/// Elm's `exists`: a `CLet` binding only flex variables.
pub fn exists<'a>(
    bump: &'a bumpalo::Bump,
    flex_vars: &'a [Variable],
    constraint: Constraint<'a>,
) -> Constraint<'a> {
    Constraint::Let {
        declarations: &[],
        given: &[],
        binder: None,
        definitions: &[],
        rigid_vars: &[],
        flex_vars,
        header: &[],
        header_con: bump.alloc(constraint),
        body_con: bump.alloc(Constraint::True),
    }
}

// TYPE PRIMITIVES

/// Elm's `Type.FlatType`. Lives inside descriptors owned by the union-find
/// store (real heap, so owned containers are fine here).
#[derive(Clone, Debug)]
pub enum FlatType<'a> {
    App1(ModuleName<'a>, &'a str, Vec<Variable>),
    AppV1(Variable, Vec<Variable>),
    Fun1(Variable, Variable),
    EmptyRecord1,
    Record1(BTreeMap<&'a str, Variable>, Variable),
    Unit1,
    Tuple1(Variable, Variable, Vec<Variable>),
}

/// Elm's `Type.Type`: the language the constraint generator writes types in.
#[derive(Clone, Copy, Debug)]
pub enum Type<'a> {
    PartialAliasN {
        home: ModuleName<'a>,
        name: &'a str,
        args: &'a [(&'a str, &'a Type<'a>)],
        remaining: &'a [&'a str],
        body: &'a Located<nash_ast::Type<'a>>,
    },
    AppVarN(&'a Type<'a>, &'a [&'a Type<'a>]),
    AliasN {
        home: ModuleName<'a>,
        name: &'a str,
        args: &'a [(&'a str, &'a Type<'a>)],
        real: &'a Type<'a>,
        body: &'a Located<nash_ast::Type<'a>>,
    },
    VarN(Variable),
    AppN {
        home: ModuleName<'a>,
        name: &'a str,
        args: &'a [&'a Type<'a>],
    },
    FunN(&'a Type<'a>, &'a Type<'a>),
    EmptyRecordN,
    /// Name-sorted, mirroring Elm's `Map.Map Name Type`.
    RecordN {
        fields: &'a [(&'a str, &'a Type<'a>)],
        ext: &'a Type<'a>,
    },
    UnitN,
    TupleN(&'a Type<'a>, &'a Type<'a>, &'a [&'a Type<'a>]),
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

// BUILT-IN MODULES
//
// List uses the canonical nash/core Builtin identity. Other primitive
// representations are handled by the representation plan.

pub const fn list_home<'a>() -> ModuleName<'a> {
    nash_ast::primitives::builtin_home()
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
pub fn literal_default(trait_: nash_ast::QualifiedName<'_>) -> Option<Type<'static>> {
    if trait_.home.package != Some(nash_ast::primitives::CORE) || trait_.home.name != "Literal" {
        return None;
    }
    let name = match trait_.name {
        "FromInt" => "int",
        "FromString" => "string",
        "FromBytes" => "bytes",
        _ => return None,
    };
    Some(Type::AppN {
        home: nash_ast::primitives::builtin_home(),
        name,
        args: &[],
    })
}

pub const fn bool<'a>() -> Type<'a> {
    Type::AppN {
        home: nash_ast::primitives::builtin_home(),
        name: "bool",
        args: &[],
    }
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
