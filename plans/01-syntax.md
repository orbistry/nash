# Plan 01 — Surface syntax

## Goal

Extend `nash-source` and `nash-parse` to the full surface syntax in
[docs/syntax.md](../docs/syntax.md): `'a` type variables and little type
names, kind-annotated binders, constraints, bytes literals, keyword
expressions, `do` blocks, macro calls, attributes, `trait`/`impl`,
partial operator sections, `validator module`, and the `tests` block. Remove
Elm leftovers (`Char`, `Float`, ports, effects, shaders, record extension
types).

Status: complete. Chunks 0 through 12, including 6a, and the nested-section
verification fix landed on 2026-09-04.

## Verification — 2026-09-04

The initial review found a nested-section binder collision. Permanent left
and right nesting tests reproduced `Shadowing { name: "$section", .. }`
before the fix. `canonicalize_section` now selects a generated binder that
is absent from the enclosing scope. Reviewed canonical snapshots verify
separate outer and inner bindings. The inference acceptance test imports
an operator from another module, applies nested sections, and verifies
unit and string results through parse, canonicalize, constrain, and solve.

After the fix:

- `cargo fmt --all`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed (1,704 tests; three ignored documentation tests).
- `cargo insta test`: passed; no snapshots awaiting review.
- `cargo insta test --unreferenced reject`: passed; no unreferenced snapshots.

All 13 original feature changesets remain present. A patch changeset records
the binder fix. These results verify surface syntax and the existing
compiler pipeline, not execution support for features deliberately
rejected with `Unsupported` until later plans.

## Prerequisites

None. This is the first plan. Every later plan (kinds, traits, testing,
macros) consumes the AST defined here.

## Crates touched

- `crates/nash-source` — AST additions (`src/lib.rs`)
- `crates/nash-parse` — parser, `src/error.rs`, `src/keyword.rs`, snapshots
- `crates/nash-can` — minimal edits only, so the workspace compiles after
  every chunk: field renames and one new error variant
  `Error::Unsupported { feature: &'static str, region: Region }` in
  `crates/nash-can/src/error.rs`. Every new source node that nash-can cannot
  yet canonicalize is rejected with it. Later plans replace those arms.
- `SPEC.md` — progress checklist only (the grammar lives in `docs/syntax.md`)
- `.sampo/changesets/` — one changeset per chunk

## Reference files

- Elm: `elm/compiler/src/Parse/{Primitives,Type,Expression,Pattern,
  Declaration,Module,Keyword,Variable,Symbol,Number,String,Space}.hs`,
  `elm/compiler/src/Reporting/Error/Syntax.hs`
- Aiken (tests block, bytes literals, `fail`/`todo`/`trace`):
  `aiken/crates/aiken-lang/src/parser/{expr,definition,literal}/`,
  `aiken/crates/aiken-lang/src/parser/definition/test.rs`,
  `aiken/crates/aiken-lang/src/parser/literal/bytearray.rs`
- Current parser (cited by `file:line` below): `crates/nash-parse/src/*`,
  `crates/nash-source/src/lib.rs`

## Conventions for every chunk

- Parser functions keep Elm's shape: `one_of` (`lib.rs:217`),
  `one_of_with_fallback` (`lib.rs:253`), `in_context` (`lib.rs:296`),
  `specialize` (`lib.rs:335`), `word1`/`word2` (`lib.rs:362`, `lib.rs:383`),
  `chomp`/`chomp_and_check_indent`/`check_indent`/`check_aligned`
  (`space.rs:33`, `:48`, `:77`, `:93`), `with_indent`/`with_backset_indent`
  (`lib.rs:132`, `:152`).
- New keyword parsers go in `keyword.rs` next to the existing ones
  (`keyword.rs:25`–`139`), all through the private `keyword` helper
  (`keyword.rs:144`). Add a keyword parser in the chunk that first uses it
  (clippy runs with `-D warnings`; unused functions fail CI).
- Error enums follow `error.rs`: one enum per syntactic construct, every
  variant ends in `Row, Col`, nested errors are `&'a Inner<'a>` allocated in
  the arena by the `specialize`/`in_context` closure.
- Tests use the module-local macros: `assert_type_snapshot!` (`type_.rs:512`),
  `assert_expression_snapshot!` / `assert_indented_expression_snapshot!`
  (`expression/mod.rs:397`, `:440`), `assert_expr_error_snapshot!`
  (`expression/mod.rs:379`), `assert_pattern_snapshot!` /
  `assert_pattern_error_snapshot!` (`pattern/mod.rs:323`, `:341`),
  `assert_decl_snapshot!` (`declaration/mod.rs:139`), `assert_module_snapshot!`
  (`module.rs:338`). Where a chunk needs an error variant of a macro that does
  not exist yet (`assert_decl_error_snapshot!`, `assert_type_error_snapshot!`,
  `assert_module_error_snapshot!`), add it beside the success macro with the
  same body but `.expect_err("expected parse error")`.
- After each chunk: `cargo fmt --all`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo insta test --accept` after reviewing
  the diff, `cargo test`.
- Comments: one-line docstrings. Keep the `Mirrors Elm's ...` Haskell blocks
  only where a function is a port; new Nash-only functions get one sentence.

---

## Chunk 0 — Leftovers and reserved words

### Files

- `crates/nash-parse/src/keyword.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-parse/src/number.rs`
- `crates/nash-parse/src/symbol.rs`
- `crates/nash-parse/src/expression/number.rs` (tests)
- `crates/nash-parse/src/expression/variable.rs` (tests)
- `crates/nash-parse/src/pattern/term.rs` (tests)

### Change

1. Replace the reserved word list (`keyword.rs:8`). `port` goes; `do`,
   `trait`, `impl`, `comptime`, `assert`, `fail`, `todo`, `trace`, `tests`,
   `validator` come in. Contextual words (`alias infix left right non test
   prop via once within cpu mem`) are not reserved.
2. Delete Elm-only error variants. `Error::{UnexpectedPort, NoPorts,
   NoPortsInPackage, NoPortModulesInPackage, NoEffectsOutsideKernel}`
   (`error.rs:24`–`44`), `Module::{PortProblem, PortName, PortExposing,
   Effect}` (`error.rs:60`–`63`), `Decl::Port` and `enum Port`
   (`error.rs:100`, `:121`–`129`), `Expr::{Char, EndlessShader,
   ShaderProblem}` (`error.rs:182`, `:186`–`187`), `Pattern::{Char, Float}`
   (`error.rs:321`, `:324`), `enum Char` (`error.rs:409`–`413`). Nothing
   outside `error.rs` references them (verified by grep).
3. Reject floats explicitly. `chomp_zero` (`number.rs:56`) and `chomp_int`
   (`number.rs:80`) return `Number::Dot(n)` when the next bytes are `.` and a
   digit. Change `Number::Dot(i32)` (`error.rs:437`) to `Dot(i128)` so the
   digits parsed so far fit.
4. Reserve `=>` and `<-` as operators (`symbol.rs:51`–`58`).
5. Fix `lower_name` (`variable.rs:30`–`52`): it advances before the reserved
   check, so a reserved word is a *consuming* failure and `one_of` treats it
   as committed. Elm's `Var.lower` fails empty (`eerr`). Save state before
   advancing and restore on a reserved word, exactly as `foreign_alpha`
   already does (`variable.rs:119`–`131`). Later chunks rely on this
   (`int where`, `'a =>`, a `do` block ending at an aligned keyword).

### Code

```rust
// keyword.rs
pub const RESERVED: &[&str] = &[
    "if", "then", "else", "case", "of", "let", "in", "do",
    "type", "module", "where", "import", "exposing", "as",
    "trait", "impl",
    "comptime", "assert", "fail", "todo", "trace",
    "tests", "validator",
];
```

```rust
// number.rs — inside chomp_int's loop, before the ident-inner arm
Some(b'.') if matches!(self.peek_at(1), Some(d) if d.is_ascii_digit()) => {
    return Err(error::Number::Dot(n));
}
// same arm in chomp_zero with `Dot(0)`
```

```rust
// error.rs
pub enum Number { End, Dot(i128), HexDigit, NoLeadingZero }

pub enum BadOperator { Dot, Pipe, Arrow, Equals, HasType, FatArrow, LeftArrow }
```

```rust
// symbol.rs — in `operator`
"=>" => Err(to_error(BadOperator::FatArrow, row, col)),
"<-" => Err(to_error(BadOperator::LeftArrow, row, col)),
```

### Elm reference

`Parse/Number.hs` (`chompInt`, `NumberDot`), `Parse/Symbol.hs`
(`operator`, `BadOperator`), `Parse/Variable.hs` (`reservedWords`).

### Tests

- `expression/number.rs`: `assert_expr_error_snapshot!("1.5")`,
  `assert_expr_error_snapshot!("0.5")`.
- `expression/variable.rs`: `assert_expr_error_snapshot!("do")`,
  `assert_expr_error_snapshot!("trait")`, `assert_expr_snapshot!("port")`
  (now a plain variable), `assert_expr_snapshot!("test")`.
- `expression/mod.rs`: `assert_expression_snapshot!("a => b")` becomes an
  error test `assert_expr_error_snapshot!` on `term`? No: `=>` is hit in
  `chomp_expr_end`; add `assert_expression_error_snapshot!` (new macro,
  `.expect_err`) with `"a => b"` and `"a <- b"`.
- `pattern/term.rs`: `assert_pattern_error_snapshot!("'x'")`.

### Done when

No `Char`/`Float`/`Port`/`Shader` identifiers remain in `nash-parse`;
`1.5` snapshots as `Number(Dot(1), ..)`; clippy and all tests pass.

---

## Chunk 1 — `'a` type variables, little type names, kinds, no record extension

### Files

- `crates/nash-source/src/lib.rs`
- `crates/nash-parse/src/type_.rs`
- `crates/nash-parse/src/declaration/union.rs`
- `crates/nash-parse/src/declaration/type_alias.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-parse/src/expression/variable.rs` (add `type_var_name`)
- `crates/nash-parse/src/exposing.rs` (`type lower(..)` in exposing lists)
- `crates/nash-can/src/{error.rs,types.rs,module.rs,pattern.rs,interface.rs,environment/foreign.rs}` (compile fixes)
- Test fixtures in `crates/nash-can/src/module.rs`, `crates/nash-can/src/types.rs`,
  `crates/nash-solve/tests/inference.rs`, `scratch/src/*.nash`

### Change

1. **AST.** `Type::Var(&'a str)` (`lib.rs:217`) now means `'a` (stored without
   the quote). Add `Type::VarApp` for `'f 'a`. Remove `ext` from
   `Type::Record` (`lib.rs:229`–`232`). Add `TypeParam` and `Kind`. `Union`
   and `Alias` `arguments` (`lib.rs:37`, `:51`) become `&'a [&'a TypeParam<'a>]`.
2. **Type terms** (`type_.rs:156`–`201`). The "type variable" alternative
   (`type_.rs:191`–`194`) parses `'a`. The named alternative accepts lowercase
   heads too: `foreign_upper` (`type_.rs:455`) becomes `type_name` and returns
   `TypeName::{Unqualified, Qualified}` for `int`, `Int`, `Data.Map`.
3. **Type application** (`type_.rs:86`–`108`). The head is a type name or a
   type variable. Zero-argument `'a` stays `Type::Var`; `'f 'a` is `VarApp`.
4. **Record types** (`type_.rs:316`–`380`). Delete the `|` alternative
   (`type_.rs:346`–`359`); the body after `{` is `}` or a field list.
5. **Type parameters.** `chomp_custom_name_to_equals_help` (`union.rs:59`) and
   `chomp_alias_name_to_equals_help` (`type_alias.rs:57`) parse `type_param`
   instead of `lower_name`. `chomp_custom_name_to_equals` (`union.rs:45`) and
   `chomp_alias_name_to_equals` (`type_alias.rs:43`) use `type_decl_name`
   (upper or lower) instead of `upper_name`.
6. **Kinds.** New `kind_expr` / `kind_atom` in `type_.rs`; `Big -> Big`,
   `(Big -> Big) -> Const`. The atom set is closed (`Big`, `Const`, `Term`,
   `Storable`); any other uppercase name is `error::Kind::Name`.
7. **Labeled constructor fields.** `variant` (`union.rs:99`) branches on the
   byte after the constructor name: `{` parses `record_fields` into
   `CtorArgs::Labeled` (same loop shape as `type_record_field`/
   `type_record_end`, `type_.rs:391`, `:410`, with `CustomType` errors);
   anything else is the existing positional `chomp_variant_args`
   (`union.rs:121`) into `CtorArgs::Positional`. A `{` after a positional
   argument is still parsed as a record type term and rejected later by
   nash-can (anonymous record types are only legal as an alias body;
   `kinds.md`). Construction `Datum { owner = o, deadline = d }` needs no
   parser change: it is `Call { function: Var CapVar, arguments: [Record] }`
   and canonicalization reinterprets it against the constructor declaration.
8. **Exposing lists.** Little types are exposed and imported as `type
   option(..)` / `type step`. `chomp_exposed` (`exposing.rs:100`–`138`) gets
   a fourth alternative: `keyword_type`, indent check, `lower_name`, then
   `privacy` (`exposing.rs:143`). An uppercase name (or nothing) after
   `type` is `Exposing::TypeName`. New `Exposed::LowerType { name, privacy }`.
   nash-can treats `LowerType` exactly like `Upper` in every arm that
   resolves exposed types (`interface.rs`, `environment/foreign.rs`): the
   type environment already keys types by name regardless of case.
9. **nash-can.** `types.rs:71` unchanged; `types.rs:89`–`101` drops `ext`
   (`ext: None` on the canonical record until row polymorphism is removed);
   `SourceType::VarApp` and any `TypeParam.kind.is_some()` return
   `Error::Unsupported`. `module.rs:347`, `:457`, `:478`, `:487`, `:506`,
   `:528`, `:537`, `:558` read `arg.name.value` / `arg.name.region`. Every
   fixture that spells a type variable bare (`Maybe a`, `List a`, `a -> b`)
   is rewritten with `'a`; the `extensible_record_alias_allowed` test becomes
   an error test (`Type::Record` context error) or is deleted. Every reader
   of `ctor.arguments` (`module.rs:354`, `:356`, `:379`, `:384`, `:434`,
   `:493`) goes through a helper `ctor_arg_types(ctor) -> impl Iterator<Item
   = &'a Located<Type<'a>>>` that yields positional types or labeled field
   types in declaration order; labels are dropped here and picked up by the
   representation plan (accessors, labeled construction, and the
   `Ctor { a, b }` pattern sugar rewrite).

### Code

```rust
// nash-source/src/lib.rs
#[derive(Debug)]
pub enum Type<'a> {
    Lambda { from: &'a Located<Type<'a>>, to: &'a Located<Type<'a>> },
    /// `'a`, stored without the quote.
    Var(&'a str),
    /// `'f 'a`: a type variable applied to one or more arguments.
    VarApp { region: Region, name: &'a str, args: &'a [&'a Located<Type<'a>>] },
    /// `Int`, `int`, `List 'a`, `option int`.
    Type { region: Region, name: &'a str, args: &'a [&'a Located<Type<'a>>] },
    TypeQual { region: Region, module: &'a str, name: &'a str, args: &'a [&'a Located<Type<'a>>] },
    Record(&'a [&'a FieldType<'a>]),
    Unit,
    Tuple { first: &'a Located<Type<'a>>, second: &'a Located<Type<'a>>, rest: &'a [&'a Located<Type<'a>>] },
}

/// A type binder in a declaration head: `'a` or `('f : Big -> Big)`.
#[derive(Debug)]
pub struct TypeParam<'a> {
    pub name: &'a Located<&'a str>,
    pub kind: Option<&'a Located<Kind<'a>>>,
}

/// Base kinds are a closed set; the parser rejects any other name.
#[derive(Debug)]
pub enum Kind<'a> {
    Big,
    Const,
    Term,
    Storable,
    Arrow { from: &'a Located<Kind<'a>>, to: &'a Located<Kind<'a>> },
}

pub struct Union<'a> {
    pub name: &'a Located<&'a str>,
    pub arguments: &'a [&'a TypeParam<'a>],
    pub ctors: &'a [&'a Ctor<'a>],
}
// Alias::arguments likewise.

/// `Done 'a` (positional) or `Datum { owner : Bytes, deadline : Int }` (labeled).
#[derive(Debug)]
pub struct Ctor<'a> {
    pub name: &'a Located<&'a str>,
    pub arguments: CtorArgs<'a>,
}

/// Labeled fields are encoded flat, exactly like positional arguments.
#[derive(Debug)]
pub enum CtorArgs<'a> {
    Positional(&'a [&'a Located<Type<'a>>]),
    Labeled(&'a [(&'a Located<&'a str>, &'a Located<Type<'a>>)]),
}
```

```rust
// nash-source/src/lib.rs
pub enum Exposed<'a> {
    Lower(&'a Located<&'a str>),
    Upper { name: &'a Located<&'a str>, privacy: Privacy },
    /// `type option(..)`: a little type, prefixed to disambiguate from a value.
    LowerType { name: &'a Located<&'a str>, privacy: Privacy },
    Operator { region: Region, op: &'a str },
}
```

```rust
// nash-parse/src/exposing.rs — fourth alternative in chomp_exposed
Box::new(|p: &mut Parser<'a>| {
    p.keyword_type(error::Exposing::Value)?;
    p.chomp_and_check_indent(error::Exposing::Space, error::Exposing::TypeName)?;
    let name_start = p.get_position();
    let name = p.lower_name(error::Exposing::TypeName)?;
    let located = p.add_end(name_start, name);
    p.chomp_and_check_indent(error::Exposing::Space, error::Exposing::IndentEnd)?;
    let privacy = p.privacy()?;
    Ok(p.alloc(Exposed::LowerType { name: located, privacy }))
}),
// error.rs, enum Exposing: `TypeName(Row, Col),`
```

```rust
// nash-parse/src/error.rs
pub enum Type<'a> {
    Record(&'a TRecord<'a>, Row, Col),
    Tuple(&'a TTuple<'a>, Row, Col),
    Start(Row, Col),
    /// `'` not followed by a lowercase name.
    VarStart(Row, Col),
    Space(Space, Row, Col),
    IndentStart(Row, Col),
}

pub enum TypeParam<'a> {
    Start(Row, Col),
    Colon(Row, Col),
    Kind(&'a Kind<'a>, Row, Col),
    End(Row, Col),
    Space(Space, Row, Col),
    IndentColon(Row, Col),
    IndentKind(Row, Col),
    IndentEnd(Row, Col),
}

pub enum Kind<'a> {
    Start(Row, Col),
    /// An uppercase name that is not `Big`, `Const`, `Term` or `Storable`.
    Name(&'a str, Row, Col),
    End(Row, Col),
    Space(Space, Row, Col),
    IndentStart(Row, Col),
    Paren(&'a Kind<'a>, Row, Col),
}

// CustomType and TypeAlias each gain:
//     Param(&'a TypeParam<'a>, Row, Col),
// CustomType also gains, for named constructor fields:
//     Field(Row, Col), FieldColon(Row, Col), FieldType(&'a Type<'a>, Row, Col),
//     FieldEnd(Row, Col), IndentField(Row, Col), IndentFieldType(Row, Col),
```

```rust
// nash-parse/src/expression/variable.rs
/// Parse `'a` and return `a`; fails without consuming unless `'` + lowercase.
pub(crate) fn type_var_name<E>(&mut self, to_error: impl FnOnce(u16, u16) -> E) -> Result<&'a str, E> {
    let (row, col) = self.position();
    if self.peek() != Some(b'\'') || !matches!(self.peek_at(1), Some(b) if b.is_ascii_lowercase()) {
        return Err(to_error(row, col));
    }
    self.advance();
    let start_pos = self.pos;
    self.advance();
    self.chomp_inner_chars();
    Ok(self.slice_from(start_pos))
}

/// Parse a type-declaration name: `Foo` (Big) or `foo` (little).
pub(crate) fn type_decl_name<E>(&mut self, to_error: impl FnOnce(u16, u16) -> E) -> Result<&'a str, E> {
    match self.peek() {
        Some(b) if b.is_ascii_uppercase() => self.upper_name(to_error),
        _ => self.lower_name(to_error),
    }
}
```

```rust
// nash-parse/src/type_.rs
enum TypeName<'a> { Unqualified(&'a str), Qualified(&'a str, &'a str) }

/// Parse `Int`, `int`, or `Data.Map`.
fn type_name<E>(&mut self, to_error: impl FnOnce(u16, u16) -> E) -> Result<TypeName<'a>, E> {
    let (row, col) = self.position();
    let start_pos = self.pos;
    match self.peek() {
        Some(b) if b.is_ascii_lowercase() => Ok(TypeName::Unqualified(self.lower_name(to_error)?)),
        Some(b) if b.is_ascii_uppercase() => {
            self.advance();
            self.chomp_inner_chars();
            if self.is_dot_upper() {
                self.chomp_qualified_upper_for_type(start_pos)
            } else if self.is_dot_lower() {
                Err(to_error(row, col))
            } else {
                Ok(TypeName::Unqualified(self.slice_from(start_pos)))
            }
        }
        _ => Err(to_error(row, col)),
    }
}

fn type_app(&mut self, start: Position) -> Result<(&'a Located<Type<'a>>, Position), error::Type<'a>> {
    let head = self.one_of(
        error::Type::Start,
        vec![
            Box::new(|p: &mut Parser<'a>| p.type_name(error::Type::Start).map(Head::Name)),
            Box::new(|p: &mut Parser<'a>| p.type_var_name(error::Type::Start).map(Head::Var)),
        ],
    )?;
    let head_end = self.get_position();
    self.chomp(error::Type::Space)?;
    let (args, end) = self.type_chomp_args(head_end)?;
    let region = Region::new(start, head_end);
    let tipe = match head {
        Head::Name(TypeName::Unqualified(name)) => Type::Type { region, name, args },
        Head::Name(TypeName::Qualified(module, name)) => Type::TypeQual { region, module, name, args },
        Head::Var(name) if args.is_empty() => Type::Var(name),
        Head::Var(name) => Type::VarApp { region, name, args },
    };
    Ok((self.alloc(Located::at(Region::new(start, end), tipe)), end))
}

enum Head<'a> { Name(TypeName<'a>), Var(&'a str) }
```

```rust
// type_term: replace the "Type variable" alternative (type_.rs:191)
Box::new(|p: &mut Parser<'a>| {
    let var = p.type_var_name(error::Type::Start)?;
    Ok(p.add_end(start, Type::Var(var)))
}),
// and the named alternative uses `p.type_name(error::Type::Start)?`
```

```rust
// type_record_body: non-empty alternative (replaces type_.rs:335-377)
Box::new(|p: &mut Parser<'a>| {
    let field = p.type_record_field()?;
    let fields = p.type_record_end(field)?;
    Ok(p.add_end(start, Type::Record(fields)))
}),
```

```rust
// type_.rs — binders and kinds
/// Parse `'a` or `('f : Big -> Big)`.
pub(crate) fn type_param(&mut self) -> Result<&'a TypeParam<'a>, error::TypeParam<'a>> {
    let start = self.get_position();
    self.one_of(
        error::TypeParam::Start,
        vec![
            Box::new(|p: &mut Parser<'a>| {
                let name = p.type_var_name(error::TypeParam::Start)?;
                Ok(p.alloc(TypeParam { name: p.add_end(start, name), kind: None }))
            }),
            Box::new(|p: &mut Parser<'a>| {
                p.word1(b'(', error::TypeParam::Start)?;
                p.chomp_and_check_indent(error::TypeParam::Space, error::TypeParam::IndentColon)?;
                let name_start = p.get_position();
                let name = p.type_var_name(error::TypeParam::Start)?;
                let name = p.add_end(name_start, name);
                p.chomp_and_check_indent(error::TypeParam::Space, error::TypeParam::IndentColon)?;
                p.word1(b':', error::TypeParam::Colon)?;
                p.chomp_and_check_indent(error::TypeParam::Space, error::TypeParam::IndentKind)?;
                let (kind, end) = p.specialize(
                    |bump, e, r, c| error::TypeParam::Kind(bump.alloc(e), r, c),
                    |p| p.kind_expr(),
                )?;
                p.check_indent(end.line, end.column, error::TypeParam::IndentEnd)?;
                p.word1(b')', error::TypeParam::End)?;
                Ok(p.alloc(TypeParam { name, kind: Some(kind) }))
            }),
        ],
    )
}

/// Parse `Big`, `Big -> Big`, `(Big -> Big) -> Const`.
fn kind_expr(&mut self) -> Result<(&'a Located<Kind<'a>>, Position), error::Kind<'a>> {
    let start = self.get_position();
    let atom = self.kind_atom()?;
    let end1 = self.get_position();
    self.chomp(error::Kind::Space)?;
    self.one_of_with_fallback(
        vec![Box::new(|p: &mut Parser<'a>| {
            p.check_indent(end1.line, end1.column, error::Kind::IndentStart)?;
            p.word2(b'-', b'>', error::Kind::Start)?;
            p.chomp_and_check_indent(error::Kind::Space, error::Kind::IndentStart)?;
            let (to, end2) = p.kind_expr()?;
            Ok((p.alloc(Located::at(Region::new(start, end2), Kind::Arrow { from: atom, to })), end2))
        })],
        (atom, end1),
    )
}

fn kind_atom(&mut self) -> Result<&'a Located<Kind<'a>>, error::Kind<'a>> {
    let start = self.get_position();
    self.one_of(
        error::Kind::Start,
        vec![
            Box::new(|p: &mut Parser<'a>| {
                let (row, col) = p.position();
                let name = p.upper_name(error::Kind::Start)?;
                let kind = match name {
                    "Big" => Kind::Big,
                    "Const" => Kind::Const,
                    "Term" => Kind::Term,
                    "Storable" => Kind::Storable,
                    other => return Err(error::Kind::Name(other, row, col)),
                };
                Ok(p.add_end(start, kind))
            }),
            Box::new(|p: &mut Parser<'a>| {
                p.in_context(
                    |bump, e, r, c| error::Kind::Paren(bump.alloc(e), r, c),
                    |p| p.word1(b'(', error::Kind::Start),
                    |p| {
                        p.chomp_and_check_indent(error::Kind::Space, error::Kind::IndentStart)?;
                        let (kind, end) = p.kind_expr()?;
                        p.check_indent(end.line, end.column, error::Kind::End)?;
                        p.word1(b')', error::Kind::End)?;
                        Ok(kind)
                    },
                )
            }),
        ],
    )
}
```

```rust
// union.rs — variant: positional or labeled fields
fn variant(&mut self) -> Result<(&'a Ctor<'a>, Position), CustomType<'a>> {
    let name_start = self.get_position();
    let name_str = self.upper_name(CustomType::Variant)?;
    let name = self.add_end(name_start, name_str);
    let name_end = self.get_position();
    self.chomp(CustomType::Space)?;
    let (arguments, end) = if self.peek() == Some(b'{') {
        self.check_indent(name_end.line, name_end.column, CustomType::IndentField)?;
        self.advance();
        self.chomp_and_check_indent(CustomType::Space, CustomType::IndentField)?;
        let first = self.ctor_field()?;
        let fields = self.ctor_fields_end(first)?;
        let end = self.get_position();
        self.chomp(CustomType::Space)?;
        (CtorArgs::Labeled(fields), end)
    } else {
        let (args, end) = self.specialize(
            |bump, e, row, col| CustomType::VariantArg(bump.alloc(e), row, col),
            |p| p.chomp_variant_args(name_end),
        )?;
        (CtorArgs::Positional(args), end)
    };
    Ok((self.alloc(Ctor { name, arguments }), end))
}
// ctor_field returns `(&'a Located<&'a str>, &'a Located<Type<'a>>)`;
// ctor_fields_end collects them into `&'a [(..)]`. Both mirror
// type_record_field / type_record_end (type_.rs:391, :410) with
// CustomType::{Field, FieldColon, FieldType, FieldEnd, IndentField,
// IndentFieldType} in place of the TRecord variants.

// union.rs — chomp_custom_name_to_equals_help, parameter alternative
Box::new(|p: &mut Parser<'a>| {
    let param = p.specialize(
        |bump, e, r, c| CustomType::Param(bump.alloc(e), r, c),
        |p| p.type_param(),
    )?;
    p.chomp_and_check_indent(CustomType::Space, CustomType::IndentEquals)?;
    args.push(param);
    Ok(CustomNameState::MoreArgs)
}),
// chomp_custom_name_to_equals: `let name_str = self.type_decl_name(CustomType::Name)?;`
// type_alias.rs: same two edits with TypeAlias::Param / TypeAlias::Name.
```

```rust
// nash-can/src/error.rs
Unsupported { feature: &'static str, region: Region },

// nash-can/src/types.rs, canonicalize_type
SourceType::VarApp { region: r, .. } => {
    return Err(vec![Error::Unsupported { feature: "higher-kinded type application", region: *r }]);
}
SourceType::Record(fields) => { /* as before */ CanType::Record { fields: can_fields, ext: None } }
```

### Elm reference

`Parse/Type.hs` (`term`, `app`, `chompArgs`, `record`), `Parse/Variable.hs`
(`foreignUpper`, `lower`), `Parse/Declaration.hs`
(`chompCustomNameToEqualsHelp`, `chompAliasNameToEqualsHelp`).

### Tests

- `type_.rs` (rewrite existing inputs `a`→`'a`, `msg`→`'msg`, `Maybe a`→
  `Maybe 'a`, `{ onClick : msg -> Cmd msg }`→`'msg`; delete
  `record_extension*`): `"'a"`, `"int"`, `"list int"`, `"'f 'a"`,
  `"option 'a -> 'a"`, `"Map 'k (List 'v)"`, `"{ x : int, y : Int }"`;
  errors (new `assert_type_error_snapshot!`): `"'"`, `"'A"`, `"{ r | x : int }"`,
  `"a"` is *not* an error (it is a little type named `a`; snapshot it).
- `union.rs`: `"type option 'a = Some 'a | None"`,
  `"type Fix ('f : Big -> Big) = Fix ('f (Fix 'f))"`,
  `"type step 'a = Done 'a | Next int 'a"`,
  `"type Datum = Datum { owner : Bytes, deadline : Int }"`,
  multiline `type Shape = Circle { r : int } | Rect { w : int, h : int }`;
  errors (new `assert_decl_error_snapshot!`): `"type Maybe a = Just a"`,
  `"type T ('f : ) = A"`, `"type T ('f : Foo) = A"` (Kind::Name),
  `"type D = D { owner }"`, `"type D = D int { x : int }"` parses (record
  term) and is a nash-can error, not a parse error.
- `type_alias.rs`: `"type alias acc = { total : int, seen : list Int }"`,
  `"type alias Pair 'a 'b = ('a, 'b)"`; error `"type alias = int"`.
- `exposing.rs`: `"(type option(..))"`, `"(type step, map)"`,
  `"(type option(..), Data(..), (+))"`; errors (new
  `assert_exposing_error_snapshot!`): `"(type Foo)"`, `"(type)"`,
  `"(type option(..)"`.
- `import.rs`: `"import Prelude exposing (type option(..), map)\n"`.
- `module.rs`: header `"module P exposing (type option(..), type step)"`.
- nash-can: rerun `cargo insta test -p nash-can --accept` after rewriting
  fixtures; new test `type_var_app_unsupported` asserting
  `Error::Unsupported`; a module exposing `type option(..)` with
  `type option 'a = Some 'a | None` produces the same interface as the
  uppercase equivalent.

### Done when

`cargo test` passes across the workspace with every fixture using `'a`;
`grep -rn "ext:" crates/nash-source` returns nothing; `scratch/src/*.nash`
parses with `nash check scratch`.

---

## Chunk 2 — Constraints and annotations

### Files

- `crates/nash-source/src/lib.rs`
- `crates/nash-parse/src/type_.rs`
- `crates/nash-parse/src/declaration/value.rs`
- `crates/nash-parse/src/expression/let_.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-can/src/{module.rs,expression.rs,types.rs}`

### Change

1. **AST.** `Annotation { constraints, typ }` and `Constraint { class,
   args }`. `Value.annotation` (`lib.rs:27`) and `Def::Define.annotation`
   (`lib.rs:156`) become `Option<&'a Annotation<'a>>`.
2. **Parser.** New `type_scheme()` in `type_.rs`: parse a `type_expr`; if
   the next token is `=>`, reinterpret the parsed type as the context and
   parse the real type. Reinterpretation (`to_constraints`) accepts
   `Type::Type`/`TypeQual` with one or more args as one constraint and a
   `Type::Tuple` of such as several; anything else is
   `error::Type::Context`. This is how GHC parses contexts and avoids
   backtracking. `value_decl` (`value.rs:49`–`52`) and `definition`
   (`let_.rs:151`–`154`) call `type_scheme` instead of `type_expr`.
3. **nash-can.** `module.rs:251` and `expression.rs:1015` pass
   `ann.typ` to `types::to_annotation`; a non-empty `ann.constraints`
   returns `Error::Unsupported { feature: "constraints" }` until the traits
   plan.

### Code

```rust
// nash-source/src/lib.rs
/// `Eq 'a => 'a -> 'a -> bool`
#[derive(Debug)]
pub struct Annotation<'a> {
    pub constraints: &'a [&'a Located<Constraint<'a>>],
    pub typ: &'a Located<Type<'a>>,
}

/// `Ord 'a`, `Lift 'small 'big`, `Cardano.Eq Datum`
#[derive(Debug)]
pub struct Constraint<'a> {
    pub class: &'a Located<&'a str>,
    pub module: Option<&'a str>,
    pub args: &'a [&'a Located<Type<'a>>],
}
```

```rust
// error.rs, in enum Type
/// The thing before `=>` is not a constraint or tuple of constraints.
Context(Row, Col),
/// `=>` followed by nothing.
IndentAfterContext(Row, Col),
```

```rust
// type_.rs
/// Parse `[context =>] type`.
pub fn type_scheme(&mut self) -> Result<(&'a Annotation<'a>, Position), error::Type<'a>> {
    let start = self.get_position();
    let (first, end1) = self.type_expr()?;
    self.one_of_with_fallback(
        vec![Box::new(|p: &mut Parser<'a>| {
            p.check_indent(end1.line, end1.column, error::Type::IndentStart)?;
            p.word2(b'=', b'>', error::Type::Start)?;
            let constraints = p.to_constraints(first, start)?;
            p.chomp_and_check_indent(error::Type::Space, error::Type::IndentAfterContext)?;
            let (typ, end2) = p.type_expr()?;
            Ok((p.alloc(Annotation { constraints, typ }), end2))
        })],
        (self.alloc(Annotation { constraints: &[], typ: first }), end1),
    )
}

/// Reinterpret an already-parsed type as a context.
fn to_constraints(
    &self,
    typ: &'a Located<Type<'a>>,
    start: Position,
) -> Result<&'a [&'a Located<Constraint<'a>>], error::Type<'a>> {
    let bad = || error::Type::Context(start.line, start.column);
    match &typ.value {
        Type::Tuple { first, second, rest } => {
            let mut out: BumpVec<'a, &'a Located<Constraint<'a>>> = BumpVec::new_in(self.bump);
            for t in [*first, *second].into_iter().chain(rest.iter().copied()) {
                out.push(self.to_constraint(t).ok_or_else(bad)?);
            }
            Ok(out.into_bump_slice())
        }
        _ => Ok(self.alloc_slice_copy(&[self.to_constraint(typ).ok_or_else(bad)?])),
    }
}

fn to_constraint(&self, typ: &'a Located<Type<'a>>) -> Option<&'a Located<Constraint<'a>>> {
    let (region, module, name, args) = match &typ.value {
        Type::Type { region, name, args } if !args.is_empty() => (*region, None, *name, *args),
        Type::TypeQual { region, module, name, args } if !args.is_empty() => (*region, Some(*module), *name, *args),
        _ => return None,
    };
    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let class = self.alloc(Located::at(region, name));
    Some(self.alloc(Located::at(typ.region, Constraint { class, module, args })))
}
```

```rust
// value.rs, annotation alternative
let (annotation, _) = p.specialize(
    |bump, e, row, col| DeclDef::Type(bump.alloc(e), row, col),
    |p| p.type_scheme(),
)?;
p.check_fresh_line(DeclDef::NameRepeat)?;
let def_name = p.chomp_matching_name_decl(name)?;
p.chomp_and_check_indent(DeclDef::Space, DeclDef::IndentEquals)?;
p.chomp_value_args_and_body(maybe_docs, start, def_name, Some(annotation))
// chomp_value_args_and_body's `type_ann: Option<&'a Annotation<'a>>`; same in let_.rs.
```

### Elm reference

`Parse/Type.hs` (`expression`), `Parse/Declaration.hs` (`valueDecl`),
`Parse/Expression.hs` (`definition`). No Elm counterpart for contexts; GHC's
`checkContext` in `GHC/Parser/PostProcess.hs` is the model.

### Tests

- `type_.rs` (new `assert_scheme_snapshot!` calling `type_scheme`):
  `"Eq 'a => 'a -> 'a -> bool"`, `"(Eq 'a, Show 'b) => 'a -> 'b -> string"`,
  `"Lift 'small 'big => 'small -> 'big"`, `"Cardano.Eq Datum => Datum -> bool"`,
  `"'a -> 'a"` (empty constraints); errors: `"'a => 'a"`, `"Eq => 'a"`,
  `"(Eq 'a, 'b) => 'a"`, `"int 'a => 'a"`.
- `value.rs`: `"max : Ord 'a => 'a -> 'a -> 'a\nmax a b = a"`.
- `let_.rs`: indented `let f : Eq 'a => 'a -> bool\n    f x = x == x in f 1`.
- nash-can: `constraints_unsupported` test.

### Done when

Annotations round-trip through nash-can unchanged when `constraints` is
empty; all snapshot tests pass.

---

## Chunk 3 — Bytes literals

### Files

- `crates/nash-parse/src/bytes.rs` (new)
- `crates/nash-parse/src/expression/bytes.rs` (new)
- `crates/nash-parse/src/expression/mod.rs`
- `crates/nash-parse/src/pattern/term.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/{expression.rs,pattern.rs}`

### Change

`#"ff00"` in expressions and patterns. The primitive `bytes_literal`
decodes hex into an arena slice (`bump.alloc_slice_fill_iter`). `term`
(`expression/mod.rs:289`) and `pattern_term_help` (`pattern/term.rs:17`) get
a new alternative before `string`. nash-can maps `Expr::Bytes`/
`Pattern::Bytes` to `Error::Unsupported { feature: "bytes literal" }` (the
literal traits land with the stdlib plan).

### Code

```rust
// nash-source: Expr::Bytes(&'a [u8]) and Pattern::Bytes(&'a [u8])

// error.rs
pub enum Bytes { Endless, OddLength, BadHexDigit(u16) }
// Expr::Bytes(Bytes, Row, Col) and Pattern::Bytes(Bytes, Row, Col)

// bytes.rs
impl<'a> Parser<'a> {
    /// Parse `#"..."` with an even number of hex digits into decoded bytes.
    pub fn bytes_literal<E>(
        &mut self,
        to_expectation: impl FnOnce(Row, Col) -> E,
        to_error: impl FnOnce(error::Bytes, Row, Col) -> E,
    ) -> Result<&'a [u8], E> {
        let (row, col) = self.position();
        if self.peek() != Some(b'#') || self.peek_at(1) != Some(b'"') {
            return Err(to_expectation(row, col));
        }
        self.advance_by(2);
        let start_pos = self.pos;
        loop {
            match self.peek() {
                None | Some(b'\n') => return Err(to_error(error::Bytes::Endless, self.row(), self.col())),
                Some(b'"') => break,
                Some(b) if b.is_ascii_hexdigit() => self.advance(),
                Some(_) => return Err(to_error(error::Bytes::BadHexDigit(self.col()), self.row(), self.col())),
            }
        }
        let hex = &self.src[start_pos..self.pos];
        self.advance();
        if hex.len() % 2 != 0 {
            return Err(to_error(error::Bytes::OddLength, row, col));
        }
        Ok(self.bump.alloc_slice_fill_iter(hex.chunks(2).map(|pair| {
            (hex_value(pair[0]) << 4) | hex_value(pair[1])
        })))
    }
}
// `hex_value` moves from number.rs into a shared `pub(crate) fn` in bytes.rs.
```

```rust
// expression/bytes.rs
pub(crate) fn bytes(&mut self, start: Position) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
    let bs = self.bytes_literal(error::Expr::Start, error::Expr::Bytes)?;
    Ok(self.add_end(start, Expr::Bytes(bs)))
}
// term(): `Box::new(|p| p.bytes(start)),` before `p.string(start)`.
// pattern/term.rs: `pattern_bytes` mirrors pattern_string (term.rs:179).
```

### Elm / Aiken reference

`Parse/String.hs` (`string`) for the primitive shape;
`aiken/crates/aiken-lang/src/parser/literal/bytearray.rs` for hex decoding
rules.

### Tests

- `expression/bytes.rs`: `assert_expr_snapshot!("#\"\"")`, `"#\"ff00\""`,
  `"#\"DEADbeef\""`; errors `"#\"f\""`, `"#\"zz\""`, `"#\"ab"`.
- `expression/mod.rs`: `assert_expression_snapshot!("f #\"01\" x")`.
- `pattern/term.rs`: `assert_pattern_snapshot!("#\"00\"")`,
  `assert_pattern_error_snapshot!("#\"0\"")`.

### Done when

Bytes literals snapshot as `Bytes([255, 0])`; odd length and non-hex report
`Bytes(OddLength/BadHexDigit)`.

---

## Chunk 4 — Keyword expressions: `assert`, `fail`, `todo`, `trace`, `comptime`

### Files

- `crates/nash-parse/src/expression/keyword.rs` (new)
- `crates/nash-parse/src/expression/mod.rs`
- `crates/nash-parse/src/keyword.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/expression.rs`

### Change

Five prefix expressions parsed exactly where `let`/`if`/`case`/lambda are
parsed: the `expression` alternatives (`expression/mod.rs:46`–`65`) and the
"final expression after operator" group (`expression/mod.rs:170`–`180`).
Each is an `in_context` on its keyword so errors nest like `Expr::If`.
Message terms for `fail`/`todo` are optional and parsed with
`one_of_with_fallback` after `chomp` + `check_indent`.

### Code

```rust
// nash-source
Assert(&'a Located<Expr<'a>>),
Fail(Option<&'a Located<Expr<'a>>>),
Todo(Option<&'a Located<Expr<'a>>>),
Trace { message: &'a Located<Expr<'a>>, body: &'a Located<Expr<'a>> },
Comptime(&'a Located<Expr<'a>>),
```

```rust
// error.rs, in enum Expr
Assert(&'a Keyword<'a>, Row, Col),
Fail(&'a Keyword<'a>, Row, Col),
Todo(&'a Keyword<'a>, Row, Col),
Trace(&'a Keyword<'a>, Row, Col),
Comptime(&'a Keyword<'a>, Row, Col),

/// Errors inside a keyword-prefixed expression.
pub enum Keyword<'a> {
    Space(Space, Row, Col),
    Body(&'a Expr<'a>, Row, Col),
    Message(&'a Expr<'a>, Row, Col),
    IndentBody(Row, Col),
    IndentMessage(Row, Col),
}
```

```rust
// keyword.rs: keyword_assert, keyword_fail, keyword_todo, keyword_trace,
// keyword_comptime — each `self.keyword(b"assert", to_error)` etc.

// expression/keyword.rs
impl<'a> Parser<'a> {
    /// `assert expr`, `comptime expr`: keyword followed by a full expression.
    fn keyword_body(
        &mut self,
        start: Position,
        wrap: fn(&'a error::Keyword<'a>, Row, Col) -> error::Expr<'a>,
        kw: fn(&mut Self, fn(Row, Col) -> error::Expr<'a>) -> Result<(), error::Expr<'a>>,
        build: fn(&'a Located<Expr<'a>>) -> Expr<'a>,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            move |bump, e, r, c| wrap(bump.alloc(e), r, c),
            |p| kw(p, error::Expr::Start),
            |p| {
                p.chomp_and_check_indent(error::Keyword::Space, error::Keyword::IndentBody)?;
                let (body, end) = p.specialize(
                    |bump, e, r, c| error::Keyword::Body(bump.alloc(e), r, c),
                    |p| p.expression(),
                )?;
                Ok((p.alloc(Located::at(Region::new(start, end), build(body))), end))
            },
        )
    }

    pub(crate) fn assert_(&mut self, start: Position) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_body(start, error::Expr::Assert, Parser::keyword_assert, Expr::Assert)
    }

    pub(crate) fn comptime(&mut self, start: Position) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_body(start, error::Expr::Comptime, Parser::keyword_comptime, Expr::Comptime)
    }

    /// `fail`, `fail "msg"`, `todo`, `todo "msg"`.
    fn keyword_message(
        &mut self,
        start: Position,
        wrap: fn(&'a error::Keyword<'a>, Row, Col) -> error::Expr<'a>,
        kw: fn(&mut Self, fn(Row, Col) -> error::Expr<'a>) -> Result<(), error::Expr<'a>>,
        build: fn(Option<&'a Located<Expr<'a>>>) -> Expr<'a>,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            move |bump, e, r, c| wrap(bump.alloc(e), r, c),
            |p| kw(p, error::Expr::Start),
            |p| {
                let kw_end = p.get_position();
                p.chomp(error::Keyword::Space)?;
                let message = p.one_of_with_fallback(
                    vec![Box::new(|p: &mut Parser<'a>| {
                        let (row, col) = p.position();
                        p.check_indent(row, col, error::Keyword::IndentMessage)?;
                        let term = p.specialize(
                            |bump, e, r, c| error::Keyword::Message(bump.alloc(e), r, c),
                            |p| p.term(),
                        )?;
                        Ok(Some(term))
                    })],
                    None,
                )?;
                let end = message.map_or(kw_end, |m| m.region.end);
                Ok((p.alloc(Located::at(Region::new(start, end), build(message))), end))
            },
        )
    }

    pub(crate) fn fail(&mut self, start: Position) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_message(start, error::Expr::Fail, Parser::keyword_fail, Expr::Fail)
    }

    pub(crate) fn todo(&mut self, start: Position) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_message(start, error::Expr::Todo, Parser::keyword_todo, Expr::Todo)
    }

    /// `trace term expr`
    pub(crate) fn trace(&mut self, start: Position) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            |bump, e, r, c| error::Expr::Trace(bump.alloc(e), r, c),
            |p| p.keyword_trace(error::Expr::Start),
            |p| {
                p.chomp_and_check_indent(error::Keyword::Space, error::Keyword::IndentMessage)?;
                let message = p.specialize(
                    |bump, e, r, c| error::Keyword::Message(bump.alloc(e), r, c),
                    |p| p.term(),
                )?;
                p.chomp_and_check_indent(error::Keyword::Space, error::Keyword::IndentBody)?;
                let (body, end) = p.specialize(
                    |bump, e, r, c| error::Keyword::Body(bump.alloc(e), r, c),
                    |p| p.expression(),
                )?;
                Ok((p.alloc(Located::at(Region::new(start, end), Expr::Trace { message, body })), end))
            },
        )
    }
}
```

`expression()` and the final-expression `one_of` in `chomp_expr_end` each
gain five boxed alternatives (`p.assert_(start)`, `p.fail(start)`,
`p.todo(start)`, `p.trace(start)`, `p.comptime(start)`) after `p.lambda`.
The `fn` pointer parameters keep `keyword_body`/`keyword_message` monomorphic
and avoid boxing; if the borrow checker rejects the `kw` pointer signature,
inline the five `in_context` calls instead.

### Elm / Aiken reference

`Parse/Expression.hs` (`if_`, `expression`, `chompExprEnd`);
`aiken/crates/aiken-lang/src/parser/expr/{fail_todo_trace.rs}`.

### Tests

`expression/keyword.rs`: `"assert (x > 0)"`, `"assert x == y"`, `"fail"`,
`"fail \"boom\""`, `"todo"`, `"trace \"m\" (f x)"`, `"trace \"m\" x + 1"`,
`"comptime (fib 20)"`, `"a + fail"`, indented case with `Cancel -> fail` on
one branch and another branch below (message must not swallow the next
branch); errors: `"assert"`, `"trace \"m\""`, `"comptime"`.

### Done when

`trace "m" x + 1` snapshots as `Trace { body: BinOps .. }`; a case branch
`-> fail` followed by an aligned branch parses `Fail(None)`.

---

## Chunk 5 — `do` blocks

### Files

- `crates/nash-parse/src/expression/do_.rs` (new)
- `crates/nash-parse/src/expression/mod.rs`
- `crates/nash-parse/src/keyword.rs` (`keyword_do`)
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/expression.rs`

### Change

`do` opens an aligned block like `case ... of` (`case.rs:55`–`70`). A
statement is a `let` statement, `pattern <- expr`, or `expr`. The `let`
form reuses the def block of `let_` (`let_.rs:40`–`46`) and then looks for
`in`: present means the line is an ordinary let *expression* statement,
absent means a `Stmt::Let` whose scope is the rest of the block. Because a
pattern prefix is also a valid expression prefix (`Just x <- e` vs `Just x`),
the bind form is tried with an explicit `save_state`/`restore_state`
(`lib.rs:172`, `:183`) when `<-` is absent; this is the only backtracking
point added by this plan. The last statement must be an expression; a
trailing bind or `let` is `Do::LastNotExpr`.

### Code

```rust
// nash-source
Do { stmts: &'a [&'a Located<Stmt<'a>>], last: &'a Located<Expr<'a>> },

#[derive(Debug)]
pub enum Stmt<'a> {
    /// `let` defs with no `in`; scope is the rest of the block.
    Let(&'a [&'a Located<Def<'a>>]),
    Bind { pattern: &'a Located<Pattern<'a>>, expr: &'a Located<Expr<'a>> },
    Expr(&'a Located<Expr<'a>>),
}
```

```rust
// error.rs
// Expr::Do(&'a Do<'a>, Row, Col)
pub enum Do<'a> {
    Space(Space, Row, Col),
    Let(&'a Let<'a>, Row, Col),
    Pattern(&'a Pattern<'a>, Row, Col),
    Arrow(Row, Col),
    Expr(&'a Expr<'a>, Row, Col),
    /// The block ends with `pat <- e` or a `let` statement.
    LastNotExpr(Row, Col),
    IndentStmt(Row, Col),
    IndentArrow(Row, Col),
    IndentExpr(Row, Col),
    Alignment(u16, Row, Col),
}
```

```rust
// expression/do_.rs
impl<'a> Parser<'a> {
    /// `do` followed by an aligned block of statements.
    pub(crate) fn do_(&mut self, start: Position) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            |bump, e, r, c| error::Expr::Do(bump.alloc(e), r, c),
            |p| p.keyword_do(error::Expr::Start),
            |p| {
                p.chomp_and_check_indent(Do::Space, Do::IndentStmt)?;
                let (stmts, last, end) = p.with_indent(|p| p.do_body())?;
                Ok((p.alloc(Located::at(Region::new(start, end), Expr::Do { stmts, last })), end))
            },
        )
    }

    /// Aligned statements ending in an expression; shared with test bodies (chunk 11).
    pub(crate) fn do_body(
        &mut self,
    ) -> Result<(&'a [&'a Located<Stmt<'a>>], &'a Located<Expr<'a>>, Position), Do<'a>> {
        let (first, first_end) = self.do_stmt()?;
        let (stmts, end) = self.chomp_do_stmts(vec![first], first_end)?;
        let (last, init) = stmts.split_last().expect("at least one statement");
        let last = match last.value {
            Stmt::Expr(e) => e,
            Stmt::Bind { .. } | Stmt::Let(_) => {
                return Err(Do::LastNotExpr(last.region.start.line, last.region.start.column));
            }
        };
        Ok((self.alloc_slice_copy(init), last, end))
    }

    fn chomp_do_stmts(
        &mut self,
        mut stmts: Vec<&'a Located<Stmt<'a>>>,
        end: Position,
    ) -> Result<(Vec<&'a Located<Stmt<'a>>>, Position), Do<'a>> {
        let fallback = stmts.clone();
        self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.check_aligned(Do::Alignment)?;
                let (stmt, new_end) = p.do_stmt()?;
                stmts.push(stmt);
                p.chomp_do_stmts(stmts, new_end)
            })],
            (fallback, end),
        )
    }

    /// One statement: `let` block, `pattern <- e` (with backtracking), or an expression.
    fn do_stmt(&mut self) -> Result<(&'a Located<Stmt<'a>>, Position), Do<'a>> {
        let start = self.get_position();
        if let Some(stmt) = self.do_let_stmt(start)? {
            return Ok(stmt);
        }
        let saved = self.save_state();
        if let Ok((pattern, pat_end)) = self.pattern_expr()
            && self.check_indent(pat_end.line, pat_end.column, ()).is_ok()
            && self.word2(b'<', b'-', ()).is_ok()
        {
            self.chomp_and_check_indent(Do::Space, Do::IndentExpr)?;
            let (expr, end) = self.specialize(|bump, e, r, c| Do::Expr(bump.alloc(e), r, c), |p| p.expression())?;
            let stmt = Stmt::Bind { pattern, expr };
            return Ok((self.alloc(Located::at(Region::new(start, end), stmt)), end));
        }
        self.restore_state(saved);
        let (expr, end) = self.specialize(|bump, e, r, c| Do::Expr(bump.alloc(e), r, c), |p| p.expression())?;
        Ok((self.alloc(Located::at(Region::new(start, end), Stmt::Expr(expr))), end))
    }

    /// `let defs` as a statement, or `let defs in body` as an expression statement.
    fn do_let_stmt(&mut self, start: Position) -> Result<Option<(&'a Located<Stmt<'a>>, Position)>, Do<'a>> {
        if self.keyword_let(|_, _| ()).is_err() {
            return Ok(None);
        }
        let (defs, defs_end) = self.specialize(
            |bump, e, r, c| Do::Let(bump.alloc(e), r, c),
            |p| p.with_backset_indent(3, |p| {
                p.chomp_and_check_indent(Let::Space, Let::IndentDef)?;
                p.with_indent(|p| {
                    let (first, first_end) = p.chomp_let_def()?;
                    p.chomp_let_defs(vec![first], first_end)
                })
            }),
        )?;
        let has_in = self.check_indent(defs_end.line, defs_end.column, ()).is_ok()
            && self.keyword_in(|_, _| ()).is_ok();
        let defs = self.alloc_slice_copy(&defs);
        if !has_in {
            return Ok(Some((self.alloc(Located::at(Region::new(start, defs_end), Stmt::Let(defs))), defs_end)));
        }
        self.chomp_and_check_indent(Do::Space, Do::IndentExpr)?;
        let (body, end) = self.specialize(|bump, e, r, c| Do::Expr(bump.alloc(e), r, c), |p| p.expression())?;
        let let_expr = self.alloc(Located::at(Region::new(start, end), Expr::Let { defs, body }));
        Ok(Some((self.alloc(Located::at(Region::new(start, end), Stmt::Expr(let_expr))), end)))
    }
}
```

`chomp_let_def` and `chomp_let_defs` (`let_.rs:87`, `:120`) become
`pub(crate)`. `keyword_in` on the aligned next statement fails because
`check_indent` requires a column past the block indent, so a statement
named `in`-something cannot be misread (and `in` is reserved anyway).

`pattern_expr` (`pattern/mod.rs:62`) chomps trailing space, so after a
successful pattern the parser sits on `<-` or on whatever follows; the
`check_indent` guard keeps a pattern on one line from claiming a `<-` on the
next aligned line. `expression()` and the final-expression group add
`Box::new(|p| p.do_(start))`.

### Elm reference

`Parse/Expression.hs` (`case_`, `chompCaseEnd`, `chompBranch`) for the
aligned-block shape; `Parse/Pattern.hs` (`expression`).

### Tests

`expression/do_.rs` (all `assert_indented_expression_snapshot!`):

```
do
    x <- fuzz int
    label "small"
    assert (x < 100)
```

`do\n    pure 1` (single expression), `do\n    (a, b) <- pair\n    pure a`,
`do\n    Just x <- m\n    pure x`, nested `do` inside a bind, `a + do\n ...`,
the `let` statement:

```
do
    let
        twice = x * 2
        name = "n"
    label name
    assert (twice > x)
```

`do\n    let y = 1 in pure y` (let expression statement, single line);
errors: `do\n    x <- e` (LastNotExpr), `do\n    let y = 1` (LastNotExpr),
`do\n    x <- e\n  y` (Alignment), `do` alone (IndentStmt).

### Done when

Bind, `let`, and plain expression statements snapshot correctly; a trailing
bind or `let` reports `Do(LastNotExpr ..)`.

---

## Chunk 6 — Macro calls `name!(args)`

### Files

- `crates/nash-parse/src/expression/macro_.rs` (new)
- `crates/nash-parse/src/expression/mod.rs`
- `crates/nash-parse/src/expression/variable.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/expression.rs`

### Change

After `variable` (`expression/mod.rs:295`–`298`) succeeds with a lowercase
(possibly qualified) name, if the next two bytes are `!(` the term is a macro
call. Arguments are comma-separated expressions parsed like a tuple body
(`tuple.rs:tuple_body`, `chomp_tuple_end`). A macro call is still
`accessible` afterwards (`m!(x).field`).

### Code

```rust
// nash-source
MacroCall { name: &'a Located<&'a str>, module: Option<&'a str>, args: &'a [&'a Located<Expr<'a>>] },

// error.rs
// Expr::Macro(&'a Macro<'a>, Row, Col)
pub enum Macro<'a> {
    Open(Row, Col),
    Arg(&'a Expr<'a>, Row, Col),
    End(Row, Col),
    Space(Space, Row, Col),
    IndentArg(Row, Col),
    IndentEnd(Row, Col),
}
```

```rust
// expression/macro_.rs
impl<'a> Parser<'a> {
    /// If `!(` follows a lowercase variable, parse the macro call around it.
    pub(crate) fn macro_call_or_term(
        &mut self,
        start: Position,
        var: &'a Located<Expr<'a>>,
    ) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        let (name, module) = match var.value {
            Expr::Var { kind: VarType::LowVar, name } => (name, None),
            Expr::VarQual { kind: VarType::LowVar, module, name } => (name, Some(module)),
            _ => return Ok(var),
        };
        if self.peek() != Some(b'!') || self.peek_at(1) != Some(b'(') {
            return Ok(var);
        }
        let name = self.alloc(Located::at(var.region, name));
        self.in_context(
            |bump, e, r, c| error::Expr::Macro(bump.alloc(e), r, c),
            |p| p.word2(b'!', b'(', error::Expr::Start),
            |p| {
                p.chomp_and_check_indent(Macro::Space, Macro::IndentArg)?;
                let args = p.one_of(
                    Macro::Open,
                    vec![
                        Box::new(|p: &mut Parser<'a>| { p.word1(b')', Macro::Open)?; Ok(&[][..]) }),
                        Box::new(|p: &mut Parser<'a>| p.macro_args()),
                    ],
                )?;
                Ok(p.add_end(start, Expr::MacroCall { name, module, args }))
            },
        )
    }

    fn macro_args(&mut self) -> Result<&'a [&'a Located<Expr<'a>>], Macro<'a>> {
        let mut args: BumpVec<'a, &'a Located<Expr<'a>>> = BumpVec::new_in(self.bump);
        loop {
            let (arg, end) = self.specialize(|bump, e, r, c| Macro::Arg(bump.alloc(e), r, c), |p| p.expression())?;
            args.push(arg);
            self.check_indent(end.line, end.column, Macro::IndentEnd)?;
            let done = self.one_of(
                Macro::End,
                vec![
                    Box::new(|p: &mut Parser<'a>| { p.word1(b',', Macro::End)?; p.chomp_and_check_indent(Macro::Space, Macro::IndentArg)?; Ok(false) }),
                    Box::new(|p: &mut Parser<'a>| { p.word1(b')', Macro::End)?; Ok(true) }),
                ],
            )?;
            if done { return Ok(args.into_bump_slice()); }
        }
    }
}

// term(): the variable alternative becomes
Box::new(|p: &mut Parser<'a>| {
    let var = p.variable(start)?;
    let expr = p.macro_call_or_term(start, var)?;
    p.accessible(start, expr)
}),
```

### Aiken reference

No Elm counterpart. Aiken has no macros; Rust's `mac!(...)` is the surface
model. Tuple-body parsing in `Parse/Expression.hs` (`chompTupleEnd`) is the
structural model.

### Tests

`expression/macro_.rs`: `"json!({ a = 1 })"`, `"m!()"`, `"m!(1, \"two\", x)"`,
`"Cardano.Macros.address!(\"addr1\")"`, `"m!(x).field"`, `"f m!(1) y"`,
`"x ! (y)"` (binop, not macro), `"Foo!(x)"` (constructor: `!` is a binop);
errors: `"m!("`, `"m!(1,)"`, `"m!(1 2)"`.

### Done when

`m!(x)` snapshots as `MacroCall`; `x ! (y)` still snapshots as `BinOps`.

---

## Chunk 6a — Partial operator sections

### Files

- `crates/nash-source/src/lib.rs`
- `crates/nash-parse/src/expression/tuple.rs`
- `crates/nash-parse/src/error.rs`
- `crates/nash-can/src/expression.rs`
- parser and canonicalization snapshots

### Change

Extend the existing whole-operator form `(+)` with Haskell-style partial
sections:

```elm
(> 5)    -- \x -> x > 5
(5 >)    -- \x -> 5 > x
```

Keep the section explicit in the surface AST so parsing does not invent a
source-level binder:

```rust
// nash-source
LeftSection {
    left: &'a Located<Expr<'a>>,
    operator: &'a str,
},
RightSection {
    operator: &'a str,
    right: &'a Located<Expr<'a>>,
},
```

Here "left" and "right" name the supplied side of the operator. During
canonicalization, resolve the operator exactly as for `BinOps` and lower the
section to an ordinary one-argument `nash_ast::Expr::Lambda` whose body is a
`Binop`. The missing operand uses a compiler-generated local that cannot
collide with source text. No section variant survives into `nash-ast`, the
solver, macro reification, or codegen.

`tuple_body` currently recognizes only an operator immediately followed by
`)`. Extend it to distinguish:

- `(op)` — the existing `Expr::Op`;
- `(op expression)` — a right section;
- `(expression op)` — a left section;
- `(expression)` and `(expression, ...)` — the existing parenthesized and
  tuple forms.

Preserve the existing minus rule: `(-x)` and `(- x)` are negation, not a
right section, while `(-)` remains the subtraction function and `(x -)` is a
left section. Reserved operators remain errors in section position.

### Elm/Haskell reference

Elm's parser supplies the surrounding tuple/parentheses machinery but only
supports an operator as a function. Haskell 2010 Report section 3.5 defines
the partial-section meanings; retain Nash's existing Elm-style error
hierarchy and indentation checks.

### Tests

- Parser success snapshots: `(> 5)`, `(5 >)`, `(f x |> )`, `((+) 1)`,
  `(-)`, `(-1)`, and `(1 -)`.
- Parser error snapshots: a missing operand or close parenthesis and a
  reserved operator in section position.
- Canonicalization snapshots show `(> 5)` and `(5 >)` as lambdas with the
  resolved `>` function and opposite operand order.
- An application snapshot for `(> 5) 10` confirms a section remains a term.

### Done when

Both partial forms parse, canonicalize to capture-free ordinary lambdas, and
need no changes in constrain, solve, or codegen.

---

## Chunk 7 — Attributes

### Files

- `crates/nash-parse/src/declaration/attribute.rs` (new)
- `crates/nash-parse/src/declaration/mod.rs`
- `crates/nash-parse/src/declaration/{value.rs,union.rs,type_alias.rs}`
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/module.rs`

### Change

`declaration` (`declaration/mod.rs:40`–`54`) parses the doc comment, then
zero or more attributes each starting at column 1 (`check_fresh_line`
between them, as `chomp_doc_comment` does at `mod.rs:76`), then the
declaration. Attributes are stored on `Value`, `Union`, `Alias` (and on
`Trait`/`Impl` in chunks 8–9). The args list reuses `macro_args`'s shape.
nash-can returns `Error::Unsupported { feature: "attributes" }` when any
declaration carries one.

### Code

```rust
// nash-source
#[derive(Debug)]
pub struct Attribute<'a> {
    pub name: &'a Located<&'a str>,
    pub args: &'a [&'a Located<Expr<'a>>],
}
// `pub attributes: &'a [&'a Attribute<'a>],` added to Value, Union, Alias.

// error.rs
// Decl::Attribute(&'a Attribute<'a>, Row, Col)
pub enum Attribute<'a> {
    Name(Row, Col),
    Arg(&'a Expr<'a>, Row, Col),
    End(Row, Col),
    Space(Space, Row, Col),
    FreshLine(Row, Col),
    IndentArg(Row, Col),
    IndentEnd(Row, Col),
}
```

```rust
// declaration/attribute.rs
impl<'a> Parser<'a> {
    /// Zero or more `@name(args)` lines, each ending on a fresh line.
    pub(super) fn chomp_attributes(&mut self) -> Result<&'a [&'a Attribute<'a>], error::Decl<'a>> {
        let mut attrs: BumpVec<'a, &'a Attribute<'a>> = BumpVec::new_in(self.bump);
        loop {
            let next = self.one_of_with_fallback(
                vec![Box::new(|p: &mut Parser<'a>| {
                    p.in_context(
                        |bump, e, r, c| error::Decl::Attribute(bump.alloc(e), r, c),
                        |p| p.word1(b'@', error::Decl::Start),
                        |p| p.attribute_body(),
                    ).map(Some)
                })],
                None,
            )?;
            match next {
                Some(attr) => attrs.push(attr),
                None => return Ok(attrs.into_bump_slice()),
            }
        }
    }

    fn attribute_body(&mut self) -> Result<&'a Attribute<'a>, error::Attribute<'a>> {
        let name_start = self.get_position();
        let name = self.lower_name(error::Attribute::Name)?;
        let name = self.add_end(name_start, name);
        let args = self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.word1(b'(', error::Attribute::End)?;
                p.chomp_and_check_indent(error::Attribute::Space, error::Attribute::IndentArg)?;
                p.one_of(
                    error::Attribute::End,
                    vec![
                        Box::new(|p: &mut Parser<'a>| { p.word1(b')', error::Attribute::End)?; Ok(&[][..]) }),
                        Box::new(|p: &mut Parser<'a>| p.attribute_args()),
                    ],
                )
            })],
            &[][..],
        )?;
        self.chomp(error::Attribute::Space)?;
        self.check_fresh_line(error::Attribute::FreshLine)?;
        Ok(self.alloc(Attribute { name, args }))
    }
    // attribute_args: same loop as macro_args with error::Attribute variants.
}

// declaration():
let maybe_docs = self.chomp_doc_comment()?;
let attributes = self.chomp_attributes()?;
let start = self.get_position();
// type_decl / value_decl receive `attributes` and store it.
```

### Elm / Aiken reference

`Parse/Declaration.hs` (`chompDocComment`, `declaration`);
`aiken/crates/aiken-lang/src/parser/annotation.rs` is not a match; Rust
attribute syntax is the model.

### Tests

`declaration/attribute.rs`: `"@derive(Eq, Show)\ntype T = A | B"`,
`"@inline\nf x = x"`, `"@cost(cpu 10, \"n\")\ntype alias a = int"`,
`"{-| doc -}\n@derive(Eq)\ntype T = A"`, two attributes stacked; errors:
`"@Derive(Eq)\ntype T = A"`, `"@derive(Eq\ntype T = A"`, `"@derive(Eq) type T = A"`
(same line).

### Done when

Attributes appear on the declaration snapshot; an attribute not followed by
a fresh line is `Decl(Attribute(FreshLine ..))`.

---

## Chunk 8 — `trait ... where`

### Files

- `crates/nash-parse/src/declaration/trait_.rs` (new)
- `crates/nash-parse/src/declaration/mod.rs`
- `crates/nash-parse/src/module.rs`
- `crates/nash-parse/src/keyword.rs` (`keyword_trait`, `keyword_where`)
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/module.rs`

### Change

1. **AST.** `Trait`, `TraitMethod`; `Module.traits`; `Decl::Trait`.
2. **Head.** `trait [context =>] Name param+ where`. The head is parsed
   without reinterpretation: after `trait`, `one_of` [ `(` context `)` `=>`,
   `Name param*` then `one_of_with_fallback` [`=>` meaning it was a
   superclass] ]. Superclass constraints take only type-variable arguments.
3. **Body.** After `where`, `chomp`, then `with_indent` over aligned items
   (same shape as `chomp_let_defs`, `let_.rs:87`). An item is a signature
   `name : type_scheme`; the next aligned item is checked for a default
   definition of the same name (`chomp_matching_name`, `let_.rs:229`, then
   `chomp_def_args_and_body`, `let_.rs:175`). Empty bodies are allowed: the
   next token at column 1 ends the block.
4. **Module.** `categorize_decls` (`module.rs:257`) gains a `traits` vector.
   nash-can rejects a non-empty `module.traits` with `Unsupported`.

### Code

```rust
// nash-source
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
// Module: `pub traits: &'a [&'a Located<Trait<'a>>],`
```

```rust
// error.rs
// Decl::Trait(&'a Trait<'a>, Row, Col)
pub enum Trait<'a> {
    Space(Space, Row, Col),
    Name(Row, Col),
    Param(&'a TypeParam<'a>, Row, Col),
    Super(&'a Type<'a>, Row, Col),
    /// A superclass argument that is not a type variable.
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
    Alignment(u16, Row, Col),
}
```

```rust
// declaration/trait_.rs
impl<'a> Parser<'a> {
    pub(super) fn trait_decl(
        &mut self,
        attributes: &'a [&'a Attribute<'a>],
        start: Position,
    ) -> Result<(Decl<'a>, Position), error::Decl<'a>> {
        self.in_context(
            |bump, e, r, c| error::Decl::Trait(bump.alloc(e), r, c),
            |p| p.keyword_trait(error::Decl::Start),
            |p| {
                p.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentName)?;
                let (supers, name, params) = p.trait_head()?;
                p.keyword_where(TraitErr::Where)?;
                let where_end = p.get_position();
                p.chomp(TraitErr::Space)?;
                let (methods, end) = p.trait_body(where_end)?;
                let decl = Trait { name, params, supers, methods, attributes };
                Ok((Decl::Trait(p.alloc(Located::at(Region::new(start, end), decl))), end))
            },
        )
    }

    /// `[context =>] Name param+`, leaving the parser on `where`.
    fn trait_head(&mut self) -> Result<(&'a [&'a Located<Constraint<'a>>], &'a Located<&'a str>, &'a [&'a TypeParam<'a>]), TraitErr<'a>> {
        let supers = self.one_of_with_fallback(
            vec![
                Box::new(|p: &mut Parser<'a>| p.paren_super_context()),
                Box::new(|p: &mut Parser<'a>| {
                    let c = p.super_constraint()?;
                    p.word2(b'=', b'>', TraitErr::Where)?;
                    p.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentName)?;
                    Ok(p.alloc_slice_copy(&[c]))
                }),
            ],
            &[][..],
        )?;
        let name_start = self.get_position();
        let name = self.upper_name(TraitErr::Name)?;
        let name = self.add_end(name_start, name);
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;
        let mut params: BumpVec<'a, &'a TypeParam<'a>> = BumpVec::new_in(self.bump);
        loop {
            let next = self.one_of_with_fallback(
                vec![Box::new(|p: &mut Parser<'a>| {
                    let param = p.specialize(|bump, e, r, c| TraitErr::Param(bump.alloc(e), r, c), |p| p.type_param())?;
                    p.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentWhere)?;
                    Ok(Some(param))
                })],
                None,
            )?;
            match next { Some(param) => params.push(param), None => break }
        }
        if params.is_empty() {
            let (row, col) = self.position();
            return Err(TraitErr::Param(self.alloc(error::TypeParam::Start(row, col)), row, col));
        }
        Ok((supers, name, params.into_bump_slice()))
    }

    /// `Eq 'a` with only type-variable arguments.
    fn super_constraint(&mut self) -> Result<&'a Located<Constraint<'a>>, TraitErr<'a>> {
        let start = self.get_position();
        let class_name = self.upper_name(TraitErr::Name)?;
        let class = self.add_end(start, class_name);
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;
        let mut args: BumpVec<'a, &'a Located<Type<'a>>> = BumpVec::new_in(self.bump);
        loop {
            let next = self.one_of_with_fallback(
                vec![Box::new(|p: &mut Parser<'a>| {
                    let vs = p.get_position();
                    let v = p.type_var_name(TraitErr::SuperArg)?;
                    p.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;
                    Ok(Some(p.add_end(vs, Type::Var(v))))
                })],
                None,
            )?;
            match next { Some(a) => args.push(a), None => break }
        }
        if args.is_empty() { let (r, c) = self.position(); return Err(TraitErr::SuperArg(r, c)); }
        Ok(self.add_end(start, Constraint { class, module: None, args: args.into_bump_slice() }))
    }
    // paren_super_context: `(` super_constraint { `,` super_constraint } `)` `=>`.

    /// Aligned methods after `where`; empty when the next token is at column 1.
    fn trait_body(&mut self, where_end: Position) -> Result<(&'a [&'a TraitMethod<'a>], Position), TraitErr<'a>> {
        if self.col() == 1 || self.is_eof() {
            return Ok((&[], where_end));
        }
        self.check_indent(where_end.line, where_end.column, TraitErr::IndentMethod)?;
        self.with_indent(|p| {
            let mut methods: BumpVec<'a, &'a TraitMethod<'a>> = BumpVec::new_in(p.bump);
            let (first, mut end) = p.trait_method()?;
            methods.push(first);
            loop {
                let next = p.one_of_with_fallback(
                    vec![Box::new(|p: &mut Parser<'a>| { p.check_aligned(TraitErr::Alignment)?; p.trait_method().map(Some) })],
                    None,
                )?;
                match next { Some((m, e)) => { methods.push(m); end = e; } None => break }
            }
            Ok((methods.into_bump_slice(), end))
        })
    }

    /// `name : scheme` optionally followed by an aligned `name args = body`.
    fn trait_method(&mut self) -> Result<(&'a TraitMethod<'a>, Position), TraitErr<'a>> {
        let start = self.get_position();
        let name_str = self.lower_name(TraitErr::MethodName)?;
        let name = self.add_end(start, name_str);
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentColon)?;
        self.word1(b':', TraitErr::Colon)?;
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentType)?;
        let (annotation, sig_end) = self.specialize(|bump, e, r, c| TraitErr::Type(bump.alloc(e), r, c), |p| p.type_scheme())?;
        let default = self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.check_aligned(TraitErr::Alignment)?;
                if !p.remaining().starts_with(name_str.as_bytes()) { let (r, c) = p.position(); return Err(TraitErr::MethodName(r, c)); }
                p.specialize(
                    |bump, e, r, c| TraitErr::Default(name_str, bump.alloc(e), r, c),
                    |p| {
                        let def_start = p.get_position();
                        let def_name = p.chomp_matching_name(name_str)?;
                        p.chomp_and_check_indent(DefErr::Space, DefErr::IndentEquals)?;
                        p.chomp_def_args_and_body(def_start, def_name, None)
                    },
                ).map(Some)
            })],
            None,
        )?;
        let end = default.map_or(sig_end, |(_, e)| e);
        let method = self.alloc(TraitMethod { name, annotation, default: default.map(|(d, _)| d) });
        Ok((method, end))
    }
}
```

The `starts_with(name)` guard makes the default alternative fail without
consuming when the next aligned item is a different method, so `one_of`
moves on. `chomp_matching_name` and `chomp_def_args_and_body` in `let_.rs`
change from private to `pub(crate)`. `declaration()` adds
`Box::new(|p| p.trait_decl(attributes, start))` before `value_decl`.

### Elm reference

`Parse/Expression.hs` (`let_`, `chompLetDefs`, `definition`,
`chompMatchingName`) for the aligned block and annotation-then-definition
pairing; `Parse/Declaration.hs` (`typeDecl`) for the `in_context` shape.

### Tests

`declaration/trait_.rs` (`assert_decl_snapshot!`):

```
trait Eq 'a => Ord 'a where
    compare : 'a -> 'a -> ordering

    lt : 'a -> 'a -> bool
    lt a b = compare a b == LT
```

`trait Functor ('f : Big -> Big) where\n    map : ('a -> 'b) -> 'f 'a -> 'f 'b`,
`trait (Ord 'k, ToData 'k) => Key 'k where\n    hash : 'k -> Bytes`,
`trait Lift 'small 'big where\n    lift : 'small -> 'big\n    lower : 'big -> 'small`,
`trait Marker 'a where` followed by `x = 1` at column 1 (empty body, via
`assert_module_snapshot!`); errors: `trait Eq where`, `trait eq 'a where`,
`trait Eq 'a\n    eq : 'a -> bool` (missing where), method definition without
signature, misaligned second method, `trait Eq int => Ord 'a where`.

### Done when

The four success cases snapshot with correct `supers`/`params`/`default`;
`module_full`-style test with a trait followed by a value parses.

---

## Chunk 9 — `impl ... where`

### Files

- `crates/nash-parse/src/declaration/impl_.rs` (new)
- `crates/nash-parse/src/declaration/mod.rs`
- `crates/nash-parse/src/module.rs`
- `crates/nash-parse/src/keyword.rs` (`keyword_impl`)
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/module.rs`

### Change

`impl [context =>] Trait type_term+ where` then an aligned block of value
definitions. The head is parsed as a `type_scheme`-style reinterpretation:
parse `type_expr` (which reads `Eq 'a` or `Eq (list 'a)` as an application),
then if `=>` follows, convert it with `to_constraints` (chunk 2) and parse
the real head; the head itself is converted with `to_constraint`, which
enforces "uppercase class with at least one argument". Body items reuse
`chomp_def_args_and_body` with `annotation: None`.

### Code

```rust
// nash-source
#[derive(Debug)]
pub struct Impl<'a> {
    pub context: &'a [&'a Located<Constraint<'a>>],
    pub head: &'a Located<Constraint<'a>>,
    pub methods: &'a [&'a Located<Def<'a>>],
    pub attributes: &'a [&'a Attribute<'a>],
}
// Module: `pub impls: &'a [&'a Located<Impl<'a>>],`

// error.rs
// Decl::Impl(&'a Impl<'a>, Row, Col)
pub enum Impl<'a> {
    Space(Space, Row, Col),
    Head(&'a Type<'a>, Row, Col),
    /// The head is not `Trait type+`.
    BadHead(Row, Col),
    Where(Row, Col),
    Method(&'a str, &'a Def<'a>, Row, Col),
    MethodName(Row, Col),
    IndentHead(Row, Col),
    IndentWhere(Row, Col),
    IndentMethod(Row, Col),
    Alignment(u16, Row, Col),
}
```

```rust
// declaration/impl_.rs
pub(super) fn impl_decl(&mut self, attributes: &'a [&'a Attribute<'a>], start: Position) -> Result<(Decl<'a>, Position), error::Decl<'a>> {
    self.in_context(
        |bump, e, r, c| error::Decl::Impl(bump.alloc(e), r, c),
        |p| p.keyword_impl(error::Decl::Start),
        |p| {
            p.chomp_and_check_indent(ImplErr::Space, ImplErr::IndentHead)?;
            let head_start = p.get_position();
            let (scheme, head_end) = p.specialize(|bump, e, r, c| ImplErr::Head(bump.alloc(e), r, c), |p| p.type_scheme())?;
            let bad = || ImplErr::BadHead(head_start.line, head_start.column);
            let head = p.to_constraint(scheme.typ).ok_or_else(bad)?;
            p.check_indent(head_end.line, head_end.column, ImplErr::IndentWhere)?;
            p.keyword_where(ImplErr::Where)?;
            let where_end = p.get_position();
            p.chomp(ImplErr::Space)?;
            let (methods, end) = p.impl_body(where_end)?;
            let decl = Impl { context: scheme.constraints, head, methods, attributes };
            Ok((Decl::Impl(p.alloc(Located::at(Region::new(start, end), decl))), end))
        },
    )
}

fn impl_body(&mut self, where_end: Position) -> Result<(&'a [&'a Located<Def<'a>>], Position), ImplErr<'a>> {
    if self.col() == 1 || self.is_eof() { return Ok((&[], where_end)); }
    self.check_indent(where_end.line, where_end.column, ImplErr::IndentMethod)?;
    self.with_indent(|p| {
        let mut methods: BumpVec<'a, &'a Located<Def<'a>>> = BumpVec::new_in(p.bump);
        let (first, mut end) = p.impl_method()?;
        methods.push(first);
        loop {
            let next = p.one_of_with_fallback(
                vec![Box::new(|p: &mut Parser<'a>| { p.check_aligned(ImplErr::Alignment)?; p.impl_method().map(Some) })],
                None,
            )?;
            match next { Some((m, e)) => { methods.push(m); end = e; } None => break }
        }
        Ok((methods.into_bump_slice(), end))
    })
}

fn impl_method(&mut self) -> Result<(&'a Located<Def<'a>>, Position), ImplErr<'a>> {
    let start = self.get_position();
    let name_str = self.lower_name(ImplErr::MethodName)?;
    let name = self.add_end(start, name_str);
    self.specialize(
        |bump, e, r, c| ImplErr::Method(name_str, bump.alloc(e), r, c),
        |p| { p.chomp_and_check_indent(DefErr::Space, DefErr::IndentEquals)?; p.chomp_def_args_and_body(start, name, None) },
    )
}
```

`type_scheme` stops at `where` because `type_chomp_args` tries `type_term`
on the reserved word and `lower_name` fails without consuming (chunk 0,
item 5).

### Elm reference

Same as chunk 8. `Parse/Type.hs` (`expression`) for the head.

### Tests

`declaration/impl_.rs`: `impl Ord int where\n    compare = Builtin.compareInteger`,
`impl Eq 'a => Eq (list 'a) where\n    eq xs ys = eqList xs ys`,
`impl Lift int Int where\n    lift = liftInt\n    lower = lowerInt`,
`impl (Eq 'a, Eq 'b) => Eq ('a, 'b) where\n    eq (a, b) (c, d) = a == c && b == d`,
`impl Show unit where` + `x = 1` at column 1 (module test); errors:
`impl int where`, `impl Eq where`, `impl Eq int`, method with annotation
`eq : int -> bool`, misaligned methods.

### Done when

All success snapshots show `context`, `head.args`, and `methods`.

---

## Chunk 10 — `validator module`

### Files

- `crates/nash-parse/src/module.rs`
- `crates/nash-parse/src/keyword.rs` (`keyword_validator`)
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/module.rs` (pass `kind` through unchanged)

### Change

`module_header` (`module.rs:26`–`64`) tries `validator` first: the keyword,
`chomp_and_check_indent`, then `module`. Store the kind on `Module`.
`validator` is reserved, so a value named `validator` cannot masquerade as a
header. nash-can keeps the kind on `CanModule` (a one-field addition; no
semantics yet — `validators.md`).

### Code

```rust
// nash-source
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind { Normal, Validator(Region) }
// Module: `pub kind: ModuleKind,`

// error.rs, Module: `Validator(Row, Col),`

// module.rs
pub fn module_header(&mut self) -> Result<(ModuleKind, &'a Located<&'a str>, &'a Located<Exposing<'a>>), error::Module<'a>> {
    let kind = self.one_of_with_fallback(
        vec![Box::new(|p: &mut Parser<'a>| {
            let start = p.get_position();
            p.keyword_validator(error::Module::Validator)?;
            let end = p.get_position();
            p.chomp_and_check_indent(error::Module::Space, error::Module::Validator)?;
            Ok(ModuleKind::Validator(Region::new(start, end)))
        })],
        ModuleKind::Normal,
    )?;
    self.keyword_module(error::Module::Problem)?;
    // ... unchanged from module.rs:32 onward
}
```

### Elm reference

`Parse/Module.hs` (`chompHeader`, the `port module` alternative).

### Tests

`module.rs`: `assert_module_header_snapshot!("validator module Vesting exposing (main)")`,
`assert_module_snapshot!` of the full overview example minus traits/tests
(those come in their chunks; add them back in chunk 11), error
`"validator Vesting exposing (main)"`, `"validator\nmodule V exposing (..)"`
(indent).

### Done when

`ModuleKind::Validator(region)` appears in the header snapshot.

---

## Chunk 11 — `tests` block

### Files

- `crates/nash-parse/src/tests_block.rs` (new; not `tests.rs`, which cargo
  would confuse with the integration-test directory)
- `crates/nash-parse/src/module.rs`
- `crates/nash-parse/src/keyword.rs` (`keyword_tests`, `keyword_test`,
  `keyword_prop`, `keyword_via`, `keyword_once`, `keyword_within`,
  `keyword_cpu`, `keyword_mem`)
- `crates/nash-parse/src/error.rs`
- `crates/nash-source/src/lib.rs`
- `crates/nash-can/src/module.rs`

### Change

1. **AST.** `Tests`, `Test`, `TestKind`, `Expect`, `Budget`, `ViaBinder`;
   `Module.tests: Option<&'a Tests<'a>>`.
2. **Module.** After `declarations()` (`module.rs:235`), `chomp`, then
   `one_of_with_fallback [tests_block]`. Anything after the block that is not
   EOF is `Module::BadEnd`.
3. **Block.** `tests` at column 1, `chomp_and_check_indent`, `with_indent`.
   Imports first: `import()` (`import.rs:24`) works unchanged because it only
   demands `col == 1` at its end for the no-alias form (`import.rs:50`) —
   that check must become `check_fresh_line_or_aligned`: inside the block an
   import ends when the next token is aligned with the block. Add
   `Import.end_aligned: bool` parameter? No: change `import_help`/`import_as`
   to use `self.col() <= self.indent()` instead of `self.col() == 1`; at top
   level `indent` is 1 (`lib.rs:65`), so behaviour there is unchanged.
4. **Items.** `test`/`prop` name modifiers `=` body. `once` after `fail` is
   accepted only on `prop`; on `test` it is `Test::OnceOnUnitTest` (rendered
   as "`once` only makes sense for a property test, which runs many
   times"). `prop` bodies must start with `let` and every binder uses
   `via`; a prop `let` without `via` is `Test::Via`.
5. **Bodies are `do` sequencing blocks.** A test body (and the part of a
   prop body after `in`) is `do` followed by an aligned statement block
   parsed with `do_body` from chunk 5, stored as a `Block`, not as
   `Expr::Do`. The `do` keyword is required (`Test::Do` when missing).
   Desugaring of this top-level `do` is `let`-sequencing (`testing.md`), not
   `bind`; the AST distinction keeps the two apart. Any nested `do` (inside a
   statement, or in a `via` generator) is an ordinary monadic `Expr::Do`.

### Code

```rust
// nash-source
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

/// A let-sequenced statement block (test bodies); same statements as `do`.
#[derive(Debug)]
pub struct Block<'a> {
    pub stmts: &'a [&'a Located<Stmt<'a>>],
    pub last: &'a Located<Expr<'a>>,
}

#[derive(Debug)]
pub enum TestBody<'a> {
    Unit(&'a Block<'a>),
    Prop { binders: &'a [&'a Located<ViaBinder<'a>>], body: &'a Block<'a> },
}

#[derive(Debug)]
pub struct ViaBinder<'a> {
    pub pattern: &'a Located<Pattern<'a>>,
    pub fuzzer: &'a Located<Expr<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect { Pass, Fail, FailOnce }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget { Cpu(i128), Mem(i128), Both { cpu: i128, mem: i128 } }
```

```rust
// error.rs
// Module::Tests(&'a Tests<'a>, Row, Col)
pub enum Tests<'a> {
    Space(Space, Row, Col),
    Import(&'a Module<'a>, Row, Col),
    Test(&'a Test<'a>, Row, Col),
    Start(Row, Col),
    IndentStart(Row, Col),
    Alignment(u16, Row, Col),
}

pub enum Test<'a> {
    Space(Space, Row, Col),
    Name(StringError, Row, Col),
    NameStart(Row, Col),
    /// `once` after `fail` on a `test` item; only a `prop` runs many times.
    OnceOnUnitTest(Row, Col),
    WithinOpen(Row, Col),
    WithinKind(Row, Col),
    WithinNumber(Number, Row, Col),
    WithinDuplicate(Row, Col),
    WithinEnd(Row, Col),
    Equals(Row, Col),
    /// The body does not start with `do`.
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
    BinderAlignment(u16, Row, Col),
}
```

```rust
// tests_block.rs
impl<'a> Parser<'a> {
    /// `tests` block at the end of a module.
    pub(crate) fn tests_block(&mut self) -> Result<&'a Tests<'a>, error::Module<'a>> {
        self.in_context(
            |bump, e, r, c| error::Module::Tests(bump.alloc(e), r, c),
            |p| p.keyword_tests(error::Module::BadEnd),
            |p| {
                p.chomp_and_check_indent(TestsErr::Space, TestsErr::IndentStart)?;
                p.with_indent(|p| {
                    let imports = p.specialize(|bump, e, r, c| TestsErr::Import(bump.alloc(e), r, c), |p| p.imports())?;
                    let mut tests: BumpVec<'a, &'a Located<Test<'a>>> = BumpVec::new_in(p.bump);
                    loop {
                        let next = p.one_of_with_fallback(
                            vec![Box::new(|p: &mut Parser<'a>| {
                                p.check_aligned(TestsErr::Alignment)?;
                                p.specialize(|bump, e, r, c| TestsErr::Test(bump.alloc(e), r, c), |p| p.test_item()).map(Some)
                            })],
                            None,
                        )?;
                        match next { Some(t) => tests.push(t), None => break }
                    }
                    Ok(p.alloc(Tests { imports, tests: tests.into_bump_slice() }))
                })
            },
        )
    }

    /// `test "name" mods = expr` or `prop "name" mods = let binders in expr`.
    fn test_item(&mut self) -> Result<&'a Located<Test<'a>>, TestErr<'a>> {
        let start = self.get_position();
        let is_prop = self.one_of(
            TestErr::NameStart,
            vec![
                Box::new(|p: &mut Parser<'a>| { p.keyword_test(TestErr::NameStart)?; Ok(false) }),
                Box::new(|p: &mut Parser<'a>| { p.keyword_prop(TestErr::NameStart)?; Ok(true) }),
            ],
        )?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentName)?;
        let name_start = self.get_position();
        let name_str = self.string_literal(TestErr::NameStart, TestErr::Name)?;
        let name = self.add_end(name_start, name_str);
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
        let expect = self.test_expect(is_prop)?;
        let budget = self.test_budget()?;
        self.word1(b'=', TestErr::Equals)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let (body, end) = if is_prop { self.prop_body()? } else {
            let (block, end) = self.test_block()?;
            (TestBody::Unit(block), end)
        };
        self.chomp(TestErr::Space)?;
        Ok(self.alloc(Located::at(Region::new(start, end), Test { name, expect, budget, body })))
    }

    /// `do` followed by an aligned statement block, sequenced with `let`.
    fn test_block(&mut self) -> Result<(&'a Block<'a>, Position), TestErr<'a>> {
        self.keyword_do(TestErr::Do)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let (stmts, last, end) = self.specialize(
            |bump, e, r, c| TestErr::Body(bump.alloc(e), r, c),
            |p| p.with_indent(|p| p.do_body()),
        )?;
        Ok((self.alloc(Block { stmts, last }), end))
    }

    /// `fail` on both kinds; `fail once` only on `prop`.
    fn test_expect(&mut self, is_prop: bool) -> Result<Expect, TestErr<'a>> {
        self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.keyword_fail(TestErr::Equals)?;
                p.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
                let (row, col) = p.position();
                p.one_of_with_fallback(
                    vec![Box::new(|p: &mut Parser<'a>| {
                        p.keyword_once(TestErr::OnceOnUnitTest)?;
                        if !is_prop {
                            return Err(TestErr::OnceOnUnitTest(row, col));
                        }
                        p.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
                        Ok(Expect::FailOnce)
                    })],
                    Expect::Fail,
                )
            })],
            Expect::Pass,
        )
    }

    /// `within (cpu N, mem M)` in either order, at most one of each.
    fn test_budget(&mut self) -> Result<Option<Budget>, TestErr<'a>> {
        self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.keyword_within(TestErr::Equals)?;
                p.chomp_and_check_indent(TestErr::Space, TestErr::WithinOpen)?;
                p.word1(b'(', TestErr::WithinOpen)?;
                p.chomp_and_check_indent(TestErr::Space, TestErr::WithinKind)?;
                let first = p.budget_entry()?;
                p.chomp_and_check_indent(TestErr::Space, TestErr::WithinEnd)?;
                let budget = p.one_of(
                    TestErr::WithinEnd,
                    vec![
                        Box::new(|p: &mut Parser<'a>| {
                            p.word1(b',', TestErr::WithinEnd)?;
                            p.chomp_and_check_indent(TestErr::Space, TestErr::WithinKind)?;
                            let (row, col) = p.position();
                            let second = p.budget_entry()?;
                            p.chomp_and_check_indent(TestErr::Space, TestErr::WithinEnd)?;
                            p.word1(b')', TestErr::WithinEnd)?;
                            match (first, second) {
                                (Budget::Cpu(cpu), Budget::Mem(mem)) | (Budget::Mem(mem), Budget::Cpu(cpu)) => Ok(Budget::Both { cpu, mem }),
                                _ => Err(TestErr::WithinDuplicate(row, col)),
                            }
                        }),
                        Box::new(|p: &mut Parser<'a>| { p.word1(b')', TestErr::WithinEnd)?; Ok(first) }),
                    ],
                )?;
                p.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
                Ok(Some(budget))
            })],
            None,
        )
    }

    fn budget_entry(&mut self) -> Result<Budget, TestErr<'a>> {
        let kind = self.one_of(
            TestErr::WithinKind,
            vec![
                Box::new(|p: &mut Parser<'a>| { p.keyword_cpu(TestErr::WithinKind)?; Ok(Budget::Cpu as fn(i128) -> Budget) }),
                Box::new(|p: &mut Parser<'a>| { p.keyword_mem(TestErr::WithinKind)?; Ok(Budget::Mem as fn(i128) -> Budget) }),
            ],
        )?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::WithinKind)?;
        let n = self.number_literal(TestErr::WithinKind, TestErr::WithinNumber)?;
        Ok(kind(n))
    }

    /// `let p via e ... in expr` with the `let` layout rules.
    fn prop_body(&mut self) -> Result<(TestBody<'a>, Position), TestErr<'a>> {
        self.keyword_let(TestErr::Let)?;
        let (binders, binders_end) = self.with_backset_indent(3, |p| {
            p.chomp_and_check_indent(TestErr::Space, TestErr::IndentBinder)?;
            p.with_indent(|p| {
                let mut binders: BumpVec<'a, &'a Located<ViaBinder<'a>>> = BumpVec::new_in(p.bump);
                let (first, mut end) = p.via_binder()?;
                binders.push(first);
                loop {
                    let next = p.one_of_with_fallback(
                        vec![Box::new(|p: &mut Parser<'a>| { p.check_aligned(TestErr::BinderAlignment)?; p.via_binder().map(Some) })],
                        None,
                    )?;
                    match next { Some((b, e)) => { binders.push(b); end = e; } None => break }
                }
                Ok((binders.into_bump_slice(), end))
            })
        })?;
        self.check_indent(binders_end.line, binders_end.column, TestErr::IndentIn)?;
        self.keyword_in(TestErr::In)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let (body, end) = self.test_block()?;
        Ok((TestBody::Prop { binders, body }, end))
    }

    fn via_binder(&mut self) -> Result<(&'a Located<ViaBinder<'a>>, Position), TestErr<'a>> {
        let start = self.get_position();
        let pattern = self.specialize(|bump, e, r, c| TestErr::Pattern(bump.alloc(e), r, c), |p| p.pattern_term())?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::Via)?;
        self.keyword_via(TestErr::Via)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBinder)?;
        let (fuzzer, end) = self.specialize(|bump, e, r, c| TestErr::Fuzzer(bump.alloc(e), r, c), |p| p.expression())?;
        Ok((self.alloc(Located::at(Region::new(start, end), ViaBinder { pattern, fuzzer })), end))
    }
}
```

`module()` (`module.rs:199`) after `declarations()`:

```rust
let decls = self.declarations()?;
self.chomp(error::Module::Space)?;
let tests = self.one_of_with_fallback(
    vec![Box::new(|p: &mut Parser<'a>| p.tests_block().map(Some))],
    None,
)?;
self.chomp(error::Module::Space)?;
if !self.is_eof() {
    return Err(error::Module::BadEnd(self.row, self.col));
}
```

The trailing `BadEnd` check is Elm's `fromByteString ... E.ModuleBadEnd`
(`Parse/Module.hs:35`), which the current port omits; adding it here also
turns leftover garbage after the last declaration into a proper error.

### Elm / Aiken reference

`Parse/Module.hs` (`chompImports`, `chompModule`, `fromByteString`),
`Parse/Expression.hs` (`let_`) for the binder block;
`aiken/crates/aiken-lang/src/parser/definition/test.rs` (`via`, `fail
once`, budgets).

### Tests

`tests_block.rs` (`assert_module_snapshot!` with a header and one value
before the block):

```
tests
    import Fuzz exposing (int, listOf)

    test "lt is strict" = do
        assert (not (lt 1 1))

    test "fails" fail = do
        assert (1 / 0 == 0)

    test "budget" within (cpu 1000, mem 50) = do
        assert True

    prop "antisym" fail once within (mem 5) =
        let
            a via int
            b via int
        in
        do
            label "x"
            assert (compare a b == invert (compare b a))

    prop "sorted" =
        let xs via listOf int in
        do
            let
                ys = sort xs
            n <- length ys
            assert (n == length xs)
```

Plus: `tests` with no imports; a `tests` block with imports only; a body
whose statement binds a monadic `do` expression (`r <- do\n ...`) and
snapshots `Expr::Do` inside the `Block`; errors:
`test "t" =\n        assert True` (Do: missing keyword),
`tests` followed by a top-level `x = 1` (BadEnd), `test name = 1` (NameStart),
`prop "p" = assert True` (Let), `prop "p" = let x = int in x` (Via),
`test "t" fail once = assert True` (OnceOnUnitTest),
`test "t" =\n        x <- e` (Body(LastNotExpr)),
`within (cpu 1, cpu 2)` (WithinDuplicate), `tests` then unaligned items.

### Done when

The overview's full example (`docs/overview.md` "Syntax in one page")
parses with `assert_module_snapshot!` once traits, attributes, `do`,
keyword expressions and tests are all in; `nash check scratch` still passes.

---

## Chunk 12 — SPEC.md, changesets, snapshot hygiene

### Files

- `SPEC.md`
- `.sampo/changesets/syntax-*.md` (one per chunk, written as each chunk
  lands; this chunk verifies they exist)
- `crates/nash-parse/src/snapshots/`

### Change

1. Tick the "01 Syntax" checkbox in `SPEC.md`. `SPEC.md` is now a progress
   checklist only; the grammar lives in `docs/syntax.md` and is not touched
   here.
2. `cargo insta test --unreferenced delete` to drop stale snapshots from the
   removed record-extension and char tests.
3. Changesets, e.g. `.sampo/changesets/syntax-type-vars.md`:

```markdown
---
cargo/nash-source: minor
cargo/nash-parse: minor
cargo/nash-can: patch
---

Type variables are written `'a`; bare lowercase names in type position are little types. Record extension types are removed.
```

Chunk 0 is `nash-parse: minor` alone (reserved words change is breaking for
users of `port`). Chunks 2–11, including 6a, each bump `nash-source` and
`nash-parse` `minor` and `nash-can` `patch`.

### Done when

`cargo insta test --unreferenced delete` reports nothing to delete on a
second run; the "01 Syntax" box in `SPEC.md` is ticked; `sampo` sees one
changeset per chunk.

---

## Ordering summary

| # | Chunk | Compiles alone | Depends on |
|---|---|---|---|
| 0 | leftovers, reserved words | yes | — |
| 1 | `'a`, little types, kinds, no record ext | yes (nash-can fixture rewrite) | 0 |
| 2 | constraints, `Annotation` | yes | 1 |
| 3 | bytes literals | yes | 0 |
| 4 | keyword expressions | yes | 0 |
| 5 | `do` blocks | yes | 0 |
| 6 | macro calls | yes | 0 |
| 6a | partial operator sections | yes | 0 |
| 7 | attributes | yes | 6 (arg list shape) |
| 8 | traits | yes | 1, 2, 7 |
| 9 | impls | yes | 2, 7, 8 (`keyword_where`) |
| 10 | validator header | yes | 0 |
| 11 | tests block | yes | 4, 5 (`do_body`), 10 |
| 12 | SPEC, changesets, snapshots | yes | all |

Chunks 3–6a and 10 are independent of one another and can land in any order
after chunk 0.

## Open questions

Raised by this plan:

- Chunk 0 makes `lower_name` fail without consuming on a reserved word.
  Existing error snapshots that recorded the post-consumption position will
  change; accept them, since the new behaviour is what `one_of` needs
  everywhere and matches Elm's `eerr`.
