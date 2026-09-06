//! Port of the data types from Elm's `Reporting.Error.Type`: what the
//! constraint generator records about *why* two types must match, and the
//! type errors the solver produces when they don't.
//!
//! `toReport` rendering is deferred along with the rest of error reporting.

use nash_ast::FieldUpdate;
use nash_region::Region;

use crate::error_type::ErrorType;

// ERRORS

#[derive(Debug)]
pub enum Error<'a> {
    BadKind {
        region: Region,
        name: &'a str,
        args: &'a [&'a ErrorType<'a>],
        reason: KindProblem<'a>,
    },
    AmbiguousType {
        region: Region,
        name: &'a str,
        variable: &'a ErrorType<'a>,
        predicates: &'a [AmbiguousPredicate<'a>],
    },
    /// A cycle of evidence arguments adds an impl wrapper on each traversal.
    PolymorphicRecursion {
        region: Region,
        name: &'a str,
        trait_: nash_ast::QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
    },
    /// Inference finished without a proof for this use-site requirement.
    UnresolvedConstraint {
        region: Region,
        name: &'a str,
        trait_: nash_ast::QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
    },
    MissingImpl {
        region: Region,
        name: &'a str,
        trait_: nash_ast::QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
        available: &'a [&'a [nash_ast::HeadCon<'a>]],
    },
    ImplResolutionLimit {
        region: Region,
        name: &'a str,
        trait_: nash_ast::QualifiedName<'a>,
    },
    /// A rigid trait argument needs a constraint in the owner's annotation.
    MissingConstraint {
        region: Region,
        name: &'a str,
        trait_: nash_ast::QualifiedName<'a>,
        args: &'a [&'a ErrorType<'a>],
        binder: &'a nash_region::Located<&'a str>,
    },
    /// An annotation quantifies a variable fixed by an enclosing scope.
    AnnotationVariableEscapes {
        region: Region,
        name: Option<&'a str>,
        variable: &'a ErrorType<'a>,
    },
    BadExpr(
        Region,
        Category<'a>,
        &'a ErrorType<'a>,
        Expected<'a, &'a ErrorType<'a>>,
    ),
    BadPattern(
        Region,
        PCategory<'a>,
        &'a ErrorType<'a>,
        PExpected<'a, &'a ErrorType<'a>>,
    ),
    InfiniteType {
        region: Region,
        name: &'a str,
        overall_type: &'a ErrorType<'a>,
    },
}

#[derive(Debug)]
pub enum KindProblem<'a> {
    Mismatch {
        expected: nash_ast::KindScheme<'a>,
        actual: nash_ast::KindScheme<'a>,
    },
    Infinite,
    Rigid {
        declared: nash_ast::ValueKinds<'a>,
        required: nash_ast::ValueKinds<'a>,
    },
    AnonymousRecord,
}

#[derive(Debug)]
pub struct AmbiguousPredicate<'a> {
    pub trait_: nash_ast::QualifiedName<'a>,
    pub args: &'a [&'a ErrorType<'a>],
}

// EXPRESSION EXPECTATIONS

#[derive(Clone, Copy, Debug)]
pub enum Expected<'a, T> {
    NoExpectation(T),
    FromContext(Region, Context<'a>, T),
    FromAnnotation(&'a str, usize, SubContext, T),
}

/// Indexes are zero-based, mirroring Elm's `Index.ZeroBased`.
#[derive(Clone, Copy, Debug)]
pub enum Context<'a> {
    ListEntry(usize),
    Negate,
    OpLeft(&'a str),
    OpRight(&'a str),
    IfCondition,
    IfBranch(usize),
    CaseBranch(usize),
    CallArity(MaybeName<'a>, usize),
    CallArg(MaybeName<'a>, usize),
    RecordAccess {
        record_region: Region,
        maybe_name: Option<&'a str>,
        field_region: Region,
        field: &'a str,
    },
    RecordUpdateKeys(&'a str, &'a [FieldUpdate<'a>]),
    RecordUpdateValue(&'a str),
    Destructure,
}

#[derive(Clone, Copy, Debug)]
pub enum SubContext {
    TypedIfBranch(usize),
    TypedCaseBranch(usize),
    TypedBody,
}

#[derive(Clone, Copy, Debug)]
pub enum MaybeName<'a> {
    FuncName(&'a str),
    CtorName(&'a str),
    OpName(&'a str),
    NoName,
}

/// Elm's `Category`, without the `Float`, `Char`, `Shader`, and `Effects`
/// cases: nash-ast has no such expressions.
#[derive(Clone, Copy, Debug)]
pub enum Category<'a> {
    List,
    Number,
    String,
    If,
    Case,
    CallResult(MaybeName<'a>),
    Lambda,
    Accessor(&'a str),
    Access(&'a str),
    Record,
    Tuple,
    Unit,
    Local(&'a str),
    Foreign(&'a str),
}

// PATTERN EXPECTATIONS

#[derive(Clone, Copy, Debug)]
pub enum PExpected<'a, T> {
    NoExpectation(T),
    FromContext(Region, PContext<'a>, T),
}

#[derive(Clone, Copy, Debug)]
pub enum PContext<'a> {
    TypedArg(&'a str, usize),
    CaseMatch(usize),
    CtorArg(&'a str, usize),
    ListEntry(usize),
    Tail,
}

/// Elm's `PCategory`, without the `PChr` case: nash-ast has no char
/// patterns.
#[derive(Clone, Copy, Debug)]
pub enum PCategory<'a> {
    Record,
    Unit,
    Tuple,
    List,
    Ctor(&'a str),
    Int,
    Bytes,
    Str,
    Bool,
}

// HELPERS

impl<'a, T> Expected<'a, T> {
    /// Elm's `typeReplace`.
    pub fn type_replace<U>(&self, tipe: U) -> Expected<'a, U> {
        match self {
            Expected::NoExpectation(_) => Expected::NoExpectation(tipe),
            Expected::FromContext(region, context, _) => {
                Expected::FromContext(*region, *context, tipe)
            }
            Expected::FromAnnotation(name, arity, context, _) => {
                Expected::FromAnnotation(name, *arity, *context, tipe)
            }
        }
    }
}

impl<'a, T> PExpected<'a, T> {
    /// Elm's `ptypeReplace`.
    pub fn type_replace<U>(&self, tipe: U) -> PExpected<'a, U> {
        match self {
            PExpected::NoExpectation(_) => PExpected::NoExpectation(tipe),
            PExpected::FromContext(region, context, _) => {
                PExpected::FromContext(*region, *context, tipe)
            }
        }
    }
}
