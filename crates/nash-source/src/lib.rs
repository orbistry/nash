use nash_region::{Located, Region};

#[derive(Debug)]
pub struct Module<'a> {
    pub kind: ModuleKind,
    pub name: Option<&'a Located<&'a str>>,
    pub exports: &'a Located<Exposing<'a>>,
    pub docs: &'a Docs<'a>,
    pub imports: &'a [&'a Import<'a>],
    pub values: &'a [&'a Located<Value<'a>>],
    pub unions: &'a [&'a Located<Union<'a>>],
    pub aliases: &'a [&'a Located<Alias<'a>>],
    pub traits: &'a [&'a Located<Trait<'a>>],
    pub impls: &'a [&'a Located<Impl<'a>>],
    pub tests: Option<&'a Tests<'a>>,
    pub binops: &'a [&'a Located<Infix<'a>>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Normal,
    Validator(Region),
}

#[derive(Debug)]
pub struct Import<'a> {
    pub import: &'a Located<&'a str>,
    pub alias: Option<&'a str>,
    pub exposing: &'a Exposing<'a>,
}

#[derive(Debug)]
pub struct Value<'a> {
    pub name: &'a Located<&'a str>,
    pub arguments: &'a [&'a Located<Pattern<'a>>],
    pub body: &'a Located<Expr<'a>>,
    pub annotation: Option<&'a Annotation<'a>>,
    pub attributes: &'a [&'a Attribute<'a>],
}

#[derive(Debug)]
pub struct Attribute<'a> {
    pub name: &'a Located<&'a str>,
    pub args: &'a [&'a Located<Expr<'a>>],
}

#[derive(Debug)]
pub struct Trait<'a> {
    pub name: &'a Located<&'a str>,
    pub params: &'a [&'a TypeParam<'a>],
    pub supers: &'a [&'a Located<Constraint<'a>>],
    pub methods: &'a [&'a TraitMethod<'a>],
    pub attributes: &'a [&'a Attribute<'a>],
}

#[derive(Debug)]
pub struct TraitMethod<'a> {
    pub name: &'a Located<&'a str>,
    pub annotation: &'a Annotation<'a>,
    pub default: Option<&'a Located<Def<'a>>>,
}

#[derive(Debug)]
pub struct Impl<'a> {
    pub context: &'a [&'a Located<Constraint<'a>>],
    pub head: &'a Located<Constraint<'a>>,
    pub methods: &'a [&'a Located<Def<'a>>],
    pub attributes: &'a [&'a Attribute<'a>],
}

#[derive(Debug)]
pub struct Tests<'a> {
    pub imports: &'a [&'a Import<'a>],
    pub tests: &'a [&'a Located<Test<'a>>],
}

#[derive(Debug)]
pub struct Test<'a> {
    pub name: &'a Located<&'a str>,
    pub expect: Expect,
    pub budget: Option<Budget>,
    pub body: TestBody<'a>,
}

#[derive(Debug)]
pub struct Block<'a> {
    pub stmts: &'a [&'a Located<Stmt<'a>>],
    pub last: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub enum TestBody<'a> {
    Unit(&'a Block<'a>),
    Prop {
        binders: &'a [&'a Located<ViaBinder<'a>>],
        body: &'a Block<'a>,
    },
}

#[derive(Debug)]
pub struct ViaBinder<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub fuzzer: &'a Located<Expr<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    Pass,
    Fail,
    FailOnce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Cpu(i128),
    Mem(i128),
    Both { cpu: i128, mem: i128 },
}

/// A type annotation with an optional constraint context.
#[derive(Debug)]
pub struct Annotation<'a> {
    pub constraints: &'a [&'a Located<Constraint<'a>>],
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Debug)]
pub struct Constraint<'a> {
    pub class: &'a Located<&'a str>,
    pub module: Option<&'a str>,
    pub args: &'a [&'a Located<Type<'a>>],
}

// type Maybe a
//   = Just a
//   | Nothing
#[derive(Debug)]
pub struct Union<'a> {
    pub name: &'a Located<&'a str>,
    // type vars
    pub arguments: &'a [&'a TypeParam<'a>],
    pub ctors: &'a [&'a Ctor<'a>],
    pub attributes: &'a [&'a Attribute<'a>],
}

#[derive(Debug)]
pub struct Ctor<'a> {
    pub name: &'a Located<&'a str>,
    pub arguments: CtorArgs<'a>,
}

#[derive(Debug)]
pub enum CtorArgs<'a> {
    Positional(&'a [&'a Located<Type<'a>>]),
    Labeled(&'a [(&'a Located<&'a str>, &'a Located<Type<'a>>)]),
}

#[derive(Debug)]
pub struct Alias<'a> {
    pub name: &'a Located<&'a str>,
    // type vars
    pub arguments: &'a [&'a TypeParam<'a>],
    pub typ: &'a Located<Type<'a>>,
    pub attributes: &'a [&'a Attribute<'a>],
}

#[derive(Debug)]
pub struct Infix<'a> {
    pub op: &'a str,
    pub associativity: Associativity,
    pub precedence: Precedence,
    pub name: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Associativity {
    Left,
    None,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Precedence(pub u16);

#[derive(Debug)]
pub enum Expr<'a> {
    Str(&'a str),
    Bytes(&'a [u8]),
    Int(i128),
    Assert(&'a Located<Expr<'a>>),
    Fail(Option<&'a Located<Expr<'a>>>),
    Todo(Option<&'a Located<Expr<'a>>>),
    Trace {
        message: &'a Located<Expr<'a>>,
        body: &'a Located<Expr<'a>>,
    },
    Comptime(&'a Located<Expr<'a>>),
    Do {
        stmts: &'a [&'a Located<Stmt<'a>>],
        last: &'a Located<Expr<'a>>,
    },
    MacroCall {
        name: &'a Located<&'a str>,
        module: Option<&'a str>,
        args: &'a [&'a Located<Expr<'a>>],
    },
    LeftSection {
        left: &'a Located<Expr<'a>>,
        operator: &'a str,
    },
    RightSection {
        operator: &'a str,
        right: &'a Located<Expr<'a>>,
    },
    Var {
        kind: VarType,
        name: &'a str,
    },
    VarQual {
        kind: VarType,
        module: &'a str,
        name: &'a str,
    },
    List(&'a [&'a Located<Expr<'a>>]),
    Op(&'a str),
    Negate(&'a Located<Expr<'a>>),
    BinOps {
        operands: &'a [&'a BinOpOperand<'a>],
        last: &'a Located<Expr<'a>>,
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
        branches: &'a [&'a IfBranch<'a>],
        final_else: &'a Located<Expr<'a>>,
    },
    Let {
        defs: &'a [&'a Located<Def<'a>>],
        body: &'a Located<Expr<'a>>,
    },
    Case {
        scrutinee: &'a Located<Expr<'a>>,
        arms: &'a [&'a CaseArm<'a>],
    },
    Accessor(&'a str),
    Access {
        record: &'a Located<Expr<'a>>,
        field: &'a Located<&'a str>,
    },
    Update {
        record: &'a Located<&'a str>,
        fields: &'a [&'a FieldAssign<'a>],
    },
    Record(&'a [&'a FieldAssign<'a>]),
    Unit,
    Tuple {
        first: &'a Located<Expr<'a>>,
        second: &'a Located<Expr<'a>>,
        rest: &'a [&'a Located<Expr<'a>>],
    },
}

#[derive(Debug)]
pub enum Stmt<'a> {
    Let(&'a [&'a Located<Def<'a>>]),
    Bind {
        pattern: &'a Located<Pattern<'a>>,
        expr: &'a Located<Expr<'a>>,
    },
    Expr(&'a Located<Expr<'a>>),
}

#[derive(Debug)]
pub enum VarType {
    LowVar,
    CapVar,
}

#[derive(Debug)]
pub struct IfBranch<'a> {
    pub condition: &'a Located<Expr<'a>>,
    pub then_branch: &'a Located<Expr<'a>>,
}

/// An operand in a binary operator chain: expression followed by operator.
#[derive(Debug)]
pub struct BinOpOperand<'a> {
    pub expr: &'a Located<Expr<'a>>,
    pub op: &'a Located<&'a str>,
}

#[derive(Debug)]
pub enum Def<'a> {
    Define {
        name: &'a Located<&'a str>,
        args: &'a [&'a Located<Pattern<'a>>],
        body: &'a Located<Expr<'a>>,
        annotation: Option<&'a Annotation<'a>>,
    },
    Destruct {
        pattern: &'a Located<Pattern<'a>>,
        body: &'a Located<Expr<'a>>,
    },
}

#[derive(Debug)]
pub struct CaseArm<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub body: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub struct FieldAssign<'a> {
    pub field: &'a Located<&'a str>,
    pub value: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub enum Pattern<'a> {
    Anything,
    Var(&'a str),
    Record(&'a [&'a Located<&'a str>]),
    Alias {
        pattern: &'a Located<Pattern<'a>>,
        name: &'a Located<&'a str>,
    },
    Unit,
    Tuple {
        first: &'a Located<Pattern<'a>>,
        second: &'a Located<Pattern<'a>>,
        rest: &'a [&'a Located<Pattern<'a>>],
    },
    Ctor {
        region: Region,
        name: &'a str,
        args: &'a [&'a Located<Pattern<'a>>],
    },
    CtorQual {
        region: Region,
        module: &'a str,
        name: &'a str,
        args: &'a [&'a Located<Pattern<'a>>],
    },
    List(&'a [&'a Located<Pattern<'a>>]),
    Cons {
        head: &'a Located<Pattern<'a>>,
        tail: &'a Located<Pattern<'a>>,
    },
    Str(&'a str),
    Bytes(&'a [u8]),
    Int(i128),
}

#[derive(Debug)]
pub enum Type<'a> {
    Repr {
        typ: &'a Located<Type<'a>>,
        repr: &'a Located<Repr>,
    },
    Lambda {
        from: &'a Located<Type<'a>>,
        to: &'a Located<Type<'a>>,
    },
    Var(&'a str),
    VarApp {
        region: Region,
        name: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    },
    Type {
        region: Region,
        name: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    },
    TypeQual {
        region: Region,
        module: &'a str,
        name: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    },
    Record(&'a [&'a FieldType<'a>]),
    Unit,
    Tuple {
        first: &'a Located<Type<'a>>,
        second: &'a Located<Type<'a>>,
        rest: &'a [&'a Located<Type<'a>>],
    },
}

impl<'a> Type<'a> {
    /// Inspect shape without discarding the annotation from the stored type.
    pub fn unannotated(&self) -> &Self {
        let mut typ = self;
        while let Self::Repr { typ: inner, .. } = typ {
            typ = &inner.value;
        }
        typ
    }
}

#[derive(Debug)]
pub struct TypeParam<'a> {
    pub name: &'a Located<&'a str>,
    pub repr: Option<&'a Located<Repr>>,
}

/// Representation annotation sugar; Haskell 98 kinds have no surface syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Repr {
    Big,
    Const,
    Term,
    Storable,
}

#[derive(Debug)]
pub struct FieldType<'a> {
    pub field: &'a Located<&'a str>,
    pub typ: &'a Located<Type<'a>>,
}

#[derive(Debug)]
pub enum Docs<'a> {
    NoDocs(Region),
    YesDocs {
        overview: &'a Comment<'a>,
        comments: &'a [&'a (&'a str, &'a Comment<'a>)],
    },
}

#[derive(Debug)]
pub struct Comment<'a>(pub &'a Snippet<'a>);

#[derive(Debug)]
pub struct Snippet<'a> {
    pub data: &'a [u8], // already the relevant slice
    // offset: usize,
    // length: usize,
    pub off_row: u16,
    pub off_col: u16,
}

#[derive(Debug)]
pub enum Exposing<'a> {
    Open,
    Explicit(&'a [&'a Exposed<'a>]),
}

#[derive(Debug)]
pub enum Exposed<'a> {
    Lower(&'a Located<&'a str>),
    Upper {
        name: &'a Located<&'a str>,
        privacy: Privacy,
    },
    LowerType {
        name: &'a Located<&'a str>,
        privacy: Privacy,
    },
    Operator {
        region: Region,
        op: &'a str,
    },
}

#[derive(Debug)]
pub enum Privacy {
    Public(Region),
    Private,
}
