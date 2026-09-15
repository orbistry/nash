pub mod declared;
mod evidence;
pub mod head;
pub mod primitives;

use nash_region::{Located, Region};

pub use nash_source::{
    Associativity, Constant, ConversionKind, ConversionSite, DataEncoding, DataLayout, Docs,
    ModuleKind, Precedence,
};

/// A closed Haskell 98 kind. Inference variables never escape the kind checker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind<'a> {
    Type,
    Arrow(&'a Kind<'a>, &'a Kind<'a>),
}

impl Kind<'_> {
    pub fn arity(&self) -> usize {
        let mut count = 0;
        let mut kind = self;
        while let Self::Arrow(_, result) = kind {
            count += 1;
            kind = result;
        }
        count
    }
}

pub type FreeVars<'a> = &'a [&'a str];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackageSource<'a> {
    Local(&'a [u8]),
    Github,
    Gitlab,
    Bitbucket,
    Compiler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName<'a> {
    pub author: &'a str,
    pub project: &'a str,
    pub version: &'a str,
    pub source: PackageSource<'a>,
    /// Compilation-instance fingerprint; never part of the source package name.
    pub compilation: Option<u64>,
}

impl PackageName<'_> {
    /// Native core privileges are owned by the author/project, not a release.
    pub fn is_core(self) -> bool {
        self.author == "nash" && self.project == "core"
    }
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

impl QualifiedName<'_> {
    /// Recognize a native core operation without erasing its nominal identity.
    pub fn is_core_trait(self, reference: QualifiedName<'_>) -> bool {
        self.home.package.is_some_and(PackageName::is_core)
            && reference.home.package.is_some_and(PackageName::is_core)
            && self.home.name == reference.home.name
            && self.name == reference.name
    }
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
    pub kind: &'a Kind<'a>,
    pub context: &'a [Pred<'a>],
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub ctors: &'a [&'a Ctor<'a>],
    pub alternatives: u16,
    pub options: CtorOpts,
    pub data_layout: Option<DataLayout<'a>>,
}

#[derive(Debug)]
pub struct Ctor<'a> {
    /// Labels in declaration order, parallel to arguments; None is positional.
    pub labels: Option<&'a [&'a str]>,
    pub name: &'a str,
    pub index: u16,
    pub arity: u16,
    pub arguments: &'a [&'a Located<Type<'a>>],
}

impl<'a> Ctor<'a> {
    /// Field metadata in declaration (wire) order, without duplicating types.
    pub fn labeled_fields(&self) -> Option<Vec<FieldType<'a>>> {
        self.labels.map(|labels| {
            labels
                .iter()
                .zip(self.arguments)
                .enumerate()
                .map(|(index, (field, typ))| FieldType {
                    index: index as u16,
                    field,
                    typ,
                })
                .collect()
        })
    }
}

impl<'a> Union<'a> {
    /// Only a single labeled constructor has unambiguous field projection.
    pub fn labeled_fields(&self) -> Option<Vec<FieldType<'a>>> {
        let [ctor] = self.ctors else { return None };
        ctor.labeled_fields()
    }
}

#[derive(Debug)]
pub struct Alias<'a> {
    pub kind: &'a Kind<'a>,
    pub context: &'a [Pred<'a>],
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    pub typ: &'a Located<Type<'a>>,
    pub transparent: bool,
}

impl<'a> Alias<'a> {
    /// Direct record fields in declaration (wire) order.
    pub fn record_fields(&self) -> Option<Vec<&'a FieldType<'a>>> {
        let Type::Record { fields } = &self.typ.value else {
            return None;
        };
        let mut ordered: Vec<_> = fields.iter().collect();
        ordered.sort_by_key(|field| field.index);
        Some(ordered)
    }
}

#[derive(Debug)]
pub struct Binop<'a> {
    pub symbol: &'a str,
    pub associativity: Associativity,
    pub precedence: Precedence,
    pub function: QualifiedName<'a>,
    /// Checked method or imported value scheme; local functions receive their solved scheme
    /// when the module interface is built.
    pub annotation: Option<&'a Annotation<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CtorOpts {
    Normal,
    Enum,
    Unbox,
}

#[derive(Debug)]
pub enum Expr<'a> {
    Assert(&'a Located<Expr<'a>>),
    Fail(Option<&'a Located<Expr<'a>>>),
    Todo(Option<&'a Located<Expr<'a>>>),
    Trace {
        message: &'a Located<Expr<'a>>,
        body: &'a Located<Expr<'a>>,
    },
    Comptime(&'a Located<Expr<'a>>),
    Constant(Constant<'a>),
    Equal {
        left: &'a Located<Expr<'a>>,
        right: &'a Located<Expr<'a>>,
        negate: bool,
    },
    Format {
        value: &'a Located<Expr<'a>>,
    },
    TraceLabel {
        label: &'a Located<Expr<'a>>,
        arguments: &'a [&'a Located<Expr<'a>>],
        body: &'a Located<Expr<'a>>,
        verbose_only: bool,
    },
    /// An explicit conversion or pending ascription. Solved metadata records
    /// the actual output and operation selected for source-site ascriptions.
    Convert {
        kind: ConversionKind,
        typ: &'a Located<Type<'a>>,
        value: &'a Located<Expr<'a>>,
    },
    TupleIndex {
        tuple: &'a Located<Expr<'a>>,
        index: usize,
    },
    /// Static checking always visits the initializer; execution may omit it.
    LetValue {
        pattern: &'a Located<Pattern<'a>>,
        value: &'a Located<Expr<'a>>,
        body: &'a Located<Expr<'a>>,
        uses: u32,
    },
    Match {
        value: &'a Located<Expr<'a>>,
        pattern: &'a Located<Pattern<'a>>,
        body: &'a Located<Expr<'a>>,
        fallback: &'a Located<Expr<'a>>,
        annotation: Option<&'a Located<Type<'a>>>,
        conversion: Option<ConversionSite>,
    },
    RunnableCheck {
        generator: Option<&'a Located<Expr<'a>>>,
        argument_type: Option<&'a Located<Type<'a>>>,
        return_type: Option<&'a Located<Type<'a>>>,
        function: &'a Located<Expr<'a>>,
        benchmark: bool,
    },
    ModuleConstantCheck {
        value: &'a Located<Expr<'a>>,
    },
    TypeScope {
        value: &'a Located<Expr<'a>>,
    },
    Pair {
        first: &'a Located<Expr<'a>>,
        second: &'a Located<Expr<'a>>,
    },
    DataTuple {
        first: &'a Located<Expr<'a>>,
        second: &'a Located<Expr<'a>>,
        rest: &'a [&'a Located<Expr<'a>>],
    },
    DataList {
        elements: &'a [&'a Located<Expr<'a>>],
        tail: Option<&'a Located<Expr<'a>>>,
    },
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
        operator_home: ModuleName<'a>,
        reference: QualifiedName<'a>,
        annotation: &'a Annotation<'a>,
    },
    Str(&'a str),
    Bytes(&'a [u8]),
    Int(i128),
    List(&'a [&'a Located<Expr<'a>>]),
    /// Mirrors Elm's `Can.Binop op home name annotation left right`.
    Binop {
        symbol: &'a str,
        operator_home: ModuleName<'a>,
        reference: QualifiedName<'a>,
        annotation: &'a Annotation<'a>,
        left: &'a Located<Expr<'a>>,
        right: &'a Located<Expr<'a>>,
    },
    Callable {
        arity: usize,
        labels: &'a [&'a str],
        value: &'a Located<Expr<'a>>,
    },
    Function {
        parameters: &'a [&'a Located<Pattern<'a>>],
        body: &'a Located<Expr<'a>>,
    },
    SurfaceCall {
        function: &'a Located<Expr<'a>>,
        arguments: &'a [CallArgument<'a>],
        direct_builtin: Option<DirectBuiltin>,
    },
    Pipe {
        input: &'a Located<Expr<'a>>,
        function: &'a Located<Expr<'a>>,
        arguments: Option<&'a [CallArgument<'a>]>,
        direct_builtin: Option<DirectBuiltin>,
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
    FieldOrModule {
        record: &'a Located<Expr<'a>>,
        field: &'a Located<&'a str>,
        module: Option<&'a Located<Expr<'a>>>,
        module_labels: Option<&'a [&'a str]>,
    },
    Update {
        record: &'a str,
        base: &'a Located<Expr<'a>>,
        fields: &'a [FieldUpdate<'a>],
    },
    RecordUpdate {
        record: &'a str,
        base: &'a Located<Expr<'a>>,
        fields: &'a [FieldUpdate<'a>],
    },
    Record {
        alias: QualifiedName<'a>,
        annotation: &'a Annotation<'a>,
        /// Fields in declaration (wire) order.
        fields: &'a [FieldValue<'a>],
    },
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
    Constant(Constant<'a>),
    Pair {
        first: &'a Located<Pattern<'a>>,
        second: &'a Located<Pattern<'a>>,
    },
    DataTuple {
        first: &'a Located<Pattern<'a>>,
        second: &'a Located<Pattern<'a>>,
        rest: &'a [&'a Located<Pattern<'a>>],
    },
    DataList {
        elements: &'a [&'a Located<Pattern<'a>>],
        tail: Option<&'a Located<Pattern<'a>>>,
    },
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
    Bytes(&'a [u8]),
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

pub use nash_source::DirectBuiltin;

#[derive(Clone, Copy, Debug)]
pub struct CallArgument<'a> {
    pub label: Option<&'a Located<&'a str>>,
    pub value: &'a Located<Expr<'a>>,
}

/// Stable, process-unique identity carried through canonical interfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclaredHoleId(u64);

impl DeclaredHoleId {
    pub fn fresh() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeclaredHoleKind {
    Inference,
    Generic,
}

/// Immutable declaration identity. Refinements live in an owned external store,
/// never behind a borrowed mutable pointer in the canonical tree.
#[derive(Debug)]
pub struct DeclaredHole<'a> {
    pub id: DeclaredHoleId,
    pub kind: DeclaredHoleKind,
    pub owner: QualifiedName<'a>,
    pub region: Region,
}

impl<'a> DeclaredHole<'a> {
    pub fn new(owner: QualifiedName<'a>, region: Region) -> Self {
        Self::with_id(
            DeclaredHoleId::fresh(),
            DeclaredHoleKind::Inference,
            owner,
            region,
        )
    }

    pub fn with_id(
        id: DeclaredHoleId,
        kind: DeclaredHoleKind,
        owner: QualifiedName<'a>,
        region: Region,
    ) -> Self {
        Self {
            id,
            kind,
            owner,
            region,
        }
    }
}

impl PartialEq for DeclaredHole<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.kind == other.kind
    }
}
impl Eq for DeclaredHole<'_> {}
impl std::hash::Hash for DeclaredHole<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.id, state);
        std::hash::Hash::hash(&self.kind, state);
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Type<'a> {
    /// Fresh flexible inference variable at each annotation instantiation.
    Hole,
    DeclaredHole(&'a DeclaredHole<'a>),
    Function {
        arguments: &'a [&'a Located<Type<'a>>],
        result: &'a Located<Type<'a>>,
    },
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
    },
    Tuple {
        first: &'a Located<Type<'a>>,
        second: &'a Located<Type<'a>>,
        rest: &'a [&'a Located<Type<'a>>],
    },
    Alias {
        reference: QualifiedName<'a>,
        arguments: &'a [AliasArgument<'a>],
        /// Unsupplied suffix of the alias's bound parameters, in declaration order.
        remaining: &'a [&'a str],
        target: AliasType<'a>,
    },
}

impl Type<'_> {
    pub const fn unit() -> Self {
        Self::Named {
            reference: QualifiedName {
                home: primitives::builtin_home(),
                name: "unit",
            },
            args: &[],
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum AliasType<'a> {
    Open(&'a Located<Type<'a>>),
    Filled {
        /// Original body, closed over the alias's formal parameters.
        body: &'a Located<Type<'a>>,
        /// Body after substituting the supplied arguments.
        typ: &'a Located<Type<'a>>,
    },
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct AliasArgument<'a> {
    pub name: &'a str,
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldType<'a> {
    /// Declaration position in the Data.List, Data.Constr or term constr encoding.
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

/// Stored scheme obligations. Only Trait is written in a source context.
/// Semantic deduplication must ignore source positions and display provenance.
#[derive(Clone, Copy, Debug)]
pub enum Pred<'a> {
    Trait {
        trait_: QualifiedName<'a>,
        args: &'a [&'a Located<Type<'a>>],
    },
    /// A trait requirement implied by forming a type in the scheme.
    Implied {
        trait_: QualifiedName<'a>,
        args: &'a [&'a Located<Type<'a>>],
    },
    /// Well-formedness of an application with a substitutable head.
    Apply {
        head: &'a Located<Type<'a>>,
        args: &'a [&'a Located<Type<'a>>],
    },
}

impl<'a> Pred<'a> {
    pub fn key(self) -> PredicateKey<'a> {
        PredicateKey(self)
    }
    pub fn trait_ref(self) -> Option<QualifiedName<'a>> {
        match self {
            Self::Trait { trait_, .. } | Self::Implied { trait_, .. } => Some(trait_),
            Self::Apply { .. } => None,
        }
    }
    pub fn args(self) -> &'a [&'a Located<Type<'a>>] {
        match self {
            Self::Trait { args, .. } | Self::Implied { args, .. } | Self::Apply { args, .. } => {
                args
            }
        }
    }
    /// Includes the head, so copying and quantification cannot lose it.
    pub fn types(self) -> impl Iterator<Item = &'a Located<Type<'a>>> {
        let head = match self {
            Self::Apply { head, .. } => Some(head),
            _ => None,
        };
        head.into_iter().chain(self.args().iter().copied())
    }
    pub fn hidden(self) -> bool {
        !matches!(self, Self::Trait { .. })
    }
    pub fn implied(self) -> Self {
        match self {
            Self::Trait { trait_, args } => Self::Implied { trait_, args },
            other => other,
        }
    }
}

/// Semantic identity; source regions and explicit/implied provenance are excluded.
#[derive(Clone, Copy, Debug)]
pub struct PredicateKey<'a>(Pred<'a>);

#[derive(Debug)]
pub struct Trait<'a> {
    pub name: &'a Located<&'a str>,
    pub parameters: &'a [&'a str],
    /// Closed Haskell 98 kind of each trait parameter.
    pub kinds: &'a [&'a Kind<'a>],
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

/// Recursive impl pattern. Variables index the impl's first-occurrence order,
/// giving alpha-equivalent heads the same identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Head<'a> {
    Var(u16),
    Named {
        reference: QualifiedName<'a>,
        args: &'a [Head<'a>],
    },
    Tuple(&'a [Head<'a>]),
    Function(&'a Head<'a>, &'a Head<'a>),
}

#[derive(Debug)]
pub struct Impl<'a> {
    pub variables: &'a [&'a str],
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
    Tuple(usize),
    /// Function types never have impls; a wanted `Show (a -> b)` fails lookup.
    Fun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImplKey<'a> {
    pub trait_: QualifiedName<'a>,
    pub heads: &'a [Head<'a>],
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
    /// Compile-time representation proof; no runtime dictionary.
    Repr {
        trait_: primitives::ReprTrait,
        typ: &'a Located<Type<'a>>,
    },
    /// The compiler-owned core Lift rule for an already-equal Big type.
    ReflexiveLift {
        trait_: QualifiedName<'a>,
        typ: &'a Located<Type<'a>>,
    },
    /// Compiler-owned structural equality for any Big type.
    StructuralEq {
        trait_: QualifiedName<'a>,
        typ: &'a Located<Type<'a>>,
    },
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
    pub fn con(&self) -> Option<HeadCon<'a>> {
        match self {
            Head::Var(_) => None,
            Head::Named { reference, .. } => Some(HeadCon::Named(*reference)),

            Head::Tuple(args) => Some(HeadCon::Tuple(args.len())),
            Head::Function(..) => Some(HeadCon::Fun),
        }
    }

    pub fn to_type(
        &self,
        bump: &'a bumpalo::Bump,
        variables: &[&'a str],
        region: Region,
    ) -> &'a Located<Type<'a>> {
        let typ = match self {
            Head::Var(index) => Type::Var(variables[usize::from(*index)]),
            Head::Named { reference, args } => Type::Named {
                reference: *reference,
                args: bump.alloc_slice_fill_iter(
                    args.iter().map(|arg| arg.to_type(bump, variables, region)),
                ),
            },

            Head::Tuple(args) => Type::Tuple {
                first: args[0].to_type(bump, variables, region),
                second: args[1].to_type(bump, variables, region),
                rest: bump.alloc_slice_fill_iter(
                    args[2..]
                        .iter()
                        .map(|arg| arg.to_type(bump, variables, region)),
                ),
            },
            Head::Function(from, to) => Type::Lambda {
                from: from.to_type(bump, variables, region),
                to: to.to_type(bump, variables, region),
            },
        };
        bump.alloc(Located { region, value: typ })
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn labeled_union_fields_preserve_wire_order() {
        let unit = Located::at_zero(Type::unit());
        let variable = Located::at_zero(Type::Var("a"));
        let types = [&variable, &unit];
        let ctor = Ctor {
            name: "Box",
            labels: Some(&["z", "a"]),
            index: 0,
            arity: 2,
            arguments: &types,
        };
        let name = Located::at_zero("box");
        let ctors = [&ctor];
        let union = Union {
            data_layout: None,
            kind: &Kind::Type,
            context: &[],
            name: &name,
            parameters: &["a"],
            ctors: &ctors,
            alternatives: 1,
            options: CtorOpts::Normal,
        };
        let fields = union.labeled_fields().unwrap();
        assert_eq!(
            fields
                .iter()
                .map(|field| (field.index, field.field))
                .collect::<Vec<_>>(),
            [(0, "z"), (1, "a")]
        );
        assert_eq!(fields[0].typ, &variable);
        assert_eq!(fields[1].typ, &unit);
        let positional = Ctor {
            labels: None,
            ..ctor
        };
        assert!(positional.labeled_fields().is_none());
        let ctors = [&ctor, &positional];
        let multi = Union {
            ctors: &ctors,
            alternatives: 2,
            ..union
        };
        assert!(multi.labeled_fields().is_none());
    }

    #[test]
    fn evidence_alias_identity_does_not_depend_on_body_normalization() {
        let reference = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Main",
            },
            name: "Identity",
        };
        let body = Located::at_zero(Type::Var("a"));
        let unit = Located::at_zero(Type::unit());
        let args = [AliasArgument {
            name: "a",
            typ: &unit,
        }];
        let open = Located::at_zero(Type::Alias {
            reference,
            arguments: &args,
            remaining: &[],
            target: AliasType::Open(&body),
        });
        let filled = Located::at(
            Region::one(),
            Type::Alias {
                reference,
                arguments: &args,
                remaining: &[],
                target: AliasType::Filled {
                    body: &body,
                    typ: &unit,
                },
            },
        );
        let partial = Located::at_zero(Type::Alias {
            reference,
            arguments: &[],
            remaining: &["a"],
            target: AliasType::Open(&body),
        });
        let other = Located::at_zero(Type::Alias {
            reference: QualifiedName {
                name: "Other",
                ..reference
            },
            arguments: &args,
            remaining: &[],
            target: AliasType::Open(&body),
        });
        let first = Evidence::ReflexiveLift {
            trait_: primitives::lift_trait(),
            typ: &open,
        };
        let second = Evidence::ReflexiveLift {
            trait_: primitives::lift_trait(),
            typ: &filled,
        };
        assert_eq!(first, second);
        let keys = HashSet::from([
            first,
            second,
            Evidence::ReflexiveLift {
                trait_: primitives::lift_trait(),
                typ: &partial,
            },
            Evidence::ReflexiveLift {
                trait_: primitives::lift_trait(),
                typ: &other,
            },
        ]);
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn evidence_shares_types_across_source_locations() {
        let home = ModuleName {
            package: None,
            name: "Example",
        };
        let reference = QualifiedName {
            home,
            name: "Value",
        };
        let first_element = Located::at_zero(Type::unit());
        let second_element = Located::at(Region::one(), Type::unit());
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
        let a = Evidence::ReflexiveLift {
            trait_: primitives::lift_trait(),
            typ: &first,
        };
        let b = Evidence::ReflexiveLift {
            trait_: primitives::lift_trait(),
            typ: &second,
        };
        let c = Evidence::ReflexiveLift {
            trait_: primitives::lift_trait(),
            typ: &other,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(HashSet::from([a, b, c]).len(), 2);
    }
}

#[cfg(test)]
mod record_tests {
    use super::*;

    #[test]
    fn alias_record_fields_in_wire_order() {
        let unit = Located::at_zero(Type::Named {
            reference: QualifiedName {
                home: primitives::builtin_home(),
                name: "unit",
            },
            args: &[],
        });
        let fields = [
            FieldType {
                index: 1,
                field: "a",
                typ: &unit,
            },
            FieldType {
                index: 0,
                field: "z",
                typ: &unit,
            },
        ];
        let body = Located::at_zero(Type::Record { fields: &fields });
        let name = Located::at_zero("point");
        let alias = Alias {
            transparent: false,
            kind: &Kind::Type,
            context: &[],
            name: &name,
            parameters: &[],
            typ: &body,
        };
        let ordered = alias.record_fields().unwrap();
        assert_eq!(
            ordered.iter().map(|field| field.field).collect::<Vec<_>>(),
            ["z", "a"]
        );
        assert_eq!(fields[0].field, "a");
        let transparent = Alias {
            typ: &unit,
            ..alias
        };
        assert!(transparent.record_fields().is_none());
    }
}
