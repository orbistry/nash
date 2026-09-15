//! Compiler-owned types and representation predicates of `nash/core.Builtin`.

mod builtins;
pub use builtins::{BUILTINS, Builtin, BuiltinLowering};

use crate::{Kind, ModuleName, PackageName, QualifiedName};

pub const CORE: PackageName<'static> = PackageName {
    author: "nash",
    project: "core",
    version: "",
    source: crate::PackageSource::Compiler,
    compilation: None,
};
pub const fn builtin_home() -> ModuleName<'static> {
    ModuleName {
        package: Some(CORE),
        name: "Builtin",
    }
}
pub const fn literal_home() -> ModuleName<'static> {
    ModuleName {
        package: Some(CORE),
        name: "Literal",
    }
}
const fn core_trait(module: &'static str, name: &'static str) -> QualifiedName<'static> {
    QualifiedName {
        home: ModuleName {
            package: Some(CORE),
            name: module,
        },
        name,
    }
}
pub const fn eq_trait() -> QualifiedName<'static> {
    core_trait("Eq", "Eq")
}
pub const fn lift_trait() -> QualifiedName<'static> {
    core_trait("Lift", "Lift")
}
pub const fn monad_trait() -> QualifiedName<'static> {
    core_trait("Monad", "Monad")
}
pub const fn num_trait() -> QualifiedName<'static> {
    core_trait("Num", "Num")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Repr {
    Big,
    Const,
    Term,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReprTrait {
    Big,
    Const,
    Term,
    Storable,
    Little,
}

impl ReprTrait {
    pub const ALL: [Self; 5] = [
        Self::Big,
        Self::Const,
        Self::Term,
        Self::Storable,
        Self::Little,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Big => "Big",
            Self::Const => "Const",
            Self::Term => "Term",
            Self::Storable => "Storable",
            Self::Little => "Little",
        }
    }
    pub const fn qualified(self) -> QualifiedName<'static> {
        QualifiedName {
            home: builtin_home(),
            name: self.name(),
        }
    }
    pub fn of(name: QualifiedName<'_>) -> Option<Self> {
        if name.home != builtin_home() {
            return None;
        }
        Self::ALL.into_iter().find(|repr| repr.name() == name.name)
    }
    pub const fn admits(self) -> ReprSet {
        ReprSet(match self {
            Self::Big => 1,
            Self::Const => 2,
            Self::Term => 4,
            Self::Storable => 3,
            Self::Little => 6,
        })
    }
    pub const fn supers(self) -> &'static [Self] {
        match self {
            Self::Big => &[Self::Storable],
            Self::Const => &[Self::Storable, Self::Little],
            Self::Term => &[Self::Little],
            Self::Storable | Self::Little => &[],
        }
    }
}

/// Used only for representation contradictions, never for kind unification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReprSet(u8);
impl ReprSet {
    pub const ALL: Self = Self(7);
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn contains(self, repr: Repr) -> bool {
        self.0
            & match repr {
                Repr::Big => 1,
                Repr::Const => 2,
                Repr::Term => 4,
            }
            != 0
    }
}

#[derive(Debug)]
pub struct Primitive {
    pub name: &'static str,
    pub kind: &'static Kind<'static>,
    pub repr: Repr,
    /// Requirements on formal parameters, in declaration order.
    pub context: &'static [(usize, ReprTrait)],
    pub ctors: &'static [&'static crate::Ctor<'static>],
}

const TYPE: &Kind<'static> = &Kind::Type;
const UNARY: &Kind<'static> = &Kind::Arrow(TYPE, TYPE);
const BINARY: &Kind<'static> = &Kind::Arrow(TYPE, UNARY);
const BOOL_CTORS: &[&crate::Ctor<'static>] = &[
    &crate::Ctor {
        labels: None,
        name: "False",
        index: 0,
        arity: 0,
        arguments: &[],
    },
    &crate::Ctor {
        labels: None,
        name: "True",
        index: 1,
        arity: 0,
        arguments: &[],
    },
];
const DATA_OPTION_CTORS: &[&crate::Ctor<'static>] = &[
    &crate::Ctor {
        labels: None,
        name: "Some",
        index: 0,
        arity: 1,
        arguments: &[&nash_region::Located::at_zero(crate::Type::Var("p0"))],
    },
    &crate::Ctor {
        labels: None,
        name: "None",
        index: 1,
        arity: 0,
        arguments: &[],
    },
];
/// Shared canonical layout for optional external Data, including specialization.
pub static DATA_OPTION_UNION: crate::Union<'static> = crate::Union {
    kind: UNARY,
    context: &[],
    name: &nash_region::Located::at_zero("data_option"),
    parameters: &["p0"],
    ctors: DATA_OPTION_CTORS,
    alternatives: 2,
    options: crate::CtorOpts::Normal,
    data_layout: Some(crate::DataLayout {
        encoding: crate::DataEncoding::Constr,
        opaque: false,
        tags: &[0, 1],
    }),
};

macro_rules! data_ctor {
    ($name:literal, $index:literal, $labels:expr, [$($arg:expr),*]) => {
        &crate::Ctor { name: $name, index: $index, labels: $labels,
            arity: (&[$($arg),*] as &[&nash_region::Located<crate::Type<'static>>]).len() as u16,
            arguments: &[$($arg),*] }
    };
}

const ORDERING_CTORS: &[&crate::Ctor<'static>] = &[
    data_ctor!("Less", 0, None, []),
    data_ctor!("Equal", 1, None, []),
    data_ctor!("Greater", 2, None, []),
];
const NEVER_CTORS: &[&crate::Ctor<'static>] = &[data_ctor!("Never", 0, None, [])];
const PRNG_CTORS: &[&crate::Ctor<'static>] = &[
    data_ctor!(
        "Seeded",
        0,
        Some(&["seed", "choices"]),
        [
            &builtins::named("bytes", &[]),
            &builtins::named("bytes", &[])
        ]
    ),
    data_ctor!(
        "Replayed",
        1,
        Some(&["cursor", "choices"]),
        [&builtins::named("int", &[]), &builtins::named("bytes", &[])]
    ),
];
const SCRIPT_PURPOSE_CTORS: &[&crate::Ctor<'static>] = &[
    data_ctor!("__Mint", 0, None, [DATA]),
    data_ctor!(
        "__Spend",
        1,
        None,
        [DATA, &builtins::named("data_option", &[DATA])]
    ),
    data_ctor!("__Withdraw", 2, None, [DATA]),
    data_ctor!("__Publish", 3, None, [&builtins::named("int", &[]), DATA]),
    data_ctor!("__Vote", 4, None, [DATA]),
    data_ctor!("__Propose", 5, None, [&builtins::named("int", &[]), DATA]),
];
const SCRIPT_CONTEXT_CTORS: &[&crate::Ctor<'static>] = &[data_ctor!(
    "__ScriptContext",
    0,
    None,
    [DATA, DATA, &builtins::named("data_script_purpose", &[])]
)];

macro_rules! data_union {
    ($name:literal, $ctors:ident, $tags:expr) => {
        crate::Union {
            kind: TYPE,
            context: &[],
            name: &nash_region::Located::at_zero($name),
            parameters: &[],
            ctors: $ctors,
            alternatives: $ctors.len() as u16,
            options: crate::CtorOpts::Normal,
            data_layout: Some(crate::DataLayout {
                encoding: crate::DataEncoding::Constr,
                tags: $tags,
                opaque: false,
            }),
        }
    };
}

static ORDERING_UNION: crate::Union<'static> =
    data_union!("data_ordering", ORDERING_CTORS, &[0, 1, 2]);
static NEVER_UNION: crate::Union<'static> = data_union!("data_never", NEVER_CTORS, &[1]);
static PRNG_UNION: crate::Union<'static> = data_union!("data_prng", PRNG_CTORS, &[0, 1]);
static SCRIPT_PURPOSE_UNION: crate::Union<'static> = data_union!(
    "data_script_purpose",
    SCRIPT_PURPOSE_CTORS,
    &[0, 1, 2, 3, 4, 5]
);
static SCRIPT_CONTEXT_UNION: crate::Union<'static> =
    data_union!("data_script_context", SCRIPT_CONTEXT_CTORS, &[0]);

/// Compiler-owned nominal types with an external constructor-Data layout.
pub fn data_union(name: &str) -> Option<&'static crate::Union<'static>> {
    Some(match name {
        "data_option" => &DATA_OPTION_UNION,
        "data_ordering" => &ORDERING_UNION,
        "data_never" => &NEVER_UNION,
        "data_prng" => &PRNG_UNION,
        "data_script_purpose" => &SCRIPT_PURPOSE_UNION,
        "data_script_context" => &SCRIPT_CONTEXT_UNION,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralConstructor {
    Pair,
    Unit,
}

pub fn structural_constructor(type_name: &str, ctor_name: &str) -> Option<StructuralConstructor> {
    match (type_name, ctor_name) {
        ("data_pair", "Pair") => Some(StructuralConstructor::Pair),
        ("unit", "Void") => Some(StructuralConstructor::Unit),
        _ => None,
    }
}
const DATA: &nash_region::Located<crate::Type<'static>> = &builtins::named("Data", &[]);
const DATA_LIST: &nash_region::Located<crate::Type<'static>> = &builtins::named("list", &[DATA]);
const DATA_CTORS: &[&crate::Ctor<'static>] = &[
    &crate::Ctor {
        labels: None,
        name: "Constr",
        index: 0,
        arity: 2,
        arguments: &[&builtins::named("int", &[]), DATA_LIST],
    },
    &crate::Ctor {
        labels: None,
        name: "Map",
        index: 1,
        arity: 1,
        arguments: &[&builtins::named(
            "list",
            &[&builtins::named("pair", &[DATA, DATA])],
        )],
    },
    &crate::Ctor {
        labels: None,
        name: "List",
        index: 2,
        arity: 1,
        arguments: &[DATA_LIST],
    },
    &crate::Ctor {
        labels: None,
        name: "I",
        index: 3,
        arity: 1,
        arguments: &[&builtins::named("int", &[])],
    },
    &crate::Ctor {
        labels: None,
        name: "B",
        index: 4,
        arity: 1,
        arguments: &[&builtins::named("bytes", &[])],
    },
];

macro_rules! primitive {
    ($name:literal, $kind:ident, $repr:ident, $context:expr, $ctors:expr) => {
        Primitive {
            name: $name,
            kind: $kind,
            repr: Repr::$repr,
            context: $context,
            ctors: $ctors,
        }
    };
}
pub const PRIMITIVES: &[Primitive] = &[
    primitive!("Data", TYPE, Big, &[], DATA_CTORS),
    primitive!("Int", TYPE, Big, &[], &[]),
    primitive!("Bytes", TYPE, Big, &[], &[]),
    primitive!("List", UNARY, Big, &[(0, ReprTrait::Big)], &[]),
    primitive!(
        "Map",
        BINARY,
        Big,
        &[(0, ReprTrait::Big), (1, ReprTrait::Big)],
        &[]
    ),
    primitive!("int", TYPE, Const, &[], &[]),
    primitive!("bytes", TYPE, Const, &[], &[]),
    primitive!("string", TYPE, Const, &[], &[]),
    primitive!("bool", TYPE, Const, &[], BOOL_CTORS),
    primitive!("unit", TYPE, Const, &[], &[]),
    primitive!("bls_g1", TYPE, Const, &[], &[]),
    primitive!("bls_g2", TYPE, Const, &[], &[]),
    primitive!("bls_mlr", TYPE, Const, &[], &[]),
    primitive!("value", TYPE, Const, &[], &[]),
    primitive!("list", UNARY, Const, &[(0, ReprTrait::Storable)], &[]),
    primitive!("array", UNARY, Const, &[(0, ReprTrait::Storable)], &[]),
    // Unlike native pair, both runtime fields are encoded Data.
    primitive!("data_pair", BINARY, Const, &[], &[]),
    primitive!("data_list", UNARY, Const, &[], &[]),
    primitive!("data_tuple", UNARY, Const, &[], &[]),
    primitive!("data_option", UNARY, Big, &[], DATA_OPTION_CTORS),
    primitive!("data_ordering", TYPE, Big, &[], ORDERING_CTORS),
    primitive!("data_never", TYPE, Big, &[], NEVER_CTORS),
    primitive!("data_prng", TYPE, Big, &[], PRNG_CTORS),
    primitive!("data_script_purpose", TYPE, Big, &[], SCRIPT_PURPOSE_CTORS),
    primitive!("data_script_context", TYPE, Big, &[], SCRIPT_CONTEXT_CTORS),
    primitive!(
        "pair",
        BINARY,
        Const,
        &[(0, ReprTrait::Storable), (1, ReprTrait::Storable)],
        &[]
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primitive_inventory_is_complete_and_well_kinded() {
        let mut names = std::collections::BTreeSet::new();
        for primitive in PRIMITIVES {
            assert!(names.insert(primitive.name));
            assert!(
                primitive
                    .context
                    .iter()
                    .all(|(index, _)| *index < primitive.kind.arity())
            );
        }
        assert_eq!(
            DATA_CTORS.iter().map(|ctor| ctor.arity).collect::<Vec<_>>(),
            [2, 1, 1, 1, 1]
        );
    }
}
