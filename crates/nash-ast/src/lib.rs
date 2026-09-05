mod evidence;
pub mod primitives;

use nash_region::{Located, Region};

pub use nash_source::{Associativity, Docs, ModuleKind, Precedence};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaseKind {
    Big,
    Const,
    Term,
}

/// The shapes a kind variable may take. Bit set over `BaseKind` plus arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KindSet(u8);

impl KindSet {
    pub const BIG: KindSet = KindSet(0b0001);
    pub const CONST: KindSet = KindSet(0b0010);
    pub const TERM: KindSet = KindSet(0b0100);
    pub const ARROW: KindSet = KindSet(0b1000);
    pub const ANY: KindSet = KindSet(0b0111);
    pub const ALL: KindSet = KindSet(0b1111);
    pub const STORABLE: KindSet = KindSet(0b0011);
    pub const LITTLE: KindSet = KindSet(0b0110);

    pub fn of(base: BaseKind) -> KindSet {
        match base {
            BaseKind::Big => KindSet::BIG,
            BaseKind::Const => KindSet::CONST,
            BaseKind::Term => KindSet::TERM,
        }
    }

    pub fn contains(self, other: KindSet) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn intersect(self, other: KindSet) -> KindSet {
        KindSet(self.0 & other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A kind after inference. `Var` indexes the enclosing `KindScheme`.
#[derive(Debug)]
pub enum Kind<'a> {
    Base(BaseKind),
    Var(u16),
    Arrow(&'a Kind<'a>, &'a Kind<'a>),
}

/// `forall k0 .. kn. kind`, with one bound per variable.
#[derive(Clone, Copy, Debug)]
pub struct KindScheme<'a> {
    pub bounds: &'a [KindSet],
    pub kind: &'a Kind<'a>,
}

impl<'a> KindScheme<'a> {
    pub fn mono(kind: &'a Kind<'a>) -> KindScheme<'a> {
        KindScheme { bounds: &[], kind }
    }

    /// The result kind after all parameters are applied.
    pub fn result(&self) -> &'a Kind<'a> {
        let mut kind = self.kind;
        while let Kind::Arrow(_, to) = kind {
            kind = to;
        }
        kind
    }
}

pub type FreeVars<'a> = &'a [&'a str];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName<'a> {
    pub author: &'a str,
    pub project: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleName<'a> {
    pub package: Option<PackageName<'a>>,
    pub name: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName<'a> {
    pub home: ModuleName<'a>,
    pub name: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConstructorName<'a> {
    pub home: ModuleName<'a>,
    pub union: &'a str,
    pub name: &'a str,
}

#[derive(Debug)]
pub struct Module<'a> {
    pub traits: &'a [&'a Located<Trait<'a>>],
    pub impls: &'a [&'a Located<Impl<'a>>],
    pub kind: ModuleKind,
    pub name: ModuleName<'a>,
    pub exports: Exports<'a>,
    pub docs: &'a Docs<'a>,
    pub decls: &'a Decls<'a>,
    pub unions: &'a [&'a Located<Union<'a>>],
    pub aliases: &'a [&'a Located<Alias<'a>>],
    pub binops: &'a [&'a Located<Binop<'a>>],
}

#[derive(Debug)]
pub enum Decls<'a> {
    Declare {
        definition: &'a Def<'a>,
        next: &'a Decls<'a>,
    },
    DeclareRec {
        definition: &'a Def<'a>,
        following: &'a [&'a Def<'a>],
        next: &'a Decls<'a>,
    },
    Empty,
}

#[derive(Debug)]
pub enum Def<'a> {
    Def {
        name: &'a Located<&'a str>,
        args: &'a [&'a Located<Pattern<'a>>],
        body: &'a Located<Expr<'a>>,
    },
    TypedDef {
        context: &'a [Pred<'a>],
        /// The original annotation, before aliases and function arguments are split.
        annotation: &'a Located<Type<'a>>,
        name: &'a Located<&'a str>,
        free_vars: FreeVars<'a>,
        args: &'a [TypedPattern<'a>],
        body: &'a Located<Expr<'a>>,
        typ: &'a Located<Type<'a>>,
    },
}

#[derive(Debug)]
pub struct TypedPattern<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Debug)]
pub struct Union<'a> {
    pub kind: KindScheme<'a>,
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub ctors: &'a [&'a Ctor<'a>],
    pub alternatives: u16,
    pub options: CtorOpts,
}

#[derive(Debug)]
pub struct Ctor<'a> {
    pub name: &'a str,
    pub index: u16,
    pub arity: u16,
    pub arguments: &'a [&'a Located<Type<'a>>],
}

#[derive(Debug)]
pub struct Alias<'a> {
    pub kind: KindScheme<'a>,
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Debug)]
pub struct Binop<'a> {
    pub symbol: &'a str,
    pub associativity: Associativity,
    pub precedence: Precedence,
    pub function: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CtorOpts {
    Normal,
    Enum,
    Unbox,
}

#[derive(Debug)]
pub enum Expr<'a> {
    VarMethod {
        trait_: QualifiedName<'a>,
        method: &'a str,
        annotation: &'a Annotation<'a>,
    },
    VarLocal(&'a str),
    VarTopLevel(QualifiedName<'a>),
    /// Mirrors Elm's `Can.VarForeign home name annotation`. The annotation
    /// comes from the defining module's interface, which is only produced
    /// after that module has been type-solved.
    VarForeign {
        reference: QualifiedName<'a>,
        annotation: &'a Annotation<'a>,
    },
    VarConstructor {
        options: CtorOpts,
        reference: ConstructorName<'a>,
        index: u16,
        annotation: &'a Annotation<'a>,
    },
    /// Mirrors Elm's `Can.VarOperator op home name annotation`.
    VarOperator {
        symbol: &'a str,
        reference: QualifiedName<'a>,
        annotation: &'a Annotation<'a>,
    },
    Str(&'a str),
    Int(i128),
    List(&'a [&'a Located<Expr<'a>>]),
    Negate(&'a Located<Expr<'a>>),
    /// Mirrors Elm's `Can.Binop op home name annotation left right`.
    Binop {
        symbol: &'a str,
        reference: QualifiedName<'a>,
        annotation: &'a Annotation<'a>,
        left: &'a Located<Expr<'a>>,
        right: &'a Located<Expr<'a>>,
    },
    Lambda {
        parameters: &'a [&'a Located<Pattern<'a>>],
        body: &'a Located<Expr<'a>>,
    },
    Call {
        function: &'a Located<Expr<'a>>,
        arguments: &'a [&'a Located<Expr<'a>>],
    },
    If {
        branches: &'a [IfBranch<'a>],
        final_else: &'a Located<Expr<'a>>,
    },
    Let {
        definition: &'a Def<'a>,
        body: &'a Located<Expr<'a>>,
    },
    LetRec {
        definitions: &'a [&'a Def<'a>],
        body: &'a Located<Expr<'a>>,
    },
    LetDestruct {
        pattern: &'a Located<Pattern<'a>>,
        value: &'a Located<Expr<'a>>,
        body: &'a Located<Expr<'a>>,
    },
    Case {
        scrutinee: &'a Located<Expr<'a>>,
        branches: &'a [CaseBranch<'a>],
    },
    Accessor(&'a str),
    Access {
        record: &'a Located<Expr<'a>>,
        field: &'a Located<&'a str>,
    },
    Update {
        record: &'a str,
        base: &'a Located<Expr<'a>>,
        fields: &'a [FieldUpdate<'a>],
    },
    Record(&'a [FieldValue<'a>]),
    Unit,
    Tuple {
        first: &'a Located<Expr<'a>>,
        second: &'a Located<Expr<'a>>,
        rest: &'a [&'a Located<Expr<'a>>],
    },
}

#[derive(Debug)]
pub struct IfBranch<'a> {
    pub condition: &'a Located<Expr<'a>>,
    pub then_branch: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub struct CaseBranch<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub body: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub struct FieldUpdate<'a> {
    pub field: &'a Located<&'a str>,
    pub value: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub struct FieldValue<'a> {
    pub field: &'a Located<&'a str>,
    pub value: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub enum Pattern<'a> {
    Anything,
    Var(&'a str),
    Record(&'a [&'a str]),
    Alias {
        pattern: &'a Located<Pattern<'a>>,
        name: &'a str,
    },
    Unit,
    Tuple {
        first: &'a Located<Pattern<'a>>,
        second: &'a Located<Pattern<'a>>,
        rest: &'a [&'a Located<Pattern<'a>>],
    },
    List(&'a [&'a Located<Pattern<'a>>]),
    Cons {
        head: &'a Located<Pattern<'a>>,
        tail: &'a Located<Pattern<'a>>,
    },
    Constructor(PatternCtor<'a>),
    Bool {
        union: &'a Union<'a>,
        value: bool,
    },
    Str(&'a str),
    Int(i128),
}

#[derive(Debug)]
pub struct PatternCtor<'a> {
    pub reference: ConstructorName<'a>,
    pub union: &'a Union<'a>,
    pub index: u16,
    pub arguments: &'a [PatternCtorArg<'a>],
    pub options: CtorOpts,
    pub alternatives: u16,
}

#[derive(Debug)]
pub struct PatternCtorArg<'a> {
    pub index: u16,
    pub typ: &'a Located<Type<'a>>,
    pub pattern: &'a Located<Pattern<'a>>,
}

#[derive(Debug)]
pub struct Annotation<'a> {
    /// Scheme context, in evidence order.
    pub context: &'a [Pred<'a>],
    pub free_vars: FreeVars<'a>,
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Type<'a> {
    Lambda {
        from: &'a Located<Type<'a>>,
        to: &'a Located<Type<'a>>,
    },
    Var(&'a str),
    /// Application with a substitutable head, including higher-kinded variables.
    App {
        head: &'a Located<Type<'a>>,
        args: &'a [&'a Located<Type<'a>>],
    },
    Named {
        reference: QualifiedName<'a>,
        args: &'a [&'a Located<Type<'a>>],
    },
    Record {
        fields: &'a [FieldType<'a>],
        ext: Option<&'a str>,
    },
    Unit,
    Tuple {
        first: &'a Located<Type<'a>>,
        second: &'a Located<Type<'a>>,
        rest: &'a [&'a Located<Type<'a>>],
    },
    Alias {
        reference: QualifiedName<'a>,
        arguments: &'a [AliasArgument<'a>],
        target: AliasType<'a>,
    },
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum AliasType<'a> {
    Open(&'a Located<Type<'a>>),
    Filled(&'a Located<Type<'a>>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct AliasArgument<'a> {
    pub name: &'a str,
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldType<'a> {
    pub index: u16,
    pub field: &'a str,
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Debug)]
pub enum Exports<'a> {
    Everything(Region),
    Explicit(&'a [&'a Located<Export<'a>>]),
}

#[derive(Debug)]
pub enum Export<'a> {
    Trait(&'a str),
    Value(&'a str),
    Binop(&'a str),
    Alias(&'a str),
    UnionOpen(&'a str),
    UnionClosed(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);
impl NodeId {
    pub fn expr(e: &Located<Expr<'_>>) -> Self {
        NodeId(e as *const _ as usize)
    }
    pub fn pattern(p: &Located<Pattern<'_>>) -> Self {
        NodeId(p as *const _ as usize)
    }
    /// A definition, addressed by its name node (`Def::{Def, TypedDef}.name`).
    pub fn def(name: &Located<&str>) -> Self {
        NodeId(name as *const _ as usize)
    }
}

/// `Tr t1 .. tn`: the claim that `t1..tn` have an impl of `Tr`.
#[derive(Debug)]
pub struct Pred<'a> {
    pub trait_: QualifiedName<'a>,
    pub args: &'a [&'a Located<Type<'a>>],
}

#[derive(Debug)]
pub struct Trait<'a> {
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    /// One kind per parameter, generalized together (plans/02 `KindScheme`).
    pub kind: KindScheme<'a>,
    /// Superclasses; args are `Type::Var` over `parameters`.
    pub supers: &'a [Pred<'a>],
    pub methods: &'a [Method<'a>],
}

#[derive(Debug)]
pub struct Method<'a> {
    pub name: &'a Located<&'a str>,
    /// Full method scheme: trait predicate first, then the method's own context.
    pub annotation: &'a Annotation<'a>,
    /// Default body as a `TypedDef` whose annotation is `annotation`.
    pub default: Option<&'a Def<'a>>,
}

/// One instance head: a constructor over distinct variables, or unit/tuple.
#[derive(Debug)]
pub enum Head<'a> {
    Named {
        reference: QualifiedName<'a>,
        vars: &'a [&'a str],
    },
    Unit,
    Tuple(&'a [&'a str]),
}

#[derive(Debug)]
pub struct Impl<'a> {
    pub trait_: QualifiedName<'a>,
    /// Over the head variables only.
    pub context: &'a [Pred<'a>],
    pub heads: &'a [Located<Head<'a>>],
    /// Each a `TypedDef` whose annotation is the method scheme at the heads.
    pub methods: &'a [&'a Def<'a>],
}

/// The constructor of a head, for impl lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadCon<'a> {
    Named(QualifiedName<'a>),
    Unit,
    Tuple(u8),
    /// Function types never have impls; a wanted `Show (a -> b)` fails lookup.
    Fun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImplKey<'a> {
    pub trait_: QualifiedName<'a>,
    pub heads: &'a [HeadCon<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImplRef<'a> {
    pub home: ModuleName<'a>,
    pub key: ImplKey<'a>,
}

/// How one wanted predicate was satisfied. Consumed by codegen
/// (plans/07 chunk 9 keys specializations by ground evidence, hence `Hash`).
#[derive(Debug)]
pub enum Evidence<'a> {
    Impl {
        impl_: ImplRef<'a>,
        /// The impl head's variables, in head order, at this use.
        type_args: &'a [&'a Located<Type<'a>>],
        /// One per predicate of the impl's context, in order.
        args: &'a [Evidence<'a>],
    },
    /// The `index`-th context predicate of the definition `binder` (a `NodeId::def`).
    Given { binder: NodeId, index: u16 },
    /// The `index`-th superclass of `of`'s trait.
    Super { of: &'a Evidence<'a>, index: u16 },
}

impl<'a> Head<'a> {
    pub fn con(&self) -> HeadCon<'a> {
        match self {
            Head::Named { reference, .. } => HeadCon::Named(*reference),
            Head::Unit => HeadCon::Unit,
            Head::Tuple(vars) => HeadCon::Tuple(vars.len() as u8),
        }
    }
}

#[cfg(test)]
mod kind_tests {
    use super::{BaseKind, Kind, KindScheme, KindSet};

    #[test]
    fn bounds_allow_only_documented_shapes() {
        let shapes = [
            KindSet::of(BaseKind::Big),
            KindSet::of(BaseKind::Const),
            KindSet::of(BaseKind::Term),
            KindSet::ARROW,
        ];
        for (bound, allowed) in [
            (KindSet::BIG, [true, false, false, false]),
            (KindSet::CONST, [false, true, false, false]),
            (KindSet::TERM, [false, false, true, false]),
            (KindSet::ARROW, [false, false, false, true]),
            (KindSet::ANY, [true, true, true, false]),
            (KindSet::ALL, [true, true, true, true]),
            (KindSet::STORABLE, [true, true, false, false]),
            (KindSet::LITTLE, [false, true, true, false]),
        ] {
            for (shape, expected) in shapes.into_iter().zip(allowed) {
                assert_eq!(
                    bound.contains(shape),
                    expected,
                    "{bound:?} contains {shape:?}"
                );
            }
        }
        assert!(KindSet::ALL.contains(KindSet::ANY));
        assert!(!KindSet::STORABLE.contains(KindSet::LITTLE));
    }

    #[test]
    fn intersection_refines_bounds() {
        assert_eq!(KindSet::STORABLE.intersect(KindSet::LITTLE), KindSet::CONST);
        assert_eq!(KindSet::LITTLE.intersect(KindSet::STORABLE), KindSet::CONST);
        assert_eq!(KindSet::ALL.intersect(KindSet::ANY), KindSet::ANY);
        assert!(KindSet::BIG.intersect(KindSet::LITTLE).is_empty());
        assert!(KindSet::ANY.intersect(KindSet::ARROW).is_empty());
        assert!(!KindSet::CONST.is_empty());
    }

    #[test]
    fn scheme_result_follows_only_result_arrows() {
        let big = Kind::Base(BaseKind::Big);
        let arrow = Kind::Arrow(&big, &big);
        let nested = Kind::Arrow(&big, &arrow);
        let scheme = KindScheme::mono(&nested);
        assert!(scheme.bounds.is_empty());
        assert!(std::ptr::eq(scheme.result(), &big));
        let var = Kind::Var(0);
        let higher = Kind::Arrow(&arrow, &var);
        let scheme = KindScheme {
            bounds: &[KindSet::LITTLE],
            kind: &higher,
        };
        assert!(std::ptr::eq(scheme.result(), &var));
        assert!(std::ptr::eq(KindScheme::mono(&big).result(), &big));
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn impl_evidence_shares_types_across_source_locations() {
        let home = ModuleName {
            package: None,
            name: "Example",
        };
        let reference = QualifiedName {
            home,
            name: "Value",
        };
        let first_element = Located::at_zero(Type::Unit);
        let second_element = Located::at(Region::one(), Type::Unit);
        let first_elements = [&first_element];
        let second_elements = [&second_element];
        let first = Located::at_zero(Type::Named {
            reference,
            args: &first_elements,
        });
        let second = Located::at(
            Region::one(),
            Type::Named {
                reference,
                args: &second_elements,
            },
        );
        let other = Located::at_zero(Type::Named {
            reference: QualifiedName {
                home,
                name: "Other",
            },
            args: &[],
        });
        let impl_ = ImplRef {
            home,
            key: ImplKey {
                trait_: QualifiedName { home, name: "Show" },
                heads: &[],
            },
        };
        let first_args = [&first];
        let second_args = [&second];
        let other_args = [&other];
        let a = Evidence::Impl {
            impl_,
            type_args: &first_args,
            args: &[],
        };
        let b = Evidence::Impl {
            impl_,
            type_args: &second_args,
            args: &[],
        };
        let c = Evidence::Impl {
            impl_,
            type_args: &other_args,
            args: &[],
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
        let keys = HashSet::from([a, b, c]);
        assert_eq!(keys.len(), 2);
    }
}
