//! Compiler-known types of the `nash/core` `Builtin` module.

use crate::{BaseKind, Kind, KindScheme, KindSet, ModuleName, PackageName};

pub const CORE: PackageName<'static> = PackageName {
    author: "nash",
    project: "core",
};

pub const fn builtin_home() -> ModuleName<'static> {
    ModuleName {
        package: Some(CORE),
        name: "Builtin",
    }
}

pub const fn lift_trait() -> crate::QualifiedName<'static> {
    crate::QualifiedName {
        home: ModuleName {
            package: Some(CORE),
            name: "Lift",
        },
        name: "Lift",
    }
}

pub const fn monad_trait() -> crate::QualifiedName<'static> {
    crate::QualifiedName {
        home: ModuleName {
            package: Some(CORE),
            name: "Monad",
        },
        name: "Monad",
    }
}

const BIG: &Kind<'static> = &Kind::Base(BaseKind::Big);
const CONST: &Kind<'static> = &Kind::Base(BaseKind::Const);
const K0: &Kind<'static> = &Kind::Var(0);
const K1: &Kind<'static> = &Kind::Var(1);

const BIG_TO_BIG: &Kind<'static> = &Kind::Arrow(BIG, BIG);
const BIG2_TO_BIG: &Kind<'static> = &Kind::Arrow(BIG, BIG_TO_BIG);
const STORABLE_TO_CONST: &Kind<'static> = &Kind::Arrow(K0, CONST);
// Only `mkPairData` builds pairs, so construction is restricted by the API,
// not by kinds: `unConstrData` yields `pair int (list Data)`.
const STORABLE2_TO_CONST: &Kind<'static> = &Kind::Arrow(K0, &Kind::Arrow(K1, CONST));

pub struct Primitive {
    pub name: &'static str,
    pub arity: usize,
    pub kind: KindScheme<'static>,
}

const fn mono(kind: &'static Kind<'static>) -> KindScheme<'static> {
    KindScheme { bounds: &[], kind }
}

pub const PRIMITIVES: &[Primitive] = &[
    Primitive {
        name: "Data",
        arity: 0,
        kind: mono(BIG),
    },
    Primitive {
        name: "Int",
        arity: 0,
        kind: mono(BIG),
    },
    Primitive {
        name: "Bytes",
        arity: 0,
        kind: mono(BIG),
    },
    Primitive {
        name: "List",
        arity: 1,
        kind: mono(BIG_TO_BIG),
    },
    Primitive {
        name: "Map",
        arity: 2,
        kind: mono(BIG2_TO_BIG),
    },
    Primitive {
        name: "int",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "bytes",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "string",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "bool",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "unit",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "bls_g1",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "bls_g2",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "bls_mlr",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "value",
        arity: 0,
        kind: mono(CONST),
    },
    Primitive {
        name: "list",
        arity: 1,
        kind: KindScheme {
            bounds: &[KindSet::STORABLE],
            kind: STORABLE_TO_CONST,
        },
    },
    Primitive {
        name: "array",
        arity: 1,
        kind: KindScheme {
            bounds: &[KindSet::STORABLE],
            kind: STORABLE_TO_CONST,
        },
    },
    Primitive {
        name: "pair",
        arity: 2,
        kind: KindScheme {
            bounds: &[KindSet::STORABLE, KindSet::STORABLE],
            kind: STORABLE2_TO_CONST,
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_have_declared_arity_and_unique_names() {
        let mut names = std::collections::BTreeSet::new();
        for primitive in PRIMITIVES {
            assert!(names.insert(primitive.name));
            let mut kind = primitive.kind.kind;
            let mut arity = 0;
            while let Kind::Arrow(_, result) = kind {
                arity += 1;
                kind = result;
            }
            assert_eq!(arity, primitive.arity, "{}", primitive.name);
        }
        assert_eq!(names.len(), 17);
        assert_eq!(builtin_home().package, Some(CORE));
        assert_eq!(builtin_home().name, "Builtin");
    }

    #[test]
    fn containers_enforce_runtime_element_shapes() {
        let get = |name| PRIMITIVES.iter().find(|p| p.name == name).unwrap().kind;
        for name in ["list", "array"] {
            let scheme = get(name);
            assert_eq!(scheme.bounds, &[KindSet::STORABLE]);
            assert!(matches!(
                scheme.kind,
                Kind::Arrow(Kind::Var(0), Kind::Base(BaseKind::Const))
            ));
        }
        assert!(matches!(
            get("List").kind,
            Kind::Arrow(Kind::Base(BaseKind::Big), Kind::Base(BaseKind::Big))
        ));
        let pair = get("pair");
        assert_eq!(pair.bounds, &[KindSet::STORABLE, KindSet::STORABLE]);
        assert!(matches!(
            pair.kind,
            Kind::Arrow(
                Kind::Var(0),
                Kind::Arrow(Kind::Var(1), Kind::Base(BaseKind::Const))
            )
        ));
        for name in ["Int", "Bytes", "Data"] {
            assert!(matches!(get(name).kind, Kind::Base(BaseKind::Big)));
        }
        for name in [
            "int", "bytes", "string", "bool", "unit", "bls_g1", "bls_g2", "bls_mlr", "value",
        ] {
            assert!(matches!(get(name).kind, Kind::Base(BaseKind::Const)));
        }
    }
}
