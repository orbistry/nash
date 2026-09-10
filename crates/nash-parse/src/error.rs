//! Syntax error types for the Nash parser.
//!
//! Ported from Elm's `Reporting/Error/Syntax.hs`.
//! These types form a nested hierarchy that enables high-quality error messages.
//!
//! Note: `Type`, `Expr`, `Pattern` here are ERROR types describing parse failures,
//! not AST types. They are allocated in the arena like everything else.

use crate::{Col, Row};
use nash_region::Position;

// =============================================================================
// Top-level Error
// =============================================================================

#[derive(Debug)]
pub enum Error<'a> {
    ModuleNameUnspecified(&'a str),
    ModuleNameMismatch {
        expected: &'a str,
        actual: &'a str,
        row: Row,
        col: Col,
    },
    ParseError(&'a Module<'a>),
}

// =============================================================================
// Module Errors
// =============================================================================

#[derive(Debug)]
pub enum Module<'a> {
    Space(Space, Row, Col),
    BadEnd(Row, Col),
    Problem(Row, Col),
    Validator(Row, Col),
    Name(Row, Col),
    Exposing(&'a Exposing, Row, Col),
    FreshLine(Row, Col),
    ImportStart(Row, Col),
    ImportName(Row, Col),
    ImportAs(Row, Col),
    ImportAlias(Row, Col),
    ImportExposing(Row, Col),
    ImportExposingList(&'a Exposing, Row, Col),
    ImportEnd(Row, Col),
    ImportIndentName(Row, Col),
    ImportIndentAlias(Row, Col),
    ImportIndentExposingList(Row, Col),
    Infix(Row, Col),
    Declarations(&'a Decl<'a>, Row, Col),
    Tests(&'a Tests<'a>, Row, Col),
}

#[derive(Debug)]
pub enum Tests<'a> {
    Space(Space, Row, Col),
    Import(&'a Module<'a>, Row, Col),
    Test(&'a Test<'a>, Row, Col),
    Start(Row, Col),
    IndentStart(Row, Col),
    Alignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum Test<'a> {
    Space(Space, Row, Col),
    Name(StringError, Row, Col),
    NameStart(Row, Col),
    OnceOnUnitTest(Row, Col),
    WithinOpen(Row, Col),
    WithinKind(Row, Col),
    WithinNumber(Number, Row, Col),
    WithinDuplicate(Row, Col),
    WithinEnd(Position, Row, Col),
    Equals(Row, Col),
    Do(Row, Col),
    Body(&'a Do<'a>, Row, Col),
    Let(Row, Col),
    Pattern(&'a Pattern<'a>, Row, Col),
    Via(Row, Col),
    Fuzzer(&'a Expr<'a>, Row, Col),
    In(Row, Col),
    IndentName(Row, Col),
    IndentEquals(Row, Col),
    IndentBody(Row, Col),
    IndentBinder(Row, Col),
    IndentIn(Row, Col),
    BinderAlignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum Exposing {
    Space(Space, Row, Col),
    Start(Row, Col),
    Value(Row, Col),
    Operator(Row, Col),
    OperatorReserved(BadOperator, Row, Col),
    OperatorRightParen(Position, Row, Col),
    TypePrivacyEnd(Position, Row, Col),
    TypePrivacy(Row, Col),
    TypeName(Row, Col),
    End(Row, Col),
    IndentEnd(Row, Col),
    IndentValue(Row, Col),
}

// =============================================================================
// Declaration Errors
// =============================================================================

#[derive(Debug)]
pub enum Decl<'a> {
    Start(Row, Col),
    Space(Space, Row, Col),
    Type(&'a DeclType<'a>, Row, Col),
    Def(&'a str, &'a DeclDef<'a>, Row, Col),
    FreshLineAfterDocComment(Row, Col),
    Attribute(&'a Attribute<'a>, Row, Col),
    Trait(&'a Trait<'a>, Row, Col),
    Impl(&'a Impl<'a>, Row, Col),
}

#[derive(Debug)]
pub enum Impl<'a> {
    Space(Space, Row, Col),
    Head(&'a Type<'a>, Row, Col),
    BadHead(Row, Col),
    Where(Row, Col),
    Method(&'a str, &'a Def<'a>, Row, Col),
    MethodName(Row, Col),
    IndentHead(Row, Col),
    IndentWhere(Row, Col),
    IndentMethod(Row, Col),
    Alignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum Trait<'a> {
    Space(Space, Row, Col),
    Name(Row, Col),
    Param(&'a TypeParam<'a>, Row, Col),
    Super(&'a Type<'a>, Row, Col),
    SuperArg(Row, Col),
    Where(Row, Col),
    MethodName(Row, Col),
    Colon(Row, Col),
    Type(&'a Type<'a>, Row, Col),
    Default(&'a str, &'a Def<'a>, Row, Col),
    IndentName(Row, Col),
    IndentParam(Row, Col),
    IndentWhere(Row, Col),
    IndentMethod(Row, Col),
    IndentColon(Row, Col),
    IndentType(Row, Col),
    Alignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum Attribute<'a> {
    Name(Row, Col),
    Arg(&'a Expr<'a>, Row, Col),
    End(Position, Row, Col),
    Space(Space, Row, Col),
    FreshLine(Row, Col),
    IndentArg(Row, Col),
    IndentEnd(Position, Row, Col),
}

#[derive(Debug)]
pub enum DeclDef<'a> {
    Space(Space, Row, Col),
    Equals(Row, Col),
    Type(&'a Type<'a>, Row, Col),
    Arg(&'a Pattern<'a>, Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    NameRepeat(Row, Col),
    NameMatch(&'a str, Row, Col),
    IndentType(Row, Col),
    IndentEquals(Row, Col),
    IndentBody(Row, Col),
}

#[derive(Debug)]
pub enum DeclType<'a> {
    Space(Space, Row, Col),
    Name(Row, Col),
    Alias(&'a TypeAlias<'a>, Row, Col),
    Union(&'a CustomType<'a>, Row, Col),
    IndentName(Row, Col),
}

#[derive(Debug)]
pub enum TypeAlias<'a> {
    Space(Space, Row, Col),
    Name(Row, Col),
    Param(&'a TypeParam<'a>, Row, Col),
    Equals(Row, Col),
    Body(&'a Type<'a>, Row, Col),
    IndentEquals(Row, Col),
    IndentBody(Row, Col),
}

#[derive(Debug)]
pub enum CustomType<'a> {
    Space(Space, Row, Col),
    Name(Row, Col),
    Param(&'a TypeParam<'a>, Row, Col),
    Equals(Row, Col),
    Bar(Row, Col),
    Variant(Row, Col),
    VariantArg(&'a Type<'a>, Row, Col),
    IndentEquals(Row, Col),
    IndentBar(Row, Col),
    IndentAfterBar(Row, Col),
    IndentAfterEquals(Row, Col),
    Field(Row, Col),
    FieldColon(Row, Col),
    FieldType(&'a Type<'a>, Row, Col),
    FieldEnd(Position, Row, Col),
    IndentField(Row, Col),
    IndentFieldType(Row, Col),
}

// =============================================================================
// Expression Errors
// =============================================================================

#[derive(Debug)]
pub enum Expr<'a> {
    Let(&'a Let<'a>, Row, Col),
    Case(&'a Case<'a>, Row, Col),
    If(&'a If<'a>, Row, Col),
    List(&'a List<'a>, Row, Col),
    Record(&'a Record<'a>, Row, Col),
    Tuple(&'a Tuple<'a>, Row, Col),
    Func(&'a Func<'a>, Row, Col),
    Assert(&'a Keyword<'a>, Row, Col),
    Fail(&'a Keyword<'a>, Row, Col),
    Todo(&'a Keyword<'a>, Row, Col),
    Trace(&'a Keyword<'a>, Row, Col),
    Comptime(&'a Keyword<'a>, Row, Col),
    Do(&'a Do<'a>, Row, Col),
    Macro(&'a Macro<'a>, Row, Col),
    Dot(Row, Col),
    Access(Row, Col),
    OperatorRight(&'a str, Row, Col),
    OperatorReserved(BadOperator, Row, Col),
    Start(Row, Col),
    String(StringError, Row, Col),
    Bytes(Bytes, Row, Col),
    Number(Number, Row, Col),
    Space(Space, Row, Col),
    IndentOperatorRight(&'a str, Row, Col),
}

#[derive(Debug)]
pub enum Keyword<'a> {
    Space(Space, Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    Message(&'a Expr<'a>, Row, Col),
    IndentBody(Row, Col),
    IndentMessage(Row, Col),
}

#[derive(Debug)]
pub enum Do<'a> {
    Space(Space, Row, Col),
    Let(&'a Let<'a>, Row, Col),
    Pattern(&'a Pattern<'a>, Row, Col),
    Arrow(Row, Col),
    Expr(&'a Expr<'a>, Row, Col),
    LastNotExpr(Row, Col),
    IndentStmt(Row, Col),
    IndentArrow(Row, Col),
    IndentExpr(Row, Col),
    Alignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum Macro<'a> {
    Open(Row, Col),
    Arg(&'a Expr<'a>, Row, Col),
    End(Row, Col),
    Space(Space, Row, Col),
    IndentArg(Row, Col),
    IndentEnd(Row, Col),
}

#[derive(Debug)]
pub enum Record<'a> {
    Open(Row, Col),
    End(Row, Col),
    Field(Row, Col),
    Equals(Row, Col),
    Expr(&'a Expr<'a>, Row, Col),
    Space(Space, Row, Col),
    IndentOpen(Row, Col),
    IndentEnd(Row, Col),
    IndentField(Row, Col),
    IndentEquals(Row, Col),
    IndentExpr(Row, Col),
}

#[derive(Debug)]
pub enum Tuple<'a> {
    Expr(&'a Expr<'a>, Row, Col),
    Space(Space, Row, Col),
    End(Row, Col),
    OperatorClose(Row, Col),
    OperatorReserved(BadOperator, Row, Col),
    IndentExpr1(Row, Col),
    IndentExprN(Row, Col),
    IndentEnd(Row, Col),
}

#[derive(Debug)]
pub enum List<'a> {
    Space(Space, Row, Col),
    Open(Row, Col),
    Expr(&'a Expr<'a>, Row, Col),
    End(Row, Col),
    IndentOpen(Row, Col),
    IndentEnd(Row, Col),
    IndentExpr(Row, Col),
}

#[derive(Debug)]
pub enum Func<'a> {
    Space(Space, Row, Col),
    Arg(&'a Pattern<'a>, Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    Arrow(Row, Col),
    IndentArg(Row, Col),
    IndentArrow(Row, Col),
    IndentBody(Row, Col),
}

#[derive(Debug)]
pub enum Case<'a> {
    Space(Space, Row, Col),
    Of(Row, Col),
    Pattern(&'a Pattern<'a>, Row, Col),
    Arrow(Row, Col),
    Expr(&'a Expr<'a>, Row, Col),
    Branch(&'a Expr<'a>, Row, Col),
    IndentOf(Row, Col),
    IndentExpr(Row, Col),
    IndentPattern(Row, Col),
    IndentArrow(Row, Col),
    IndentBranch(Row, Col),
    PatternAlignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum If<'a> {
    Space(Space, Row, Col),
    Then(Row, Col),
    Else(Row, Col),
    ElseBranchStart(Row, Col),
    Condition(&'a Expr<'a>, Row, Col),
    ThenBranch(&'a Expr<'a>, Row, Col),
    ElseBranch(&'a Expr<'a>, Row, Col),
    IndentCondition(Row, Col),
    IndentThen(Row, Col),
    IndentThenBranch(Row, Col),
    IndentElseBranch(Row, Col),
    IndentElse(Row, Col),
}

#[derive(Debug)]
pub enum Let<'a> {
    Space(Space, Row, Col),
    In(Row, Col),
    DefAlignment(usize, Row, Col),
    DefName(Row, Col),
    Def(&'a str, &'a Def<'a>, Row, Col),
    Destruct(&'a Destruct<'a>, Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    IndentDef(Row, Col),
    IndentIn(Row, Col),
    IndentBody(Row, Col),
}

#[derive(Debug)]
pub enum Def<'a> {
    Space(Space, Row, Col),
    Type(&'a Type<'a>, Row, Col),
    NameRepeat(Row, Col),
    NameMatch(&'a str, Row, Col),
    Arg(&'a Pattern<'a>, Row, Col),
    Equals(Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    IndentEquals(Row, Col),
    IndentType(Row, Col),
    IndentBody(Row, Col),
    Alignment(usize, Row, Col),
}

#[derive(Debug)]
pub enum Destruct<'a> {
    Space(Space, Row, Col),
    Pattern(&'a Pattern<'a>, Row, Col),
    Equals(Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    IndentEquals(Row, Col),
    IndentBody(Row, Col),
}

// =============================================================================
// Pattern Errors
// =============================================================================

#[derive(Debug)]
pub enum Pattern<'a> {
    Record(&'a PRecord, Row, Col),
    Tuple(&'a PTuple<'a>, Row, Col),
    List(&'a PList<'a>, Row, Col),
    Start(Row, Col),
    String(StringError, Row, Col),
    Bytes(Bytes, Row, Col),
    Number(Number, Row, Col),
    Alias(Row, Col),
    WildcardNotVar(&'a str, i32, Row, Col),
    Space(Space, Row, Col),
    IndentStart(Row, Col),
    IndentAlias(Row, Col),
}

#[derive(Debug)]
pub enum Bytes {
    Endless(Position),
    OddLength,
    BadHexDigit(usize),
}

#[derive(Debug)]
pub enum PRecord {
    Open(Row, Col),
    End(Row, Col),
    Field(Row, Col),
    Space(Space, Row, Col),
    IndentOpen(Row, Col),
    IndentEnd(Row, Col),
    IndentField(Row, Col),
}

#[derive(Debug)]
pub enum PTuple<'a> {
    Open(Row, Col),
    End(Row, Col),
    Expr(&'a Pattern<'a>, Row, Col),
    Space(Space, Row, Col),
    IndentEnd(Row, Col),
    IndentExpr1(Row, Col),
    IndentExprN(Row, Col),
}

#[derive(Debug)]
pub enum PList<'a> {
    Open(Row, Col),
    End(Row, Col),
    Expr(&'a Pattern<'a>, Row, Col),
    Space(Space, Row, Col),
    IndentOpen(Row, Col),
    IndentEnd(Row, Col),
    IndentExpr(Row, Col),
}

// =============================================================================
// Type Errors
// =============================================================================

#[derive(Debug)]
pub enum Type<'a> {
    Record(&'a TRecord<'a>, Row, Col),
    Tuple(&'a TTuple<'a>, Row, Col),
    Start(Row, Col),
    VarStart(Row, Col),
    Context(Row, Col),
    IndentAfterContext(Row, Col),
    Space(Space, Row, Col),
    IndentStart(Row, Col),
}

#[derive(Debug)]
pub enum TypeParam<'a> {
    Start(Row, Col),
    Colon(Row, Col),
    Repr(&'a Repr<'a>, Row, Col),
    End(Row, Col),
    Space(Space, Row, Col),
    IndentColon(Row, Col),
    IndentRepr(Row, Col),
    IndentEnd(Row, Col),
}

#[derive(Debug)]
pub enum Repr<'a> {
    /// Removed kind-arrow syntax, diagnosed at the arrow.
    Arrow(Row, Col),
    Start(Row, Col),
    Name(&'a str, Row, Col),
    Space(Space, Row, Col),
}

#[derive(Debug)]
pub enum TRecord<'a> {
    Open(Row, Col),
    End(Row, Col),
    Field(Row, Col),
    Colon(Row, Col),
    Type(&'a Type<'a>, Row, Col),
    Space(Space, Row, Col),
    IndentOpen(Row, Col),
    IndentField(Row, Col),
    IndentColon(Row, Col),
    IndentType(Row, Col),
    IndentEnd(Row, Col),
}

#[derive(Debug)]
pub enum TTuple<'a> {
    Repr(&'a Repr<'a>, Row, Col),
    IndentRepr(Row, Col),
    Open(Row, Col),
    End(Row, Col),
    Type(&'a Type<'a>, Row, Col),
    Space(Space, Row, Col),
    IndentType1(Row, Col),
    IndentTypeN(Row, Col),
    IndentEnd(Row, Col),
}

// =============================================================================
// Literal Errors (no lifetimes - leaf types)
// =============================================================================

#[derive(Debug)]
pub enum StringError {
    EndlessSingle(Position),
    EndlessMulti(Position),
    Escape(Escape),
}

#[derive(Debug)]
pub enum Escape {
    Unknown,
    BadUnicodeFormat(usize),
    BadUnicodeCode(usize),
    BadUnicodeLength {
        code: usize,
        expected: i32,
        actual: i32,
    },
}

#[derive(Debug)]
pub enum Number {
    End,
    Dot(i128),
    HexDigit,
    NoLeadingZero,
}

// =============================================================================
// Misc (no lifetimes - leaf types)
// =============================================================================

#[derive(Debug)]
pub enum Space {
    TooDeep,
    HasTab,
    EndlessMultiComment(Position),
}

#[derive(Debug)]
pub enum BadOperator {
    Dot,
    Pipe,
    Arrow,
    Equals,
    HasType,
    FatArrow,
    LeftArrow,
}
