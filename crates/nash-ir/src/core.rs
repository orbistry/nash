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

pub use crate::traverse::map;

impl<'a> Core<'a> {
    /// Visit this node before its children, in field order: function then
    /// arguments, binding values then continuation, scrutinee then branches
    /// then default. Shared subtrees are visited once per occurrence.
    pub fn walk<'tree>(&'tree self, f: &mut impl FnMut(&'tree Core<'a>)) {
        f(self);
        match self {
            Core::Var(_) | Core::Lit(_) | Core::Error => {}
            Core::Lam { body, .. } | Core::Delay(body) | Core::Force(body) => body.walk(f),
            Core::App { func, args } => {
                func.walk(f);
                for arg in *args {
                    arg.walk(f);
                }
            }
            Core::Let { value, body, .. } => {
                value.walk(f);
                body.walk(f);
            }
            Core::LetRec { binders, body } => {
                for binder in *binders {
                    binder.body.walk(f);
                }
                body.walk(f);
            }
            Core::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                scrutinee.walk(f);
                for branch in *branches {
                    branch.body.walk(f);
                }
                if let Some(body) = default {
                    body.walk(f);
                }
            }
            Core::Constr { fields, .. } => {
                for field in *fields {
                    field.walk(f);
                }
            }
            Core::Builtin { args, .. } => {
                for arg in *args {
                    arg.walk(f);
                }
            }
            Core::Field { record, .. } => record.walk(f),
            Core::Cast { arg, .. } => arg.walk(f),
            Core::Trace { message, body } => {
                message.walk(f);
                body.walk(f);
            }
        }
    }

    /// Map children before invoking the visitor on their parent. Unchanged
    /// nodes and metadata slices are reused; replacements are not revisited.
    pub fn map(
        &'a self,
        build: &crate::build::Builder<'a>,
        f: &mut impl FnMut(&'a Core<'a>) -> Option<&'a Core<'a>>,
    ) -> &'a Core<'a> {
        map(build, self, f)
    }
}
