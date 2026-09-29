//! The Core IR. Every expression carries a result type; binders also retain their types. See
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

#[derive(Clone, Copy, Debug)]
pub struct Core<'a> {
    pub ty: Ty<'a>,
    pub kind: CoreKind<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum CoreKind<'a> {
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
    /// Function parameters. A singleton internal delayed recursive value may
    /// use an empty slice with a `Delay` body and matching delayed binder type.
    pub params: &'a [Binder<'a>],
    /// Indices into `params` that every self call passes through unchanged.
    pub static_params: &'a [u16],
    pub body: &'a Core<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseKind {
    Pair,
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
    /// for `Cons`, `[first, second]` for `Pair`, one decoded pair for
    /// `DataConstr`, one binder for the
    /// other `Data` shapes, none for literals.
    pub binders: &'a [Binder<'a>],
    pub body: &'a Core<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Test<'a> {
    Pair,
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
        let mut pending = vec![self];
        while let Some(node) = pending.pop() {
            f(node);
            node.push_children_reversed(&mut pending);
        }
    }

    /// Schedule children for a LIFO work list in the same order as `walk`.
    pub fn push_children_reversed<'tree>(&'tree self, pending: &mut Vec<&'tree Core<'a>>) {
        match &self.kind {
            CoreKind::Var(_) | CoreKind::Lit(_) | CoreKind::Error => {}
            CoreKind::Lam { body, .. } | CoreKind::Delay(body) | CoreKind::Force(body) => {
                pending.push(body)
            }
            CoreKind::App { func, args } => {
                pending.extend(args.iter().rev().copied());
                pending.push(func);
            }
            CoreKind::Let { value, body, .. } => {
                pending.push(body);
                pending.push(value);
            }
            CoreKind::LetRec { binders, body } => {
                pending.push(body);
                pending.extend(binders.iter().rev().map(|b| b.body));
            }
            CoreKind::Case {
                scrutinee,
                branches,
                default,
                ..
            } => {
                pending.extend(*default);
                pending.extend(branches.iter().rev().map(|b| b.body));
                pending.push(scrutinee);
            }
            CoreKind::Constr { fields, .. } => pending.extend(fields.iter().rev().copied()),
            CoreKind::Builtin { args, .. } => pending.extend(args.iter().rev().copied()),
            CoreKind::Field { record, .. } => pending.push(record),
            CoreKind::Trace { message, body } => {
                pending.push(body);
                pending.push(message);
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
