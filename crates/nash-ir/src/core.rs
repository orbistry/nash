//! The Core IR. One tree with explicit binder types and erased parametric binders. See
//! docs/codegen.md for node semantics and lowering.

use nash_plutus::builtin::DefaultFunction;
use nash_plutus::constant::{Constant, Integer};

use crate::ty::Ty;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Name<'a> {
    pub text: &'a str,
    pub unique: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Binder<'a> {
    pub name: Name<'a>,
    pub ty: Ty<'a>,
}

#[derive(Debug)]
pub enum Core<'a> {
    Var(Name<'a>),
    Lit(&'a Constant<'a>),
    Lam {
        params: &'a [Binder<'a>],
        body: &'a Core<'a>,
    },
    App {
        func: &'a Core<'a>,
        args: &'a [&'a Core<'a>],
    },
    Let {
        binder: Binder<'a>,
        value: &'a Core<'a>,
        body: &'a Core<'a>,
    },
    LetRec {
        binders: &'a [RecBinder<'a>],
        body: &'a Core<'a>,
    },
    Case {
        kind: CaseKind,
        scrutinee: &'a Core<'a>,
        branches: &'a [Branch<'a>],
        default: Option<&'a Core<'a>>,
    },
    Constr {
        tag: u16,
        fields: &'a [&'a Core<'a>],
    },
    Field {
        record: &'a Core<'a>,
        index: u16,
        arity: u16,
    },
    Builtin {
        func: DefaultFunction,
        args: &'a [&'a Core<'a>],
    },
    Cast {
        kind: CastKind,
        from: Ty<'a>,
        to: Ty<'a>,
        arg: &'a Core<'a>,
    },
    Trace {
        message: &'a Core<'a>,
        body: &'a Core<'a>,
    },
    Error,
    Delay(&'a Core<'a>),
    Force(&'a Core<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct RecBinder<'a> {
    pub binder: Binder<'a>,
    pub params: &'a [Binder<'a>],
    /// Indices into `params` that every self call passes through unchanged.
    pub static_params: &'a [u16],
    pub body: &'a Core<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseKind {
    Tag,
    Bool,
    Int,
    Bytes,
    List,
    Data,
}

#[derive(Clone, Copy, Debug)]
pub struct Branch<'a> {
    pub test: Test<'a>,
    /// Fields bound by the test: constructor fields for `Tag`, `[head, tail]`
    /// for `Cons`, `[tag, fields]` for `DataConstr`, one binder for the
    /// other `Data` shapes, none for literals.
    pub binders: &'a [Binder<'a>],
    pub body: &'a Core<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Test<'a> {
    Tag(u16),
    True,
    False,
    Int(&'a Integer),
    Bytes(&'a [u8]),
    Nil,
    Cons,
    DataConstr,
    DataMap,
    DataList,
    DataI,
    DataB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastKind {
    ToData,
    FromDataShallow,
    ValidateData,
    Lift,
    Lower,
}

/// A whole program: top-level bindings in dependency order plus the root.
#[derive(Debug)]
pub struct Module<'a> {
    pub bindings: &'a [(Binder<'a>, &'a Core<'a>)],
    pub root: &'a Core<'a>,
}
