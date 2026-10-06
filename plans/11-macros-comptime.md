# Plan 11: macros and comptime

Goal: implement [docs/macros.md](../docs/macros.md): `macro` declarations,
`@attr` and `name!()` invocations, `quote`/`~`, the `Ast` reification in a
new `nash-macro` crate, the expansion loop in `nash-driver`, hygiene,
`comptime`, `@derive` in `nash/base`, diagnostics, and expansion snapshot
tests, explicit native integer dispatch through a library macro, and a staged
migration of property/assertion expansion from codegen into Nash macros.

## Execution strategy: establish macro basics first

This section sets implementation order. The numbered chunks below are component
work packages, not a requirement to finish each layer in isolation before running
a macro. Older code sketches illustrate shapes; current compiler APIs and the
contracts below take precedence. No implementation is marked complete by this revision.

A compiler macro is a function from syntax to syntax, executed during compilation.
The minimum complete path must answer six questions:

| Basic responsibility | Required result |
|---|---|
| Parse and resolve | Distinguish an imported macro declaration from an ordinary value and resolve an invocation to it. |
| Transport syntax | Supply structured expression nodes, source origins, lexical references, and available types without evaluating the argument. |
| Execute | Compile the macro through ordinary Nash codegen and run it with the existing bounded CEK evaluator. |
| Decode and bind | Validate returned AST and preserve caller references while giving generated bindings distinct identities. |
| Expand and check | Replace the invocation, then canonicalize and infer from fresh state; strictly check the final program. |
| Explain failures | Report the invocation, macro identity, and useful expansion/evaluation errors. |

Quote syntax is convenient AST construction, not the execution mechanism.
Deriving, property syntax, and integer dispatch are clients of this mechanism.
Neither a second evaluator nor property-specific logic belongs in the macro core.

### Milestone A: one imported expression macro, end to end

Start with the call-shaped interface and a small test macro module. Use AST
builders first; quote/splice, attributes, deriving, binding/case forms, and the
property migration need not block this first working slice. This is a development
milestone, not a reduction of the final AST coverage or accepted Plan 11 scope.

1. [ ] Inventory the current source/canonical variants and define the initial
   shared `Ast` encoding with Plan 12. Specify source origins and lexical identity
   before writing the reifier. `NodeId` identifies a syntax node, not its binder.
2. [ ] Add annotated macro declarations and exported macro shape metadata. Keep
   the existing imported-only invocation rule; test same-module and wrong-shape
   errors. Parse/format the supported syntax through the existing frontend.
3. [ ] Compile a real imported identity macro through `Build` and ordinary closed
   program assembly. Execute it on syntax with the CEK machine; no host callback
   or hardcoded macro name may substitute for the Nash macro.
4. [ ] Integrate invocation replacement into the driver. Infer available argument
   types, keep provisional macro-result types separate, and rebuild canonical
   nodes and `SolvedTypes` after expansion. Never reuse pointer-keyed evidence
   from an earlier round. Final strict inference and coverage are mandatory.
5. [ ] Demonstrate both identity expansion and AST construction with a generated
   local binding. Preserve free caller references and resolve macro-definition
   globals even when the caller does not import them. Include nested shadowing
   and a caller/generated-name collision.
6. [ ] Reject malformed macro output, preserve real input type errors, report
   evaluation/budget failures, and bound repeated expansion. Unsupported input
   variants in an intermediate slice must produce a diagnostic, never a panic.

Acceptance: snapshot source, typed macro input, expanded AST, and ordinary Core/
UPLC for these fixtures, then check execution equivalence and lexical scope.
A trace-bearing argument must not execute during expansion and must execute only
as directed by the expanded program. Follow repository snapshot rules: no duplicate
expected-output assertions; independent semantic checks follow the snapshot.

Milestone A crosses chunks 1–8 and 11–12 in small validated slices. Diagnostics
and expansion snapshots are part of the first slice, not work deferred until derive.

### Milestone B: bindings, provisional checking, and full transport

The revised order retains all three required invocation shapes. In particular,
`expect!`-style bindings and `dispatch!`-style cases are required capabilities,
not optional follow-up work after the property migration:

```nash
someMacro!(argument)

-- In a let/do binding context; x is available to the remaining body.
expect! Some x = value

-- An expression with ordered pattern/body arms.
dispatch! subject of
    0 -> first
    1 -> second
```

The compiler recognizes the call, binding, and case shapes, not these library
names. Demonstrate each shape with a real Nash macro before completing B, including
qualified invocations. The binding macro receives the remaining lexical body;
the case macro receives the subject and independent arm scopes. Neither input
is evaluated merely to pass syntax to the macro.

Keep `dispatch!(n, [branch0, branch1])` from chunk 13 as well: it constructs the
explicit positional IntegerDispatch operation. Case-shaped syntax does not by
itself choose positional dispatch, decode Data, insert bounds checks, or define
fallback behavior. Public `expect` failure semantics and `dispatch` arm rules
remain library decisions to settle before shipping those library macros.

- [ ] Extend the same transport to the complete required AST inventory; keep the
  Nash constructors and host tags synchronized. Use bounded-stack traversal for
  deep syntax, consistent with the current compiler maintenance.
- [ ] Implement scoped binding identity, unresolved-name transport, and precise
  predicate deferral. Missing provisional information is explicit; real
  unification errors remain errors. Do not erase unknown names into a payload-free
  hole that cannot be reconstructed by a macro.
- [ ] Add binding-shaped input with an explicit remaining-body boundary. Prove two
  nested bindings, dependent values, shadowing, and evaluate-once behavior using
  test-only macros. Their resulting AST controls execution order.
- [ ] Add case-shaped input with independent arm scopes and final-only coverage.
  Do not prematurely unify an input subject with patterns awaiting conversion.
- [ ] Prove nested expansion, module import/export behavior, and fresh dependency
  analysis using generated declarations/impls; complete attribute support.

This is chunks 2–6, 8, and 14 together. The initial identity example does not
justify declaring general hygiene or lenient checking complete.

### Milestone C: a property macro as the first substantial client

After binding expansion works, implement chunk 15's smallest property slice:
two dependent draws and a computed label become ordinary generator composition
and body/display callbacks. Settle how the runner discovers the generated private
root before implementing this slice. Preserve the existing runtime protocol first;
do not redesign shrinking while replacing syntax expansion.

The acceptance evidence includes deleting the replaced property-specific Core
construction, not merely adding macros alongside it. Assertion migration follows
only after source capture and optional display evidence have defined contracts.

### Milestone D: complete remaining accepted capabilities

Complete quote/splice, full declaration expansion, Ast/Derive, explicit integer
dispatch, diagnostics/tooling, and the remaining expansion corpus. Quote may be
added earlier when useful; it is not a prerequisite for the first executable
macro. Existing comptime integration stays covered throughout. These remain Plan
11 requirements; the staged order does not silently drop them.

### Settled: fresh lexical bindings (6 October 2026)

Use the same scope-preserving freshening principle as compiler optimization.
Every copied or generated binder gets a fresh identity, and its bound references
are updated consistently. Free references in supplied caller syntax retain their
original binding identity. Distinct scopes never share a binder merely because
its display spelling matches. Source names are for display, not identity.

This settles the hygiene behavior, not the concrete AST encoding. Preserve the
relationship between a supplied pattern and its remaining body when freshening
both. The older `Local string`/gensym sketches below must implement this contract;
a global string rename or one identity per spelling per expansion is insufficient.
The concrete source-origin encoding and explicit caller-visible binding APIs
remain to be settled separately.

### Settled: source locations and expansion origins (6 October 2026)

Moving or copying supplied syntax preserves its original source locations,
including individual operands. Freshening lexical bindings does not replace
those locations. Track the macro invocation and nested expansion history
separately. Generated syntax without an original caller location uses its macro
invocation as the diagnostic location. This follows the preservation principle
used for attached syntax information; it does not require reusing comment storage.

Diagnostics should point to supplied syntax where applicable and make the
expansion chain available. The concrete origin encoding remains implementation
design work; do not collapse every output span to the invocation region.

### Settled: explicit inputs and global resolution (6 October 2026)

Macro-written globals resolve in the macro's defining module. Supplied caller
syntax preserves the caller's lexical/global resolution. Pass caller dependencies
explicitly as arguments; arbitrary caller-scope lookup by a generated string and
implicit anaphoric bindings are deferred from the initial implementation.

Caller-supplied binding patterns remain supported: `expect! Some x = value`
transports the relationship between x and its remaining body. Named generated
declarations and trait methods also remain supported through declaration APIs;
declaring a name is not an escape for looking up arbitrary caller locals.

Historical `Raw`/`Ast.raw` sketches below are not the accepted initial lookup API.
Revise them to distinguish unresolved input spelling, lexical binding identity,
resolved globals, and declared names. Unresolved attribute-name syntax may be
inspected as input without granting implicit caller lookup in generated output.
Deriving uses resolved trait references and explicit method declarations.

### Decisions required before their dependent implementation

| Decision | Resolve before |
|---|---|
| Fresh binding and source-origin behavior settled above; choose their concrete encoding | Milestone A reifier/unreifier |
| Representation of unresolved names/types and exact deferred relationships | General provisional checking in B |
| Binding/case payload signatures and remaining-body boundaries | Structured forms in B |
| Property syntax, private-root discovery, metadata, and interleaved execution semantics | Property migration in C |
| Optional `Show` evidence and operand-level source fidelity | Power-assert migration |

Prefer extending existing representations and library protocols. Record each
settled contract in `docs/macros.md` and the relevant testing spec; do not create
parallel type registries, inference engines, or alternate production paths merely
to get a demonstration running.

## Current baseline and scope (6 October 2026)

Procedural macros remain pending. "Initial scope" means this plan's first
implementation, not a language release or Plutus version. This section supersedes
historical implementation sketches below.

- Plan 01 already parses macro invocations/attributes and comptime; canonical
  macro-call expansion is not implemented. Macro definitions and quote/splice
  support remain work here.
- `nash_solve::SolvedTypes` already records expression/pattern types keyed by
  `nash_ast::NodeId`. Reuse it, rather than adding another type-recording system.
- Closed comptime evaluation already works in `can_to_core::Engine::expr` via
  `closed_dependencies` and `comptime::eval_closed`, producing `Core::Lit`.
- `nash-fmt` exists. Extend its source syntax support. Expanded AST needs debug
  output for resolved names and internal-only operations, not a second formatter.
- Use current Build/Core/program assembly APIs. Do not introduce historical
  `lower_value(&ModuleSet, ...)` or `Core::Const` APIs merely to match this sketch.
- Chunks 1–8, 10–13 are pending macro work. Chunk 9 is existing comptime plus
  integration checks. Chunk 14 requires reusable structured invocation forms, without defining
  library macro behavior. Chunk 15 defines the testing migration milestones; its
  public syntax and discovery contract must be settled before that migration.

AST builders construct expressions, patterns, arms, types, functions, and
declarations. Initial quote/splice shorthand handles expressions only;
pattern/type/declaration/list-position shorthand stays deferred. Builder
expressiveness is not limited by that shorthand decision. Macros see supplied
inputs, not neighboring declarations, filesystem state, or arbitrary type bodies.

**Checking contract:** provisional canonicalization/type inference supplies typed
inputs. Run coverage and redundancy checking on the fully expanded, strictly
checked module, before normal codegen. This generic ordering permits refutable
lambda patterns as macro syntax carriers. It does not permit runtime refutable
lambdas: if expansion leaves one behind, final coverage checking rejects it.
Preserve real provisional unification errors; do not make all macro inputs
untyped. The current driver's `nash_nitpick::check` belongs after final expansion,
not in a special case for a library macro name. Recanonicalize/re-solve from fresh
state after expansion, including normal recursive dependency analysis.

**Binding contract:** moving a supplied lambda's pattern/body into a case arm must
retain bound references and caller free references. Separate clauses have separate
scopes. Test repeated names, nested shadowing, and generated names. Strengthen the
historical Local/Raw string sketches where needed: preserve binding identity or
consistently freshen each moved binder and its references. Do not globally rename
all equal strings or flatten clause scopes. This is generic chunks 4–6 hygiene.

Prerequisites:

- plans/01 (syntax): `@attr(..)` on declarations, `name!(..)`, `comptime`,
  and partial operator sections such as `(> 5)` parsed into `nash-source`.
  Sections canonicalize to ordinary lambdas before macro arguments are
  reified. This plan adds `macro`, `quote`, `~`. If plans/01 named the
  surface types differently, use its names; the shapes below are what this
  plan needs.
- plans/02 Haskell 98 follow-up: closed `nash_ast::Kind` and separate
  representation queries and datatype contexts in `nash-can::kinds`.
- plans/03 (traits): `trait`/`impl` in `nash-source`/`nash-ast`, predicate
  resolution with a hook to defer unresolved predicates.
- plans/07 (codegen): current Build specialization, closed Core assembly,
  `Core::Lit`, and existing CEK evaluation.
- plans/12 (stdlib) chunk "Ast": `crates/nash-driver/base/src/Ast.nash` is the Nash side of the
  reifier/unreifier in this plan (chunks 4 and 5), and chunk 6 there
  supplies `Cons.cons`. The tag table and `Ast.nash` must be changed
  together.

Crates touched: `nash-source`, `nash-parse`, `nash-ast`, `nash-can`,
`nash-constrain`, `nash-solve`, `nash-ir`, `nash-codegen`, `nash-driver`, `nash-report`,
new `nash-macro`, `crates/nash-driver/base/`.

References:

- Elm: `elm/compiler/src/Parse/Declaration.hs` (declaration dispatch),
  `Parse/Expression.hs` (`term`), `Canonicalize/Expression.hs`
  (`canonicalize`, `findVar`), `Canonicalize/Environment/Foreign.hs`,
  `Type/Constrain/Expression.hs` (`constrain`), `Type/Solve.hs` (`run`),
  `Elm/Interface.hs` (`fromModule`).
- Aiken: `crates/aiken-lang/src/gen_uplc.rs` (`generate_raw`,
  `generate_test`) for compiling a single definition to a program;
  `crates/aiken-project/src/lib.rs` (`run_tests`) for running programs
  with a budget and collecting traces; `crates/uplc/src/ast.rs`
  (`Term::Constr` construction).
- Current code: `crates/nash-driver/src/compile.rs:145` (`compile_module`),
  `crates/nash-can/src/module.rs:48` (`canonicalize`),
  `crates/nash-constrain/src/expression.rs:25` (`constrain`),
  `crates/nash-plutus/src/program.rs:51` (`eval_version_budget`),
  `crates/nash-plutus/src/term.rs:31` (`Term::Constr`),
  `crates/nash-plutus/src/machine/discharge.rs` (`value_as_term`),
  `crates/nash-plutus/src/constant.rs` (`Constant::{Integer, String, ByteString}`).

Conventions: `'a` is the module arena lifetime. `Arena` is
`nash_plutus::arena::Arena`; `'p` is its lifetime. The `Ast` family is
little (representation `Term`), so its runtime layout is representation.md's
"Term types": constructor = `constr i [fields]` with `i` the declaration
index and fields positional; a little record alias or labeled constructor
is `constr 0 [..]`/`constr i [..]` in field order; `string`/`int`/`bytes`
fields are the UPLC constants `Constant::String`/`Integer`/`ByteString`;
`option` = `Some` tag 0 / `None` tag 1; `cons` = `Nil` tag 0 /
`Cons` tag 1 (`core/Cons.nash`). Nothing in the macro path is `Data`.
`comptime` results (chunk 9) are unchanged: they must be UPLC constants
(`Const` or `Big` representation) and are spliced as `Core::Lit`.

---

## Chunk 1: surface syntax for `macro`, `quote`, `~`

**Files**

- `crates/nash-source/src/lib.rs`
- `crates/nash-parse/src/keyword.rs`
- `crates/nash-parse/src/declaration/mod.rs`
- `crates/nash-parse/src/declaration/macro_.rs` (new)
- `crates/nash-parse/src/expression/mod.rs`
- `crates/nash-parse/src/expression/quote.rs` (new)
- `crates/nash-parse/src/error.rs`
- `SPEC.md`

**Change**

Add the `macro` declaration, `quote (e)`, and `~x` / `~(e)` to the surface
AST and parser. Attributes, `MacroCall`, and `Comptime` come from plans/01;
this chunk assumes these exist in `nash-source`:

```rust
#[derive(Debug)]
pub struct Attribute<'a> {
    pub region: Region,
    pub module: Option<&'a str>,
    pub name: &'a Located<&'a str>,
    pub arguments: &'a [&'a Located<Expr<'a>>],
}

// on Value, Union, Alias, Trait, Impl:
pub attributes: &'a [&'a Attribute<'a>],

// in Expr:
MacroCall {
    module: Option<&'a str>,
    name: &'a Located<&'a str>,
    arguments: &'a [&'a Located<Expr<'a>>],
},
Comptime(&'a Located<Expr<'a>>),
```

**Code**

`crates/nash-source/src/lib.rs`:

```rust
#[derive(Debug)]
pub struct Module<'a> {
    // ...existing fields...
    pub macros: &'a [&'a Located<Macro<'a>>],
}

/// `macro name : T` followed by `name args = body`.
#[derive(Debug)]
pub struct Macro<'a> {
    pub name: &'a Located<&'a str>,
    pub annotation: &'a Located<Type<'a>>,
    pub arguments: &'a [&'a Located<Pattern<'a>>],
    pub body: &'a Located<Expr<'a>>,
}

pub enum Expr<'a> {
    // ...existing...
    Quote(&'a Located<Expr<'a>>),
    Splice(&'a Located<Expr<'a>>),
    /// Produced only by the macro decoder, never by the parser: a name
    /// that resolves through the interface table regardless of imports.
    VarGlobal {
        package: Option<&'a str>,
        module: &'a str,
        name: &'a str,
    },
}

pub enum Pattern<'a> {
    // ...existing...
    CtorGlobal {
        region: Region,
        package: Option<&'a str>,
        module: &'a str,
        name: &'a str,
        args: &'a [&'a Located<Pattern<'a>>],
    },
}

pub enum Type<'a> {
    // ...existing...
    TypeGlobal {
        region: Region,
        package: Option<&'a str>,
        module: &'a str,
        name: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    },
}
```

`crates/nash-parse/src/declaration/mod.rs` — extend `Decl` and dispatch:

```rust
pub enum Decl<'a> {
    Value(Option<&'a Comment<'a>>, &'a Located<Value<'a>>),
    Union(Option<&'a Comment<'a>>, &'a Located<Union<'a>>),
    Alias(Option<&'a Comment<'a>>, &'a Located<Alias<'a>>),
    Macro(Option<&'a Comment<'a>>, &'a Located<Macro<'a>>),
}

// in `declaration`: try `keyword_macro` before `value_decl`
Box::new(|p: &mut Parser<'a>| p.macro_decl(maybe_docs, start)),
```

`crates/nash-parse/src/declaration/macro_.rs`:

```rust
impl<'a> Parser<'a> {
    /// `macro name : type` then `name patterns = expr` on the next fresh line.
    pub(crate) fn macro_decl(
        &mut self,
        _docs: Option<&'a Comment<'a>>,
        start: Position,
    ) -> Result<(Decl<'a>, Position), error::Decl<'a>> {
        self.keyword_macro(error::Decl::Start)?;
        self.chomp_and_check_indent(error::Decl::Space, error::Decl::IndentMacroName)?;
        let name = self.lower_var(error::Decl::MacroName)?;
        self.chomp_and_check_indent(error::Decl::Space, error::Decl::IndentMacroColon)?;
        self.symbol_colon(error::Decl::MacroColon)?;
        self.chomp_and_check_indent(error::Decl::Space, error::Decl::IndentMacroType)?;
        let annotation = self.type_expr().map_err(|e| error::Decl::MacroType(self.alloc(e)))?;
        self.chomp(error::Decl::Space)?;
        self.check_fresh_line(error::Decl::MacroDefinition)?;
        let def_name = self.lower_var(error::Decl::MacroDefinition)?;
        if def_name.value != name.value {
            return Err(error::Decl::MacroNameMismatch(name.value, def_name.value, def_name.region));
        }
        let (arguments, body, end) = self.definition_tail(error::Decl::Def)?;
        let region = Region::new(start, end);
        let m = self.alloc(Located::at(region, Macro { name, annotation, arguments, body }));
        Ok((Decl::Macro(_docs, m), end))
    }
}
```

`definition_tail` is the existing argument-patterns-then-`=`-then-body
parser factored out of `value_decl` (`crates/nash-parse/src/declaration/value.rs`).

`crates/nash-parse/src/expression/quote.rs`:

```rust
impl<'a> Parser<'a> {
    /// `quote ( expr )`. Nested quotes are an error; splices inside are allowed.
    pub(crate) fn quote(&mut self, start: Position) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        self.keyword_quote(error::Expr::Start)?;
        if self.in_quote {
            return Err(error::Expr::NestedQuote(start.line, start.column));
        }
        self.chomp(error::Expr::Space)?;
        self.symbol_open_paren(error::Expr::QuoteOpen)?;
        self.in_quote = true;
        let inner = self.expression();
        self.in_quote = false;
        let inner = inner?;
        self.chomp(error::Expr::Space)?;
        self.symbol_close_paren(error::Expr::QuoteClose)?;
        let end = self.get_position();
        Ok(self.alloc(Located::at(Region::new(start, end), Expr::Quote(inner))))
    }

    /// `~x` or `~( expr )`; only valid while `in_quote`.
    pub(crate) fn splice(&mut self, start: Position) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        self.symbol_tilde(error::Expr::Start)?;
        if !self.in_quote {
            return Err(error::Expr::SpliceOutsideQuote(start.line, start.column));
        }
        let inner = if self.peek() == Some(b'(') {
            self.advance();
            let e = self.expression()?;
            self.symbol_close_paren(error::Expr::SpliceClose)?;
            e
        } else {
            self.lower_var_expr()?
        };
        let end = self.get_position();
        Ok(self.alloc(Located::at(Region::new(start, end), Expr::Splice(inner))))
    }
}
```

`Parser` gains `in_quote: bool` (`crates/nash-parse/src/lib.rs:39`), reset
in `new`. `term` (`expression/mod.rs`) dispatches on `quote` keyword and
`~`. `~` must not clash with an operator: add `~` to the set of reserved
symbol characters in `symbol.rs` (like `@`).

Errors (`crates/nash-parse/src/error.rs`): `Decl::MacroName`,
`Decl::MacroColon`, `Decl::MacroType`, `Decl::MacroDefinition`,
`Decl::MacroNameMismatch(&'a str, &'a str, Region)`, `Decl::Indent*`,
`Expr::NestedQuote(Row, Col)`, `Expr::SpliceOutsideQuote(Row, Col)`,
`Expr::QuoteOpen/Close`, `Expr::SpliceClose`.

`SPEC.md`: add the EBNF from docs/macros.md (`macro_decl`, `quote_expr`,
`splice`).

**Elm/Aiken reference**

`Parse/Declaration.hs` `declaration` and `valueDecl` (for
`definition_tail`); `Parse/Expression.hs` `term` for adding new term
forms; `Parse/Keyword.hs` for the keyword helpers.

**Tests** (`crates/nash-parse/src/declaration/macro_.rs`, `expression/quote.rs`)

- `assert_decl_snapshot!("macro derive : Decl -> List Expr -> List Decl\nderive decl traits = decl :: traits")`
- `assert_decl_error_snapshot!("macro derive : Decl\nother decl = decl")` → `MacroNameMismatch`
- `assert_decl_error_snapshot!("macro derive decl = decl")` → `MacroColon`
- `assert_expression_snapshot!("quote (f x 1)")`
- `assert_expression_snapshot!("quote (~x + ~(g y))")`
- `assert_expression_error_snapshot!("~x")` → `SpliceOutsideQuote`
- `assert_expression_error_snapshot!("quote (quote (x))")` → `NestedQuote`
- `assert_module_snapshot!` with a macro and a value in one module.

**Done when** the snapshots above pass and `cargo clippy` is clean.

---

## Chunk 2: canonical macro declarations, invocations, lenient mode

**Files**

- `crates/nash-ast/src/lib.rs`
- `crates/nash-can/src/module.rs`, `expression.rs`, `pattern.rs`, `types.rs`
- `crates/nash-can/src/environment.rs`, `environment/foreign.rs`
- `crates/nash-can/src/interface.rs`
- `crates/nash-can/src/error.rs`
- `crates/nash-can/src/quote.rs` (new)

**Change**

1. Canonicalize `macro` declarations into `Module.macros`; export them in
   the interface with their shape.
2. Canonicalize invocations: `Attribute` and `Expr::MacroCall` resolve the
   macro name through the env and check shape and same-module use.
   Collect every use into `CanResult.macro_uses`.
3. `Context.mode`: `Strict` (today's behaviour) or `Lenient` (unknown
   names become holes).
4. Resolve `VarGlobal` / `CtorGlobal` / `TypeGlobal` through
   `Context.interfaces` directly.
5. Desugar `Expr::Quote` into calls of `Ast` constructors.

**Code**

`crates/nash-ast/src/lib.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroShape {
    /// `Ast.decl -> cons Ast.expr -> cons Ast.decl`
    Decl,
    /// `cons Ast.expr -> Ast.expr`
    Expr,
}

#[derive(Debug)]
pub struct MacroDef<'a> {
    pub name: &'a Located<&'a str>,
    pub shape: MacroShape,
    pub def: &'a Def<'a>,
}

pub struct Module<'a> {
    // ...
    pub macros: &'a [&'a MacroDef<'a>],
}

pub enum Expr<'a> {
    // ...
    MacroCall {
        reference: QualifiedName<'a>,
        arguments: &'a [&'a Located<Expr<'a>>],
    },
    Comptime(&'a Located<Expr<'a>>),
    /// Lenient mode only: an unresolved name. Never reaches codegen.
    Hole,
}

pub enum Pattern<'a> { /* ... */ Hole }
pub enum Type<'a> { /* ... */ Hole }

/// A declaration decorated with attributes, in source order.
#[derive(Debug)]
pub struct Attribute<'a> {
    pub region: Region,
    pub reference: QualifiedName<'a>,
    /// Untyped: attribute arguments are never canonicalized.
    pub arguments: &'a [&'a Located<nash_source::Expr<'a>>],
}

#[derive(Debug)]
pub enum MacroUse<'a> {
    Decl {
        target: DeclTarget<'a>,
        attribute: &'a Attribute<'a>,
    },
    Expr {
        region: Region,
        reference: QualifiedName<'a>,
        arguments: &'a [&'a Located<Expr<'a>>],
    },
}

#[derive(Clone, Copy, Debug)]
pub enum DeclTarget<'a> {
    Value(&'a str),
    Union(&'a str),
    Alias(&'a str),
    Trait(&'a str),
    Impl(u32),
}
```

`crates/nash-can/src/module.rs`:

```rust
/// Read by canonicalization (`Context.mode`) and passed unchanged to
/// `nash_solve::run(.., mode)` (plans/03 chunk 5 "Solver API"). The one mode type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Strict,
    Lenient,
}

pub struct Context<'a> {
    pub package: Option<PackageName<'a>>,
    pub interfaces: Option<&'a BTreeMap<&'a str, Interface<'a>>>,
    pub mode: Mode,
}

pub struct CanResult<'a> {
    pub module: CanModule<'a>,
    pub warnings: Vec<Warning<'a>>,
    pub macro_uses: Vec<MacroUse<'a>>,
}
```

Macro shape from the annotation (in `module.rs`, after `canonicalize_aliases`):

```rust
fn macro_shape<'a>(annotation: &Located<CanType<'a>>) -> Result<MacroShape, Error<'a>> {
    match &annotation.value {
        CanType::Lambda { from, to } if is_ast(from, "decl") => match &to.value {
            CanType::Lambda { from: args, to: out }
                if is_cons_of(args, "expr") && is_cons_of(out, "decl") =>
            {
                Ok(MacroShape::Decl)
            }
            _ => Err(Error::MacroBadShape { region: annotation.region }),
        },
        CanType::Lambda { from, to } if is_cons_of(from, "expr") && is_ast(to, "expr") => {
            Ok(MacroShape::Expr)
        }
        _ => Err(Error::MacroBadShape { region: annotation.region }),
    }
}

/// `Cons.cons` applied to the named `Ast` type.
fn is_cons_of(t: &Located<CanType<'_>>, name: &str) -> bool {
    matches!(&t.value, CanType::Named { reference, args: [elem] }
        if reference.home.name == "Cons" && reference.name == "cons" && is_ast(elem, name))
}

fn is_ast(t: &Located<CanType<'_>>, name: &str) -> bool {
    matches!(&t.value, CanType::Named { reference, args: [] }
        if reference.home.name == "Ast" && reference.name == name)
}
```

Env entries: `Var::Macro { home: ModuleName, shape: MacroShape }` for
locally declared macros (never callable locally, but they must shadow
correctly) and `Var::ForeignMacro(ModuleName, MacroShape)` for imported
ones, populated in `foreign.rs` from `Interface.macros`:

```rust
// crates/nash-can/src/interface.rs
#[derive(Clone, Copy, Debug)]
pub struct InterfaceMacro<'a> {
    pub name: &'a str,
    pub shape: MacroShape,
}

pub struct Interface<'a> {
    // ...
    pub macros: &'a [InterfaceMacro<'a>],
}
```

Attribute resolution (`module.rs`, called while canonicalizing each decl):

```rust
fn canonicalize_attribute<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    attr: &'a nash_source::Attribute<'a>,
) -> Result<&'a Attribute<'a>, Error<'a>> {
    let (home, shape) = match attr.module {
        None => env.find_macro(attr.name)?,
        Some(prefix) => env.find_macro_qual(prefix, attr.name)?,
    };
    if home == env.home {
        return Err(Error::MacroSameModule { region: attr.region, name: attr.name.value });
    }
    if shape != MacroShape::Decl {
        return Err(Error::MacroWrongKind { region: attr.region, name: attr.name.value, expected: MacroShape::Decl });
    }
    Ok(bump.alloc(Attribute {
        region: attr.region,
        reference: QualifiedName { home, name: attr.name.value },
        arguments: attr.arguments,
    }))
}
```

`Expr::MacroCall` in `expression.rs` does the same with `MacroShape::Expr`,
canonicalizes the arguments normally, and pushes `MacroUse::Expr` onto a
`&mut Vec<MacroUse>` threaded like `warnings` is today.

Lenient mode in `expression.rs` `find_var`:

```rust
Err(err @ Error::NotFoundVar { .. }) if mode == Mode::Lenient => Ok(CanExpr::Hole),
```

Same for constructors in `pattern.rs` and types in `types.rs`. Every
other error is unchanged.

`VarGlobal` resolution:

```rust
SourceExpr::VarGlobal { package, module, name } => {
    let interface = env.global_interface(package, module)
        .ok_or(Error::MacroGlobalNotFound { region, module, name })?;
    let value = interface.values.iter().find(|v| v.name == *name)
        .ok_or(Error::MacroGlobalNotFound { region, module, name })?;
    Ok(CanExpr::VarForeign {
        reference: QualifiedName { home: interface.home, name: value.name },
        annotation: value.annotation,
    })
}
```

`Env::global_interface` looks in `Context.interfaces` by module name (the
map is build-wide, see `crates/nash-driver/src/compile.rs:86`).

Quote desugaring (`crates/nash-can/src/quote.rs`): canonicalize the quoted
expression **in the current env** (so free names resolve here), then
convert the canonical tree into a canonical expression that *builds* the
`Ast` value. The `Ast` constructors are looked up through the interface of
`Ast`; if `Ast` is not imported the error is `QuoteWithoutAst`.

```rust
pub(crate) fn quote<'a>(
    bump: &'a Bump,
    env: &Env<'a>,
    inner: &'a Located<CanExpr<'a>>,
) -> Result<CanExpr<'a>, Error<'a>> {
    let ast = AstCtors::lookup(env, inner.region)?;
    Ok(Quoter { bump, ast, env }.expr(inner))
}

struct Quoter<'a, 'e> { bump: &'a Bump, ast: AstCtors<'a>, env: &'e Env<'a> }

impl<'a> Quoter<'a, '_> {
    fn expr(&self, e: &'a Located<CanExpr<'a>>) -> CanExpr<'a> {
        let node = match &e.value {
            CanExpr::Int(n) => self.ctor("IntLit", &[CanExpr::Int(*n)]),
            CanExpr::Str(s) => self.ctor("StrLit", &[CanExpr::Str(s)]),
            CanExpr::VarLocal(name) => self.ctor("Var", &[self.name_local(name)]),
            CanExpr::VarTopLevel(q) | CanExpr::VarForeign { reference: q, .. } => {
                self.ctor("Var", &[self.name_global(*q)])
            }
            CanExpr::Binop { reference, left, right, .. } => self.ctor(
                "BinOp",
                &[self.name_global(*reference), self.expr_ref(left), self.expr_ref(right)],
            ),
            CanExpr::Lambda { parameters, body } => self.ctor(
                "Lambda",
                &[self.cons_list(parameters.iter().map(|p| self.pattern(p))), self.expr_ref(body)],
            ),
            CanExpr::Call { function, arguments } => self.ctor(
                "Call",
                &[self.expr_ref(function), self.cons_list(arguments.iter().map(|a| self.expr_ref(a)))],
            ),
            // Splice: the user's expression already evaluates to an Ast.expr.
            CanExpr::Splice(inner) => return inner.value_copy(),
            // ...one arm per variant, mirroring Reifier::expr in chunk 4...
        };
        self.wrap_expr(node)   // Ast.Expr { span = None, typ = None } node
    }

    /// `Cons x0 (Cons x1 ... Nil)` built from the `Cons` module's constructors.
    fn cons_list(&self, items: impl Iterator<Item = CanExpr<'a>>) -> CanExpr<'a> { ... }

    fn name_local(&self, s: &str) -> CanExpr<'a> {
        // Binders inside the quote are hygienic.
        self.ctor("Local", &[CanExpr::Str(s)])
    }
}
```

`CanExpr::Splice` exists only between canonicalizing the quote body and
running `Quoter`; `Quoter` consumes it. A `Splice` outside a quote cannot
occur (parser rejects it).

Also: `canonicalize_decls` skips macro definitions for SCC purposes? No.
Macro bodies are ordinary top-level definitions (they may call helpers),
so they are part of `Decls`; `Module.macros` just points at them with
their shape. The interface exports them as values too, so `nash-codegen`
can lower them like any value.

**Elm/Aiken reference**

`Canonicalize/Expression.hs` `findVar`, `findVarQual`;
`Canonicalize/Environment/Foreign.hs` `createInitialEnv` (macro
entries follow `addExposedValue`); `Elm/Interface.hs` `fromModule`
(macro list next to `values`).

**Tests** (`crates/nash-can/src/snapshots`)

- `assert_can_snapshot!` module declaring `macro m : cons Ast.expr -> Ast.expr` with `import Ast` and `import Cons` interface stubs.
- `assert_can_error_snapshot!` same module invoking `m!(1)` → `MacroSameModule`.
- Importing module using `@Derive.derive(Eq)` on a union → `macro_uses` has one `Decl` use; snapshot it.
- `m!(x)` where `m` is a value → `MacroNotAMacro`; a decl macro used as `m!()` → `MacroWrongKind`.
- Lenient: `main = missing 1` with `Mode::Lenient` canonicalizes to `Call(Hole, [1])`; `Mode::Strict` errors as today.
- Quote: `quote (f ~x 1)` with `f` foreign snapshot shows `Ast.Call (Ast.Var (Global ..f)) [x, Ast.Int 1]`.

**Done when** all snapshot tests pass and `nash-driver` still compiles
(pass `Mode::Strict` in `compile_module`).

---

## Chunk 3: reuse solved node types and add lenient solving

`crates/nash-solve/src/solved.rs` already exposes `SolvedTypes.exprs`, `.patterns`,
`.instances`, and `.schemes`, keyed by `nash_ast::NodeId`. The driver retains this
information for codegen. No parallel NodeTypes registry is needed.

- [ ] Feed existing solved types and canonical kind/representation metadata to
  the reifier; do not introduce a second inference engine.
- [ ] Add specific lenient-mode allowances for macro-introduced names and impls.
  Keep ordinary unification errors and strictly recheck output using fresh node
  identities, not stale input pointers or provisional macro-result types.
- [ ] Use `None` for genuinely unresolved metadata rather than promising concrete
  types for every provisional hole. Update the AST input contract accordingly.
- [ ] Test typed lambda pattern/body input, unresolved evidence, and strict output
  rechecking against the existing codegen type queries.
- [ ] Apply the final-only coverage contract above. Test that surviving refutable
  runtime lambdas and nonexhaustive/redundant generated cases are rejected.

---

## Chunk 4: `nash-macro` crate and the reifier (`nash_ast` → `Term::Constr`)

**Files**

- `crates/nash-macro/Cargo.toml` (new; deps: `nash-ast`, `nash-source`, `nash-region`, `nash-plutus`, `nash-can`, `nash-solve`, `bumpalo`)
- `crates/nash-macro/src/lib.rs`, `tags.rs`, `reify.rs`
- `crates/nash-driver/base/src/Ast.nash`, `crates/nash-driver/base/src/Cons.nash` (plans/12 chunks 10 and 6 — same PR)

**Change**

Build, for a `MacroUse`, the UPLC `Term::Constr` tree that *is* the
little `Ast` value the macro expects, matching `crates/nash-driver/base/src/Ast.nash`. The
tree is arena-allocated (`nash_plutus::arena::Arena`), constructor tags
are declaration indices, leaves are `Constant::String`/`Integer`/
`ByteString` constants, child lists are `Cons`/`Nil` chains, and the
result is handed to the CEK machine with `Term::apply` (chunk 7). No
`PlutusData` is built anywhere.

`tags.rs` is the single place the two sides agree. Constructor tags are
declaration indices in `Ast.nash`; keep the two files side by side.

**Code**

```rust
// crates/nash-macro/src/tags.rs
pub mod cons { pub const NIL: usize = 0; pub const CONS: usize = 1; }          // core/Cons.nash
pub mod name { pub const LOCAL: u64 = 0; pub const RAW: u64 = 1; pub const GLOBAL: u64 = 2; }
pub mod kind { pub const TYPE: u64 = 0; pub const ARROW: u64 = 1; }
pub mod repr { pub const BIG: u64 = 0; pub const CONST: u64 = 1; pub const TERM: u64 = 2; }
pub mod repr_annotation { pub const REPR: u64 = 0; pub const STORABLE: u64 = 1; }
pub mod expr {
    pub const INT: u64 = 0; pub const STR: u64 = 1; pub const BYTES: u64 = 2; pub const VAR: u64 = 3;
    pub const OP: u64 = 4; pub const LIST: u64 = 5; pub const NEGATE: u64 = 6; pub const BINOP: u64 = 7;
    pub const LAMBDA: u64 = 8; pub const CALL: u64 = 9; pub const IF: u64 = 10; pub const LET: u64 = 11;
    pub const CASE: u64 = 12; pub const ACCESSOR: u64 = 13; pub const ACCESS: u64 = 14; pub const UPDATE: u64 = 15;
    pub const RECORD: u64 = 16; pub const UNIT: u64 = 17; pub const TUPLE: u64 = 18; pub const MACRO_CALL: u64 = 19;
    pub const COMPTIME: u64 = 20; pub const INTEGER_DISPATCH: u64 = 21;
}
pub mod def { pub const DEFINE: u64 = 0; pub const DESTRUCT: u64 = 1; }
pub mod pattern {
    pub const ANY: u64 = 0; pub const VAR: u64 = 1; pub const RECORD: u64 = 2; pub const ALIAS: u64 = 3;
    pub const UNIT: u64 = 4; pub const TUPLE: u64 = 5; pub const CTOR: u64 = 6; pub const LIST: u64 = 7;
    pub const CONS: u64 = 8; pub const INT: u64 = 9; pub const STR: u64 = 10; pub const BYTES: u64 = 11;
}
pub mod typ {
    pub const VAR: u64 = 0; pub const CON: u64 = 1; pub const FUN: u64 = 2; pub const RECORD: u64 = 3;
    pub const TUPLE: u64 = 4; pub const UNIT: u64 = 5;
}
pub mod decl {
    pub const VALUE: u64 = 0; pub const UNION: u64 = 1; pub const ALIAS: u64 = 2; pub const TRAIT: u64 = 3;
    pub const IMPL: u64 = 4; pub const INFIX: u64 = 5;
}
pub mod assoc { pub const LEFT: u64 = 0; pub const RIGHT: u64 = 1; pub const NON: u64 = 2; }
pub mod option { pub const SOME: u64 = 0; pub const NONE: u64 = 1; }
```

Matching Nash (`crates/nash-driver/base/src/Ast.nash`, little types, order is load-bearing;
the full listing is docs/macros.md "What the macro sees"):

```elm
type name = Local string | Raw string | Global modname string
type alias modname = { package : option string, name : string }
type kind = Type | Arrow kind kind
type repr = Big | Const | Term
type reprAnnotation = Repr repr | Storable
type alias meta = { span : option span, typ : option typ }
type expr = Expr meta exprNode
type exprNode
    = IntLit int | StrLit string | BytesLit bytes | Var name | Op name | ListLit (cons expr)
    | Negate expr | BinOp name expr expr | Lambda (cons pattern) expr | Call expr (cons expr)
    | If expr expr expr | Let (cons def) expr | Case expr (cons arm) | Accessor string
    | Access expr string | Update name (cons fieldAssign) | Record (cons fieldAssign)
    | UnitLit | Tuple (cons expr) | MacroCall name (cons expr) | Comptime expr
    | IntegerDispatch expr (cons expr)
type def = Define name (cons pattern) expr (option typ) | Destruct pattern expr
type patternNode
    = PAny | PVar name | PRecord (cons name) | PAlias pattern name | PUnit | PTuple (cons pattern)
    | PCtor name (cons pattern) | PList (cons pattern) | PCons pattern pattern
    | PInt int | PStr string | PBytes bytes
type typ = TVar string | TCon name (cons typ) | TFun typ typ | TRecord (cons field) | TTuple (cons typ) | TUnit
type decl = Value {..} | Union {..} | Alias {..} | Trait traitDef | Impl implDef | Infix {..}
type assoc = LeftAssoc | RightAssoc | NonAssoc
```

Runtime layout (representation.md "Term types"): a constructor is
`constr i [fields]`, a record alias (`meta`, `modname`, `span`, `arm`,
...) is `constr 0 [fields]` in field order, a labeled constructor
(`Value {..}`) is `constr i [fields]` flat, and `cons` cells are
`constr 0 []` / `constr 1 [x, xs]`.

Reifier:

```rust
// crates/nash-macro/src/reify.rs
use nash_ast::{Expr as CanExpr, Pattern as CanPattern, Type as CanType, ModuleName, QualifiedName};
use nash_plutus::{arena::Arena, binder::DeBruijn, constant::{Constant, Integer}, term::Term};
use nash_solve::SolvedTypes;
use nash_constrain::NodeId;

type T<'p> = &'p Term<'p, DeBruijn>;

pub struct Reifier<'p, 'a> {
    arena: &'p Arena,
    types: &'a SolvedTypes<'a>,
    bump: &'a bumpalo::Bump,
    type_env: &'a nash_can::kinds::KindEnv<'a>,
}

impl<'p, 'a> Reifier<'p, 'a> {
    pub fn decl_use(&self, target: &'a DeclSnapshot<'a>, attr: &'a nash_ast::Attribute<'a>) -> (T<'p>, T<'p>) {
        let decl = self.decl(target);
        let args = self.cons_list(attr.arguments.iter().map(|a| self.surface_expr(a)));
        (decl, args)
    }

    pub fn expr_use(&self, arguments: &'a [&'a Located<CanExpr<'a>>]) -> T<'p> {
        self.cons_list(arguments.iter().map(|a| self.expr(a)))
    }

    pub fn expr(&self, e: &'a Located<CanExpr<'a>>) -> T<'p> {
        let typ = self.types.get(&NodeId::of(e)).map(|t| self.typ(t));
        let meta = self.meta(Some(e.region), typ);
        let node = match &e.value {
            CanExpr::Int(n) => self.constr(tags::expr::INT, &[self.int(*n)]),
            CanExpr::Str(s) => self.constr(tags::expr::STR, &[self.str(s)]),
            CanExpr::VarLocal(name) => self.constr(tags::expr::VAR, &[self.raw(name)]),
            CanExpr::VarTopLevel(q) => self.constr(tags::expr::VAR, &[self.global(*q)]),
            CanExpr::VarForeign { reference, .. } => self.constr(tags::expr::VAR, &[self.global(*reference)]),
            CanExpr::VarConstructor { reference, .. } => self.constr(
                tags::expr::VAR,
                &[self.global(QualifiedName { home: reference.home, name: reference.name })],
            ),
            CanExpr::VarOperator { reference, .. } => self.constr(tags::expr::OP, &[self.global(*reference)]),
            CanExpr::List(items) => self.constr(tags::expr::LIST, &[self.cons_list(items.iter().map(|i| self.expr(i)))]),
            CanExpr::Negate(inner) => self.constr(tags::expr::NEGATE, &[self.expr(inner)]),
            CanExpr::Binop { reference, left, right, .. } => self.constr(
                tags::expr::BINOP,
                &[self.global(*reference), self.expr(left), self.expr(right)],
            ),
            CanExpr::Lambda { parameters, body } => self.constr(
                tags::expr::LAMBDA,
                &[self.cons_list(parameters.iter().map(|p| self.pattern(p))), self.expr(body)],
            ),
            CanExpr::Call { function, arguments } => self.constr(
                tags::expr::CALL,
                &[self.expr(function), self.cons_list(arguments.iter().map(|a| self.expr(a)))],
            ),
            CanExpr::If { branches, final_else } => self.if_chain(branches, final_else),
            CanExpr::Let { definition, body } => self.constr(
                tags::expr::LET,
                &[self.cons_list([self.def(definition)]), self.expr(body)],
            ),
            CanExpr::LetRec { definitions, body } => self.constr(
                tags::expr::LET,
                &[self.cons_list(definitions.iter().map(|d| self.def(d))), self.expr(body)],
            ),
            CanExpr::LetDestruct { pattern, value, body } => self.constr(
                tags::expr::LET,
                &[self.cons_list([self.constr(tags::def::DESTRUCT, &[self.pattern(pattern), self.expr(value)])]), self.expr(body)],
            ),
            CanExpr::Case { scrutinee, branches } => self.constr(
                tags::expr::CASE,
                &[self.expr(scrutinee), self.cons_list(branches.iter().map(|b| self.arm(b)))],
            ),
            CanExpr::Accessor(field) => self.constr(tags::expr::ACCESSOR, &[self.str(field)]),
            CanExpr::Access { record, field } => self.constr(tags::expr::ACCESS, &[self.expr(record), self.str(field.value)]),
            CanExpr::Update { record, fields, .. } => self.constr(
                tags::expr::UPDATE,
                &[self.raw(record), self.cons_list(fields.iter().map(|f| self.field_assign(f.field.value, f.value)))],
            ),
            CanExpr::Record(fields) => self.constr(
                tags::expr::RECORD,
                &[self.cons_list(fields.iter().map(|f| self.field_assign(f.field.value, f.value)))],
            ),
            CanExpr::Unit => self.constr(tags::expr::UNIT, &[]),
            CanExpr::Tuple { first, second, rest } => self.constr(
                tags::expr::TUPLE,
                &[self.cons_list([first, second].into_iter().chain(rest.iter().copied()).map(|e| self.expr(e)))],
            ),
            CanExpr::MacroCall { reference, arguments } => self.constr(
                tags::expr::MACRO_CALL,
                &[self.global(*reference), self.cons_list(arguments.iter().map(|a| self.expr(a)))],
            ),
            CanExpr::Comptime(inner) => self.constr(tags::expr::COMPTIME, &[self.expr(inner)]),
            // Provisional unresolved syntax must retain its name, scope and origin.
            // Encode that metadata with an absent type; the concrete node shape
            // is defined by the shared transport contract before implementation.
            // A payload-free Hole/unreachable arm is not a valid implementation.
        };
        self.constr(0, &[meta, node])   // `Expr meta node`
    }

    fn if_chain(&self, branches: &'a [nash_ast::IfBranch<'a>], final_else: &'a Located<CanExpr<'a>>) -> T<'p> {
        let (first, rest) = branches.split_first().expect("If has at least one branch");
        let else_ = if rest.is_empty() {
            self.expr(final_else)
        } else {
            self.constr(0, &[self.meta(None, None), self.if_chain(rest, final_else)])
        };
        self.constr(tags::expr::IF, &[self.expr(first.condition), self.expr(first.then_branch), else_])
    }

    // --- names ---

    fn raw(&self, s: &str) -> T<'p> {
        self.constr(tags::name::RAW, &[self.str(s)])
    }

    fn global(&self, q: QualifiedName<'a>) -> T<'p> {
        self.constr(tags::name::GLOBAL, &[self.module(q.home), self.str(q.name)])
    }

    /// `modname` is a little record alias: `constr 0 [package, name]`.
    fn module(&self, m: ModuleName<'a>) -> T<'p> {
        let package = match m.package {
            Some(p) => self.some(self.str(&format!("{}/{}", p.author, p.project))),
            None => self.none(),
        };
        self.record(&[package, self.str(m.name)])
    }

    // --- primitives ---

    fn meta(&self, span: Option<Region>, typ: Option<T<'p>>) -> T<'p> {
        let span = match span {
            Some(r) => self.some(self.record(&[
                self.int(r.start.line as i128), self.int(r.start.column as i128),
                self.int(r.end.line as i128), self.int(r.end.column as i128),
            ])),
            None => self.none(),
        };
        let typ = typ.map_or_else(|| self.none(), |t| self.some(t));
        self.record(&[span, typ])
    }

    fn constr(&self, tag: u64, fields: &[T<'p>]) -> T<'p> {
        Term::constr(self.arena, tag as usize, self.arena.alloc_slice_copy(fields))
    }
    /// Little record alias: `constr 0 [fields]` in declaration order.
    fn record(&self, fields: &[T<'p>]) -> T<'p> { self.constr(0, fields) }
    /// `Cons x0 (Cons x1 (... Nil))`, built from the tail.
    fn cons_list(&self, items: impl IntoIterator<Item = T<'p>>) -> T<'p> {
        let items: Vec<T<'p>> = items.into_iter().collect();
        items.iter().rev().fold(self.constr(tags::cons::NIL as u64, &[]), |tail, x| {
            self.constr(tags::cons::CONS as u64, &[x, tail])
        })
    }
    fn some(&self, t: T<'p>) -> T<'p> { self.constr(tags::option::SOME, &[t]) }
    fn none(&self) -> T<'p> { self.constr(tags::option::NONE, &[]) }
    fn int(&self, n: i128) -> T<'p> {
        Term::constant(self.arena, self.arena.alloc(Constant::Integer(self.arena.alloc(Integer::from(n)))))
    }
    fn str(&self, s: &str) -> T<'p> {
        Term::constant(self.arena, self.arena.alloc(Constant::String(self.arena.alloc_str(s))))
    }
    fn bytes(&self, b: &[u8]) -> T<'p> {
        Term::constant(self.arena, self.arena.alloc(Constant::ByteString(self.arena.alloc_slice_copy(b))))
    }
}
```

`DeclSnapshot` is the canonical view of one decorated declaration
(`Def` + annotation for values, `Union`/`Alias` with closed kinds and
separate representation metadata from plans/02, `Trait`/`Impl` from plans/03). `surface_expr` reifies attribute
arguments from `nash_source::Expr` with `typ = None` and `Raw` names.
`typ` reifies `CanType` with `Alias` expanded through `AliasType::Filled`.

Check `Arena` has `alloc_slice_copy` and `alloc_str`
(`crates/nash-plutus/src/arena.rs`); add them if not. `Term::constr` and
`Term::constant` exist (`term.rs:70-76`).

**Elm/Aiken reference**

`crates/nash-plutus/src/term.rs` `Term::constr` and the `Value::Constr`
arm of `machine/discharge.rs` are the two directions of the same layout.
Aiken does not reify its AST; Elm has none.

**Tests** (`crates/nash-macro/src/reify.rs` tests)

Structural snapshots of the `Term` via `format!("{:?}")` (round trips are
chunk 5):

- `reify_int_literal`: `1` → `Constr 0 [Constr 0 [Some (Constr 0 [ints..]), Some (Constr 1 [Global Builtin "int", Nil])], Constr 0 [(con integer 1)]]`.
- `reify_lambda_local_binder`: `\x -> x` → binder and use both `Raw "x"`; the parameter list is `Cons (..) Nil`.
- `reify_foreign_var`: `List.map` → `Global {package: Some "nash/base", name: "List"} "map"`.
- `reify_if_chain`: `if a then 1 else if b then 2 else 3` → nested `If`.
- `reify_union_decl`: `type Foo 'a = A 'a | B` → `Union` with `kind = Some (Arrow Type Type)` and `representation = Some Big`.
- `reify_no_data`: no `Constant::Data` anywhere in the tree for any input (walk and assert).

**Done when** the crate builds in the workspace and the tests pass.

---

## Chunk 5: unreifier: CEK result `Term` → `nash_source`

**Files**

- `crates/nash-macro/src/unreify.rs`
- `crates/nash-macro/src/error.rs`

**Change**

Walk the macro's result back into surface AST allocated in the module
bump. The CEK machine returns the result `Value` already discharged to a
`Term` (`EvalResult.term`, via `machine/discharge.rs` `value_as_term`), so
the walker reads `Term::Constr { tag, fields }` nodes and
`Term::Constant` leaves and nothing else: a `Term::Lambda`, `Delay`,
`Builtin`/`Apply`/`Force` (a partially applied builtin), or a constant
where a constructor is expected is `UnreifyError::NotConstr`, reported as
`MacroBadOutput`. Every node gets the invocation region. Names decode per
docs/macros.md: `Raw` → plain; `Global` → `VarGlobal`/`CtorGlobal`/
`TypeGlobal`; `Local` → the text with a trailing `·` (U+00B7), renamed by
chunk 6.

**Code**

```rust
// crates/nash-macro/src/error.rs
#[derive(Debug)]
pub enum UnreifyError {
    /// `path` is the chain of constructor names from the root, for the diagnostic.
    BadShape { path: Vec<&'static str>, found: String },
    /// A node position held something that is not a `constr` tree.
    NotConstr { path: Vec<&'static str>, found: &'static str },   // "lambda" | "delay" | "builtin" | "constant"
    GlobalBinder { name: String },
    TupleTooShort { len: usize },
}

// crates/nash-macro/src/unreify.rs
type T<'p> = &'p Term<'p, DeBruijn>;

pub struct Unreifier<'a> {
    bump: &'a Bump,
    site: Region,
    path: Vec<&'static str>,
}

pub const LOCAL_MARK: char = '\u{00B7}';

impl<'a> Unreifier<'a> {
    pub fn new(bump: &'a Bump, site: Region) -> Self { Self { bump, site, path: Vec::new() } }

    pub fn decls(&mut self, t: T<'_>) -> Result<Vec<DecodedDecl<'a>>, UnreifyError> {
        self.cons_of(t, "cons decl", |s, x| s.decl(x))
    }

    /// `Term::Constr` or an error naming what was found instead.
    fn constr<'p>(&mut self, t: T<'p>, what: &'static str) -> Result<(usize, &'p [T<'p>]), UnreifyError> {
        self.path.push(what);
        match t {
            Term::Constr { tag, fields } => Ok((*tag, fields)),
            Term::Lambda { .. } => Err(self.not_constr("lambda")),
            Term::Delay(_) => Err(self.not_constr("delay")),
            Term::Builtin(_) | Term::Apply { .. } | Term::Force(_) => Err(self.not_constr("builtin")),
            Term::Constant(_) => Err(self.not_constr("constant")),
            // a discharged value is closed and fully evaluated
            Term::Var(_) | Term::Case { .. } | Term::Error => unreachable!("not a value"),
        }
    }

    /// Walk a `Cons x rest` chain to `Nil`.
    fn cons_of<'p, X>(&mut self, mut t: T<'p>, what: &'static str, f: impl Fn(&mut Self, T<'p>) -> Result<X, UnreifyError>) -> Result<Vec<X>, UnreifyError> {
        let mut out = Vec::new();
        loop {
            match self.constr(t, what)? {
                (tags::cons::NIL, []) => return Ok(out),
                (tags::cons::CONS, [x, rest]) => { out.push(f(self, x)?); t = rest; }
                (tag, fields) => return Err(self.bad_shape(format!("cons tag {tag} with {} fields", fields.len()))),
            }
        }
    }

    fn str(&mut self, t: T<'_>) -> Result<&'a str, UnreifyError> {
        match t {
            Term::Constant(Constant::String(s)) => Ok(self.bump.alloc_str(s)),
            other => Err(self.bad_shape(format!("expected a string constant, found {other:?}"))),
        }
    }
    // `int` (`Constant::Integer`), `bytes` (`Constant::ByteString`), and
    // `option` (`SOME [x]` / `NONE []`) follow the same shape.

    pub fn expr(&mut self, t: T<'_>) -> Result<&'a Located<SourceExpr<'a>>, UnreifyError> {
        let [_meta, node] = self.fields(t, 0, "expr")? else { unreachable!() };
        let (tag, fields) = self.constr(node, "exprNode")?;
        let value = match (tag, fields) {
            (tags::expr::INT, [n]) => SourceExpr::Int(self.int(n)?),
            (tags::expr::STR, [s]) => SourceExpr::Str(self.str(s)?),
            (tags::expr::VAR, [name]) => self.var(name)?,
            (tags::expr::OP, [name]) => SourceExpr::Op(self.raw_or_global_text(name)?),
            (tags::expr::LIST, [items]) => SourceExpr::List(self.exprs(items)?),
            (tags::expr::NEGATE, [e]) => SourceExpr::Negate(self.expr(e)?),
            (tags::expr::BINOP, [op, l, r]) => self.binop(op, l, r)?,
            (tags::expr::LAMBDA, [params, body]) => SourceExpr::Lambda {
                parameters: self.patterns(params)?,
                body: self.expr(body)?,
            },
            (tags::expr::CALL, [f, args]) => SourceExpr::Call {
                function: self.expr(f)?,
                arguments: self.exprs(args)?,
            },
            (tags::expr::IF, [c, t, e]) => SourceExpr::If {
                branches: self.bump.alloc_slice_copy(&[self.bump.alloc(IfBranch {
                    condition: self.expr(c)?,
                    then_branch: self.expr(t)?,
                }) as &_]),
                final_else: self.expr(e)?,
            },
            (tags::expr::LET, [defs, body]) => SourceExpr::Let {
                defs: self.cons_of_refs(defs, "cons def", |s, x| s.def(x))?,
                body: self.expr(body)?,
            },
            (tags::expr::CASE, [scrut, arms]) => SourceExpr::Case {
                scrutinee: self.expr(scrut)?,
                arms: self.cons_of_refs(arms, "cons arm", |s, x| s.arm(x))?,
            },
            (tags::expr::UNIT, []) => SourceExpr::Unit,
            (tags::expr::TUPLE, [items]) => {
                let items = self.exprs(items)?;
                let [first, second, rest @ ..] = items else {
                    return Err(UnreifyError::TupleTooShort { len: items.len() });
                };
                SourceExpr::Tuple { first, second, rest }
            }
            (tags::expr::MACRO_CALL, [name, args]) => self.macro_call(name, args)?,
            (tags::expr::COMPTIME, [e]) => SourceExpr::Comptime(self.expr(e)?),
            // ...ACCESSOR, ACCESS, UPDATE, RECORD, BYTES...
            (tag, fields) => return Err(self.bad_shape(format!("exprNode tag {tag} with {} fields", fields.len()))),
        };
        Ok(self.at(value))
    }

    /// `Var` with the three name modes.
    fn var(&mut self, name: T<'_>) -> Result<SourceExpr<'a>, UnreifyError> {
        Ok(match self.name(name)? {
            Name::Local(s) => SourceExpr::Var { kind: var_type(s), name: self.local(s) },
            Name::Raw(s) => SourceExpr::Var { kind: var_type(s), name: self.str_alloc(s) },
            Name::Global { package, module, name } => SourceExpr::VarGlobal { package, module, name },
        })
    }

    fn local(&self, s: &str) -> &'a str {
        let mut owned = String::with_capacity(s.len() + 2);
        owned.push_str(s);
        owned.push(LOCAL_MARK);
        self.bump.alloc_str(&owned)
    }

    /// `BinOp fn l r` becomes a two-operand `BinOps` chain using the operator
    /// symbol the invocation module will see: for a `Global` function name the
    /// decoder emits a `Call (VarGlobal fn) [l, r]` instead, because operator
    /// symbols are not stable across modules.
    fn binop(&mut self, op: T<'_>, l: T<'_>, r: T<'_>) -> Result<SourceExpr<'a>, UnreifyError> {
        let function = match self.name(op)? {
            Name::Global { package, module, name } => self.at(SourceExpr::VarGlobal { package, module, name }),
            Name::Raw(s) | Name::Local(s) => self.at(SourceExpr::Var { kind: VarType::LowVar, name: self.str_alloc(s) }),
        };
        Ok(SourceExpr::Call {
            function,
            arguments: self.bump.alloc_slice_copy(&[self.expr(l)?, self.expr(r)?]),
        })
    }

    fn at<T>(&self, value: T) -> &'a Located<T> {
        self.bump.alloc(Located::at(self.site, value))
    }

    fn name(&mut self, t: T<'_>) -> Result<Name<'a>, UnreifyError> {
        let (tag, fields) = self.constr(t, "name")?;
        Ok(match (tag, fields) {
            (tags::name::LOCAL, [s]) => Name::Local(self.str(s)?),
            (tags::name::RAW, [s]) => Name::Raw(self.str(s)?),
            (tags::name::GLOBAL, [m, s]) => {
                // `modname` record alias: constr 0 [package, name]
                let [package, module] = self.fields(m, 0, "modname")? else { unreachable!() };
                Name::Global {
                    package: self.option(package, |s, x| s.str(x))?,
                    module: self.str(module)?,
                    name: self.str(s)?,
                }
            }
            _ => return Err(self.bad_shape("name")),
        })
    }
}

enum Name<'a> {
    Local(&'a str),
    Raw(&'a str),
    Global { package: Option<&'a str>, module: &'a str, name: &'a str },
}

pub enum DecodedDecl<'a> {
    Value(&'a Located<Value<'a>>),
    Union(&'a Located<Union<'a>>),
    Alias(&'a Located<Alias<'a>>),
    Trait(&'a Located<Trait<'a>>),
    Impl(&'a Located<Impl<'a>>),
    Infix(&'a Located<Infix<'a>>),
}
```

Binder positions (`PVar`, `Define` name, lambda params via `PVar`,
`Union.name`, `ctor.name`, `Value.name`) reject `Global` with
`UnreifyError::GlobalBinder`.

Strings are native `Constant::String` (`&str`), so there is no UTF-8
check; ints are `Constant::Integer` (`BigInt`, converted with a range
check for spans and precedences); bytes are `Constant::ByteString`.

**Elm/Aiken reference**

`crates/nash-plutus/src/machine/discharge.rs` `value_as_term` is what
turns the CEK `Value` (with `Value::Constr(tag, fields)`) into the `Term`
this walker reads; `EvalResult.term` already carries it. Aiken's
`test_framework.rs` `Prng::from_result` is the analogous read-back of a
structured result, over `PlutusData` rather than `constr`.

**Tests** (`crates/nash-macro/src/unreify.rs`, round trips with chunk 4;
no CEK evaluation is needed because both sides are `Term`)

- `roundtrip_expr`: parse → canonicalize → reify → unreify → print equals
  the printed original for `\x -> if x then 1 else f x 2`, `case m of Some y -> y; None -> 0`,
  `let a = 1 in a + a`, `{ r | f = 1 }`, `(a, b, c)`.
- `unreify_local_marks`: `Lambda (Cons (PVar (Local "x")) Nil) (Var (Local "x"))` decodes to `\x· -> x·`.
- `unreify_global_binder_rejected`.
- `unreify_bad_shape_reports_path`: tag 99 at `expr/Call/arguments[1]`.
- `unreify_lambda_rejected`: a `Term::Lambda` in an argument position is `NotConstr { found: "lambda" }`.

**Done when** round trips are byte-identical under the debug printer of
chunk 12 (written first as a test helper, promoted in chunk 12).

---

## Chunk 6: hygiene (gensym pass)

**Files**

- `crates/nash-macro/src/hygiene.rs`

**Change**

Rename every `x·` name produced by the unreifier to `x·{round}_{use}` so
that each expansion's locals are distinct from user names and from other
expansions. Preserve separate binder scopes when combining caller fragments; follow the
binding contract at the top of this plan. The string-based sketch below only
illustrates fresh-name rendering, not a complete binding-identity algorithm.

**Code**

```rust
pub struct Gensym {
    pub round: u32,
    pub use_index: u32,
}

impl Gensym {
    pub fn rename<'a>(&self, bump: &'a Bump, decls: Vec<DecodedDecl<'a>>) -> Vec<DecodedDecl<'a>> { /* rebuild */ }
    pub fn rename_expr<'a>(&self, bump: &'a Bump, e: &'a Located<SourceExpr<'a>>) -> &'a Located<SourceExpr<'a>> { /* rebuild */ }

    fn name<'a>(&self, bump: &'a Bump, s: &'a str) -> &'a str {
        match s.strip_suffix(LOCAL_MARK) {
            Some(base) => bump.alloc_str(&format!("{base}{LOCAL_MARK}{}_{}", self.round, self.use_index)),
            None => s,
        }
    }
}
```

The walk is a full surface-AST rebuild (the arena AST is immutable);
it touches `Expr::Var`, `Pattern::Var`, `Pattern::Alias`, `Def::Define`
names, `Value.name`, `Union.name`, `Ctor.name`, `Alias.name`, and
record field names in `Pattern::Record` (a `Local` field name is an
error: fields are not binders — `UnreifyError::LocalField`).

Diagnostics: `nash-report` renders a `NotFoundVar` whose name contains
`LOCAL_MARK` as `MacroUnboundLocal` (chunk 11).

**Elm/Aiken reference**

None. Conceptually Racket's "marks", simplified to one mark per expansion.

**Tests** (`crates/nash-macro/src/hygiene.rs`)

- `renames_binder_and_use_consistently`: `\x· -> x·` → `\x·1_0 -> x·1_0`.
- `leaves_raw_alone`: `\x -> x·` → `\x -> x·1_0`.
- `two_uses_differ`: use 0 and use 1 of the same output give different names.
- Expansion-level test (chunk 12): a macro `let x = 1 in ~body` invoked with
  `body = x + 1` where the user's `x = 10` yields `11`, not `2`.

**Done when** tests pass and the expansion test is green after chunk 8.

---

## Chunk 7: compile macros to standalone programs

**Files**

- `crates/nash-codegen/src/macro_.rs` (new)
- `crates/nash-codegen/src/lib.rs`
- `crates/nash-macro/src/run.rs` (new)

**Change**

`nash-codegen` produces a UPLC program per macro. `nash-macro::run`
applies it to the reified input terms under a budget and returns the
result `Term` (the discharged value) or the failure message.

**Compilation contract**

Use the current `Build` specialization and dependency closure, followed by
ordinary closed Core assembly and Flat encoding. Compile an exported macro root
with its reachable definitions; do not inline every dependency or recreate the
removed `ModuleSet`/`lower_value` APIs. Retain owned encoded programs across module
arenas and use shape metadata from the defining module's interface.

The following execution sketch illustrates the CEK boundary; adapt signatures and
errors to the current APIs rather than introducing a second evaluator.

```rust
// crates/nash-macro/src/run.rs
pub struct MacroRun<'p> {
    /// The discharged result value (`EvalResult.term`); chunk 5 walks it.
    pub output: &'p Term<'p, DeBruijn>,
    pub budget: ExBudget,
    pub logs: Vec<String>,
}

pub enum MacroRunError {
    /// `message` is the last trace line, if any; `machine` the CEK error text.
    Failed { message: Option<String>, machine: String, logs: Vec<String> },
}

pub fn run_decl_macro<'p>(
    arena: &'p Arena,
    program: &[u8],
    decl: &'p Term<'p, DeBruijn>,
    args: &'p Term<'p, DeBruijn>,
    budget: ExBudget,
) -> Result<MacroRun<'p>, MacroRunError> {
    let program = nash_plutus::flat::decode::<DeBruijn>(arena, program).expect("macro program was encoded by this compiler");
    let applied = program.apply(arena, decl).apply(arena, args);   // Program::apply wraps Term::Apply
    finish(applied.eval_version_budget(arena, PlutusVersion::V3, budget))
}

pub fn run_expr_macro<'p>(arena: &'p Arena, program: &[u8], args: &'p Term<'p, DeBruijn>, budget: ExBudget) -> Result<MacroRun<'p>, MacroRunError> {
    let program = nash_plutus::flat::decode::<DeBruijn>(arena, program).expect("macro program was encoded by this compiler");
    finish(program.apply(arena, args).eval_version_budget(arena, PlutusVersion::V3, budget))
}

fn finish<'p>(result: EvalResult<'p, DeBruijn>) -> Result<MacroRun<'p>, MacroRunError> {
    let logs = result.info.logs;
    match result.term {
        Ok(term) => Ok(MacroRun { output: term, budget: result.info.consumed_budget, logs }),
        Err(e) => Err(MacroRunError::Failed { message: logs.last().cloned(), machine: format!("{e}"), logs }),
    }
}
```

The argument terms are closed `constr`/constant trees, so applying them
to a De Bruijn program needs no index shifting. Whether the result has
the right shape is chunk 5's job; `run` does not inspect it.

Match the real `Term`/`Constant` variant names in
`crates/nash-plutus/src/term.rs:9` and `constant.rs`.

**Elm/Aiken reference**

Aiken `crates/aiken-lang/src/gen_uplc.rs` `generate_raw` (compile one
definition with its dependencies to a `Program`), `crates/aiken-project/src/lib.rs`
`run_tests` (apply, eval with budget, keep logs).

**Tests** (`crates/nash-codegen/tests/macro_.rs`)

- Compile `macro id : cons Ast.expr -> Ast.expr; id args = case args of Cons e _ -> e; Nil -> fail "no args"` and run it on the reified `Cons (Ast.int 1) Nil`; the output `Term` equals the reified `Ast.int 1`.
- A macro that `fail "boom"`s returns `Failed { message: Some("boom") }`.
- Budget exhaustion returns `Failed` with the machine's budget error.

**Done when** tests pass through current Build specialization and closed Core
program assembly, including invocation through the real driver expansion path.

---

## Chunk 8: expansion loop in the driver

**Files**

- `crates/nash-driver/src/compile.rs`
- `crates/nash-driver/src/expand.rs` (new)
- `crates/nash-driver/src/splice.rs` (new)
- `crates/nash-config/src/config.rs` (`macroExpansionLimit`, `macroBudget`)

**Change**

`compile_module` (`crates/nash-driver/src/compile.rs:145`) becomes the
loop from docs/macros.md. Macro programs of already-compiled modules are
kept in a build-wide map next to `interfaces`.

**Code**

```rust
// crates/nash-driver/src/compile.rs
struct BuildState<'s> {
    store: &'s Bump,
    interfaces: BTreeMap<&'s str, Interface<'s>>,
    /// module name -> macro name -> flat-encoded program
    macros: BTreeMap<&'s str, BTreeMap<&'s str, Vec<u8>>>,
    limits: ExpansionLimits,
}

#[derive(Clone, Copy)]
pub struct ExpansionLimits {
    pub rounds: u32,
    pub budget: ExBudget,
}

fn compile_module<'s>(uri: &Url, source: &str, state: &BuildState<'s>) -> Result<Compiled<'s>, Vec<Diagnostic>> {
    let bump = Bump::new();
    let src: &str = bump.alloc_str(source);
    let mut parser = nash_parse::Parser::new(&bump, src.as_bytes());
    let mut module = parser.module().map_err(|e| vec![syntax(e)])?;

    let (can, node_types, annotations) = expand::expand(&bump, &mut module, state)?;   // strict result
    let interface = nash_can::from_module(&bump, &can.module, &annotations);
    let stored = nash_can::deep_copy_interface(state.store, &interface);
    let macros = codegen_macros(&bump, &can.module);                   // chunk 7
    Ok(Compiled { interface: stored, macros, decl_count: count_decls(can.module.decls) })
}
```

```rust
// crates/nash-driver/src/expand.rs
pub fn expand<'a, 's>(
    bump: &'a Bump,
    module: &mut SourceModule<'a>,
    state: &BuildState<'s>,
) -> Result<(CanResult<'a>, SolvedTypes<'a>, Annotations<'a>), Vec<Diagnostic>> {
    // `solve` adapts the current direct-inference API to the new mode.
    // Reuse SolvedTypes; do not recreate the old constraint-tree solver API.
    for round in 0..=state.limits.rounds {
        let can = canonicalize(bump, Mode::Lenient, module, state)?;
        let (annotations, node_types) = solve(bump, &can.module, Mode::Lenient)?;

        if can.macro_uses.is_empty() {
            let can = canonicalize(bump, Mode::Strict, module, state)?;
            let (annotations, node_types) = solve(bump, &can.module, Mode::Strict)?;
            check_main_parameters(&can, &annotations)?; // existing driver check
            check_patterns(bump, &can.module)?;         // nash_nitpick::check
            return Ok((can, node_types, annotations));
        }
        if round == state.limits.rounds {
            return Err(vec![Diagnostic::macro_expansion_limit(&can.macro_uses)]);
        }

        let arena = Arena::new();
        let reifier = Reifier::new(&arena, &node_types, bump, &can.tables.kinds);
        let mut outputs: Vec<Output<'a>> = Vec::with_capacity(can.macro_uses.len());
        for (use_index, use_) in can.macro_uses.iter().enumerate() {
            let gensym = Gensym { round, use_index: use_index as u32 };
            let program = state.macro_program(use_.reference())?;
            let output = match use_ {
                MacroUse::Decl { target, attribute } => {
                    let (decl, args) = reifier.decl_use(&can.module.snapshot(*target), attribute);
                    let run = run_decl_macro(&arena, program, decl, args, state.limits.budget)
                        .map_err(|e| Diagnostic::macro_failed(attribute.region, use_.reference(), e))?;
                    let decls = Unreifier::new(bump, attribute.region).decls(run.output)
                        .map_err(|e| Diagnostic::macro_bad_output(attribute.region, use_.reference(), e))?;
                    Output::Decls { target: *target, decls: gensym.rename(bump, decls) }
                }
                MacroUse::Expr { region, arguments, .. } => {
                    let args = reifier.expr_use(arguments);
                    let run = run_expr_macro(&arena, program, args, state.limits.budget)
                        .map_err(|e| Diagnostic::macro_failed(*region, use_.reference(), e))?;
                    let expr = Unreifier::new(bump, *region).expr(run.output)
                        .map_err(|e| Diagnostic::macro_bad_output(*region, use_.reference(), e))?;
                    Output::Expr { region: *region, expr: gensym.rename_expr(bump, expr) }
                }
            };
            outputs.push(output);
        }
        *module = splice::splice(bump, module, outputs);
    }
    unreachable!("loop returns or errors")
}
```

`splice::splice` rebuilds the `SourceModule`: for `Output::Decls` it
replaces the target declaration with the decoded list (attributes already
consumed are dropped; remaining attributes on the original are kept on
its replacement if the original is the first output element, which is
how `derive` chains); for `Output::Expr` it rebuilds the enclosing value
body, replacing the `MacroCall` node whose region matches. Regions are
unique per invocation because the parser assigned them from source; nodes
spliced in earlier rounds all carry the site region but are never
`MacroCall`s themselves unless the macro emitted one, and then the site
region is reused, which is fine because the walk replaces the first match
in pre-order and each round re-collects uses.

`state.macro_program(reference)` finds the flat bytes for
`reference.home.name` / `reference.name`; a miss is an internal error
(the interface said it was a macro, so the module compiled).

Config (`crates/nash-config/src/config.rs`, `Application` and `Package`):

```rust
/// Maximum macro expansion rounds per module (default 32).
#[serde(default = "default_macro_expansion_limit")]
pub macro_expansion_limit: u32,
/// CEK budget per macro run, `{ "cpu": N, "mem": M }` (default 10x mainnet tx budget).
#[serde(default)]
pub macro_budget: Option<Budget>,
```

**Elm/Aiken reference**

`elm/compiler/src/Compile.hs` `compile` (the per-module pipeline this
loop wraps). Aiken has no macros; its `aiken-project/src/lib.rs`
`compile` shows keeping per-module build artifacts across modules.

**Tests** (`crates/nash-driver/src/compile.rs` tests, in-memory sources)

- `test_expression_macro_expands`: module `M` with `macro twice : cons Ast.expr -> Ast.expr; twice args = case args of Cons a _ -> quote (~a + ~a); Nil -> fail "twice: one argument"`; module `Main` with `main = twice!(1)`; build succeeds and `Main`'s interface says `main : int`.
- `test_decl_macro_appends`: a decl macro that returns `[decl, decl2]`; `Main` exports both.
- `test_same_module_use_rejected`.
- `test_expansion_limit`: a macro that emits itself; error names the site.
- `test_macro_failure_message`: `fail "no"` → diagnostic text contains `no`.
- `test_lenient_then_strict`: `@derive(Eq)` on `Foo` and `a == b` on `Foo` in the same module builds.

**Done when** the driver tests pass and `nash check` on a project with no
macros behaves exactly as before.

---

## Chunk 9: integrate existing comptime with expansion

Closed comptime already works. `can_to_core::Engine::expr` compiles
`Expr::Comptime`, closes over reachable dependencies, calls
`comptime::eval_closed`, and emits `Core::Lit`. Existing tests cover arithmetic,
closed local dependencies, runtime capture rejection, and nonconstant results.

- [ ] Preserve that implementation and its closure/constant rules.
- [ ] Test macro output containing comptime: expand first, then use normal
  comptime evaluation during codegen.
- [ ] Test comptime within macro code under the same closure rules. Macro
  parameters are not automatically closed constants for nested comptime merely
  because the macro itself executes at compile time.
- [ ] Preserve source locations and useful errors through expansion.

Do not add a Core comptime node, another evaluator, or earlier rejection of
closed local dependencies to implement this chunk.

---

## Chunk 10: `@derive` in `nash/base`

**Files**

- `crates/nash-driver/base/src/Derive.nash` (new)
- `crates/nash-driver/base/src/Ast.nash` (builders section)
- `crates/nash-driver/src/compile.rs` tests

**Change**

Write `derive` and the four derivations in Nash per docs/macros.md. `Eq`
is in the doc; `Ord` compares constructor index then fields; `Show`
renders `Ctor field1 field2` with parentheses for nested; `Validate` requires
`representation == Some Big` and generates recursive source `validate` checks.
`ToData` and `FromData` need no derivation: their ordinary blanket Big impls
cover every Big type, and generated concrete impls would overlap them.

**Code** (`crates/nash-driver/base/src/Derive.nash`, excerpt beyond the doc's `deriveEq`)

```elm
deriveOrd : decl -> decl
deriveOrd decl =
    case decl of
        Union { name, params, ctors } ->
            let
                self = selfType name params
                a = Ast.name "a"
                b = Ast.name "b"

                -- (Ctor_i xs, Ctor_i ys) -> lexicographic; (Ctor_i _, Ctor_j _) -> compare i j
                sameCtor i ctor =
                    let
                        xs = binders "x" ctor
                        ys = binders "y" ctor
                        pairs = Cons.map2 (\x y -> quote (compare ~(Ast.var x) ~(Ast.var y))) xs ys
                    in
                    Ast.arm
                        (Ast.ptuple (Cons (ctorPat ctor xs) (Cons.singleton (ctorPat ctor ys))))
                        (Cons.foldr (\c acc -> quote (Ordering.then_ ~c ~acc)) (quote EQ) pairs)

                differentCtor =
                    Ast.arm Ast.wildcard
                        (quote (compare (~(indexOf a)) (~(indexOf b))))

                indexOf v =
                    Ast.case_ (Ast.var v)
                        (Cons.indexedMap (\i ctor -> Ast.arm (ctorPat ctor (wild ctor)) (Ast.int i)) ctors)
            in
            Ast.impl (Raw "Ord") (Cons.singleton self) (context "Ord" params)
                (Cons.singleton
                    (Ast.def (Raw "compare") (Cons (Ast.pvar a) (Cons.singleton (Ast.pvar b)))
                        (Ast.case_ (Ast.tuple (Cons (Ast.var a) (Cons.singleton (Ast.var b))))
                            (Cons.append (Cons.indexedMap sameCtor ctors) (Cons.singleton differentCtor)))))

        _ ->
            fail "derive(Ord): only `type` declarations can derive Ord"

-- Validate derivation matches declared constructor shapes, checks exact
-- arity, recursively validates every field, then invokes the constructor.
-- Record aliases use List in declaration order.
-- This requires Validate contexts for the relevant type parameters.
-- Validation must not delegate to unchecked fromData. ToData and FromData
-- already have blanket impls and must not receive generated concrete impls.
```

**Elm/Aiken reference**

Aiken derives nothing; `Eq` via `==` is builtin on all types. Haskell's
`deriving` semantics (GHC `Data.Deriving`) are the reference for the
`Ord` ordering rule (constructor order, then fields).

**Tests**

- `crates/nash-driver/base/src/Derive.nash` `tests` block: cannot invoke `derive` in its own module, so the tests live in `core/tests/DeriveTests.nash` (a test-only module) with `@derive(Eq, Ord, Show)` on a three-constructor little type and a two-field Big record, and `prop`s for reflexivity, antisymmetry, `show` round trip via a hand-written parser stub.
- Driver expansion snapshot (chunk 12): `@derive(Eq)` on `type Foo = A int | B` prints the generated `impl`.

**Done when** `nash test core/` passes and the snapshot matches
docs/macros.md's description.

---

## Chunk 11: diagnostics

**Files**

- `crates/nash-report/src/macro_.rs` (new)
- `crates/nash-report/src/lib.rs`
- `crates/nash-can/src/error.rs`, `crates/nash-parse/src/error.rs` (variants from chunks 1, 2, 9)

**Change**

Render every macro/comptime error listed in docs/macros.md as a miette
`Diagnostic` with Elm-style prose. `MacroFailed` shows the macro name,
the site, and the message. `MacroBadOutput` shows the unreifier path and
the offending term (a lambda, a wrong tag, a wrong field count).
Strict-pass errors inside generated code include the generated
declaration pretty-printed by the chunk 12 printer (replaced by
`nash-fmt` when plans/13 lands).

**Code**

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum MacroDiagnostic {
    #[error("MACRO FAILED")]
    #[diagnostic(code(nash::macro_::failed))]
    Failed {
        #[source_code]
        src: NamedSource<String>,
        #[label("while expanding this")]
        site: SourceSpan,
        macro_name: String,
        #[help]
        message: String,
    },
    #[error("MACRO OUTPUT MALFORMED")]
    BadOutput { /* site, macro_name, path, found */ },
    #[error("MACRO EXPANSION LIMIT")]
    ExpansionLimit { /* sites: Vec<SourceSpan>, limit */ },
    #[error("UNBOUND HYGIENIC NAME")]
    UnboundLocal { /* site, name (without mark), macro_name */ },
    #[error("COMPTIME NOT CLOSED")]
    ComptimeNotClosed { /* region, name */ },
    #[error("COMPTIME NOT A CONSTANT")]
    ComptimeNotConstant { /* region, typ, representation */ },
    #[error("COMPTIME FAILED")]
    ComptimeFailed { /* region, message, machine */ },
}
```

`nash-report` keeps a `Vec<GeneratedDecl { site: Region, macro_name, text: String }>`
per module so that a `NotFoundVar`/type error whose region equals a site
region is rendered with the generated text appended.

**Elm/Aiken reference**

`elm/compiler/src/Reporting/Error/Canonicalize.hs` `toReport` for prose
style; `Reporting/Doc.hs` for the layout helpers `nash-report` already
ports.

**Tests** (`crates/nash-report/src/snapshots`)

One rendered snapshot per variant, using the in-memory driver from chunk 8.

**Done when** every error in docs/macros.md's table has a snapshot.

---

## Chunk 12: expansion snapshot tests and the debug printer

**Files**

- `crates/nash-source/src/print.rs` (new)
- `crates/nash-macro/tests/expand.rs` (new)
- `crates/nash-macro/tests/snapshots/`

**Change**

A deterministic expanded-AST debug renderer and in-memory expansion snapshots.
`nash-fmt` already formats source; reuse its rendering facilities where suitable
and extend its macro syntax support. Debug output shows resolved names, hygiene
identities, and internal-only IntegerDispatch nodes; it is not necessarily
reparsable Nash. Do not create another general formatter or wait for Plan 13.

**Code**

```rust
// crates/nash-source/src/print.rs
pub fn module(m: &Module<'_>) -> String
pub fn expr(e: &Located<Expr<'_>>) -> String
pub fn pattern(p: &Located<Pattern<'_>>) -> String
pub fn typ(t: &Located<Type<'_>>) -> String
```

Rules: one declaration per blank-line-separated block, four-space
indentation, `let`/`case`/`if` always multi-line, calls single-line,
`VarGlobal` printed as `{package}:{module}.{name}` in braces so snapshots
show resolution, `x·1_0` printed as-is.

```rust
// crates/nash-macro/tests/expand.rs
macro_rules! assert_expansion_snapshot {
    ($name:literal, { $($module:literal => $source:literal),+ $(,)? }) => {{
        let expanded = support::expand_project(&[$(($module, indoc::indoc!($source))),+], $name);
        insta::assert_snapshot!(expanded);
    }};
}
```

`support::expand_project` runs chunk 8's loop with `Mode` hooks and
returns `print::module` of the named module after the final round.

**Tests**

- `expression_macro_quote`: `twice!(x)` → `Num.add x x` printed with global braces.
- `expression_macro_tail_literal`: `tail!(3, xs)` → three nested calls to
  the globally resolved `Builtin.tailList`; zero returns `xs`, a negative
  literal fails, and a variable count fails because macro arguments are AST
  rather than evaluated values. The test uses the partial builtin because
  safe `List.tail` returns an `option` that cannot feed the next call.
- `expression_macro_predicate_all_literal`:
  `predicateAll!((> 5), [10, 12, 15])` → the conjunction of three
  applications of the reified section lambda. The empty list becomes
  `True`; a variable list fails because it cannot be unrolled at expansion
  time.
- `decl_macro_derive_eq`: `@derive(Eq) type Foo = A int | B` → original plus `impl Eq Foo`.
- `hygiene_capture_avoided`: the `let x = 1 in ~body` case from chunk 6.
- `nested_macro_two_rounds`: a macro whose output calls another macro.
- `attribute_chain`: two attributes on one declaration, second sees the first's output.
- `comptime_in_macro_output`: output contains `comptime (1 + 2)`; expanded module prints it unchanged (comptime is evaluated in codegen, not here).

**Done when** snapshots are accepted and `cargo insta test --unreferenced delete` is clean.

---

## Chunk 13: explicit integer dispatch AST and positional library macro

**Decision (26 September 2026): pending implementation.** Integer dispatch is an
explicit AST operation. The user chooses it through an ordinary library macro;
there is no automatic dispatch selection for literal patterns.

Illustrative positional invocation (capability required; public library API not
fixed by this example):

```nash
dispatch!(n, [branch0, branch1, branch2])
```

The macro receives two expression ASTs, requires its second argument to be a
`ListLit`, and returns `Ast.integerDispatch subject branches`. The list brackets
are syntax consumed at expansion time, never an emitted runtime list. The macro
is an ordinary exported expression macro (`cons Ast.expr -> Ast.expr`), imported
from a library module; the compiler does not recognize the name `dispatch`.
Wrong argument count and a nonliteral branch list are library macro errors.
Other macros may construct the same AST node directly.

**AST and typing contract**

- Add `IntegerDispatch expr (cons expr)` to `Ast.exprNode` and builder
  `integerDispatch : expr -> cons expr -> expr`. Append its reification tag;
  update `Ast.nash` and the host tag table together.
- Add matching subject/branch expression nodes to source and canonical ASTs.
  The source node is a macro-output form; no new keyword or direct parser syntax
  is required. Reification/unreification validates its node shape and traverses
  both children. Hygiene and source metadata preserve caller bindings/locations.
- Require native `int` for the subject and one common result type for branches.
  No Eq, FromInt, Big conversion, or Storable constraint belongs to dispatch
  itself. Ordinary branch expressions still generate their own constraints.
- Pre-expansion typing of the list-shaped macro argument must not leak a
  runtime-list Storable requirement into the expanded program. Verify the
  existing lenient-predicate/recheck path with Term- and function-valued branches;
  do not add a special case keyed to this macro's name.
- An empty branch sequence is allowed: it has a fresh result type and always
  fails at runtime after subject evaluation, matching raw UPLC case semantics.

**Core and lowering contract**

- Preserve an explicit integer-dispatch operation through Core and its walkers,
  substitutions, free-name analysis, pretty printing, and optimizer handling.
  Do not reuse `CaseKind::Int`, whose current contract is equality-chain lowering.
- Emit exactly a native `Term::Case` on the subject with the positional branch
  terms. Evaluate the subject once and execute only the selected branch.
- Negative and out-of-range subjects fail according to UPLC. No generated bounds
  or shape guard, fallback, decoding, filling, shifting, density test, maximum
  table policy, cost model, or automatic equality fallback.
- Big Int callers explicitly write `Builtin.unIData x` when they want decoding.
  Wrong-shape decoding fails normally. Arbitrary sparse Big constructor tags
  from Plan 14 are a separate feature, not implicit dispatch inputs.
- Do not add runtime list construction, thunk wrappers, or lambda/application
  pairs merely to implement dispatch. A user-written branch may itself be a
  function or contain any ordinary expression.
- General optimizations may preserve/simplify this operation under their usual
  semantic rules; they must not treat all branch bodies as eagerly evaluated.
  This chunk does not require the deferred Plan 08 optimizer.

**Implementation and acceptance checklist**

- [ ] Extend ASTs, `Ast.nash`, builders, host tag table, reifier/unreifier, hygiene,
  typing, Core traversal/printing, and direct lowering as one coherent feature.
- [ ] Demonstrate the positional interface with an ordinary test macro and
  document its import. Chunk 14 separately supplies generic case-shaped input;
  the public dispatch macro and its accepted arms are later library decisions.
- [ ] Add expansion snapshots proving the list becomes an IntegerDispatch node,
  including source locations and hygienic references from caller scope.
- [ ] Test first/last branch selection; subject evaluation exactly once; trace
  order; selected failure; unselected trace/failure; negative, out-of-range, and
  huge integers; empty branches; explicit Big decoding and wrong-shape failure.
- [ ] Test native, Big, Term, and function result types; mismatched branch types;
  non-int subject; wrong macro arity; runtime branch-list variable rejection;
  generated dispatch from a differently named macro; nested macro expansion.
- [ ] Snapshot actual UPLC: one direct positional case, no list construction or
  added guards/wrappers. Keep ordinary literal case equality snapshots unchanged.
- [ ] Run supported-target evaluator tests, formatting, strict Clippy, workspace
  tests, and update affected crate changesets when implementation lands.

This chunk depends on chunks 2–8 for the macro path and coordinates with Plan 12's
Ast module. Include it in chunk 12's expansion/debug coverage; it can land without
waiting for deriving. The older AST/code sketches above must carry the new node
through every exhaustive match when implemented.

---

## Chunk 14: reusable structured macro invocation forms

**User requirement, accepted; implementation pending.** Support familiar Nash
syntax shapes as structured macro inputs. This chunk provides the capability;
it does not define standard-library macros or their behavior. Names such as
`expect`, `dispatch`, and `decodeIf` below are illustrative, not reserved.

| Form | Example | Structured input |
|---|---|---|
| Call-shaped | `name!(a, b)` | Ordered expression AST arguments |
| Binding-shaped | `name! pattern = value` | Pattern AST, subject expression AST, and the remaining lexical scope/block AST |
| Case-shaped | `name! subject of` followed by arms | Subject expression AST and ordered pattern/body arms |

Examples of the required syntax capability:

```nash
someMacro!(argument)

-- Inside a binding/block context:
expect! Some x = value

-- Expression with a normal case-arm block:
dispatch! value of
    0 -> first
    1 -> second

decodeIf! valueData of
    Box _ -> body
    Something a b -> otherBody
```

These examples establish parsing and AST access only. They do not specify what
is dispatched, decoded, validated, returned, or done on failure. Different macros
using the same shape may construct different AST. Do not encode their names or
semantics in the parser, solver, or lowerer.

### Parsing and AST transport

- Reuse the existing expression-argument, pattern/binding, and case-arm parsers
  and indentation/delimiter rules. Add generic invocation nodes, not a dedicated
  parser per library macro. Preserve source locations and qualification rules.
- The binding-shaped form belongs in contexts with a remaining lexical body,
  such as let bindings and do blocks. Specify each supported context and its
  scope boundary in the grammar; do not consume arbitrary following module
  declarations. Multiple binding macros must nest in source order.
- The case-shaped form is an expression with normal ordered pattern/body arms.
  It is not a block of bare positional expressions; the call-shaped interface
  can still carry an explicit list AST when a macro wants positional input.
- Macro shape metadata, signature validation, imports, reification/unreification,
  expansion, and diagnostics must distinguish all supported input shapes.
  Extend the earlier two-shape sketches (expression/declaration); they are not a
  restriction preventing binding/case-shaped inputs. Settle the exact Nash AST
  payload/signature encoding during implementation and update both sides together.
- Input is structured AST, never arbitrary token streams. This does not add
  user-defined parser rules, an open keyword-replacement mechanism, or a macro's
  ability to inspect neighboring declarations.
- Macro output remains Nash AST. Builders can construct cases, functions,
  declarations, and the separately accepted IntegerDispatch node. Additional
  quote/splice shorthand is not a prerequisite.

### Scope and checking

- A binding macro must be able to place its remaining body under the supplied
  pattern bindings in its output. Preserve references to those bindings while
  retaining caller free references. The macro need not receive or generate an
  evaluated runtime lambda to transport the body.
- Case arms preserve independent pattern/body scopes and source order. Repeated
  names in different arms must not capture one another or generated temporaries.
- Do not turn a case-shaped invocation into an ordinary runtime case *before*
  expansion. In `decodeIf! valueData of Box _ -> ...`, the input subject and
  pattern need not already satisfy ordinary case subject-type compatibility:
  generating the conversion/checks is precisely a possible macro responsibility.
- Define provisional inference for each structured input shape: retain useful
  type information where available, check names/bindings and independent bodies,
  but defer relationships introduced by the eventual expansion. Represent
  unresolved type metadata honestly. Do not disable ordinary checking everywhere
  or introduce exceptions keyed to `decodeIf`/`expect`/`dispatch`.
- Coverage, redundancy, and binding-pattern irrefutability checks apply to the
  final expanded program, not to syntax awaiting macro interpretation. Final
  strict inference and coverage checking remain mandatory; malformed output is
  not accepted merely because it came from a macro.
- Syntax transport itself does not evaluate subjects, arms, or remaining bodies.
  Runtime evaluation order and laziness are determined by the resulting AST.
  Tests should demonstrate that a library expansion can evaluate a subject once
  and leave unselected bodies unevaluated, without intrinsic compiler behavior
  for any example macro name.

### Capability acceptance tests

- [ ] Parse and format each generic form, including qualified names, multiline
  subjects, nested invocations, indentation boundaries, malformed arms/bindings,
  and combinations of ordinary and macro bindings.
- [ ] Reify/unreify subject, patterns, ordered arms, and remaining bodies with
  source metadata and correct scopes; test imported macros and shape errors.
- [ ] Use small test-only macros to demonstrate binding propagation, case-arm
  inspection, and expression argument inspection. Public macro APIs are not a
  prerequisite for completing this capability.
- [ ] Snapshot source → macro input AST → expanded AST → Core → UPLC. Test an
  expansion with a converted subject so raw input is not incorrectly checked as
  an ordinary case, plus final type/coverage errors in invalid output.
- [ ] Test two nested binding macros, caller/generated-name collisions, repeated
  arm variable names, nested shadowing, outer captures, returned functions, and
  subject-once/selected-branch effects under representative expansions.
- [ ] Show that macros can build a normal multi-clause function as a lambda/case,
  including multi-argument patterns and normal recursion/partial application.
  Do not introduce a multi-definition library syntax or public macro today.
- [ ] Update docs and all earlier macro-shape/checking sketches during
  implementation; use current solver/formatter/driver APIs.

### Deliberately not decided here

This capability chunk specifies no public `expect`, `decodeIf`, or `clauses` API.
Testing/assertion migration is staged separately in chunk 15.
Failure/fallback rules, validation depth (including ignored fields), target-type
selection, accepted dispatch arms, and library treatment of irrefutable patterns
remain library design decisions. Do not replace current power-assert reporting,
insert automatic guards/validation, or choose integer-dispatch heuristics as part
of this capability. The compiler supplies syntax/AST facilities and final checking.

Depends on chunks 1–8 and 11–12. Coordinate shape support across those chunks;
it does not require Plan 08 or implementation of particular standard macros.

---

## Chunk 15: move property and assertion expansion into Nash macros

**Accepted direction, implementation pending.** Replace testing-specific syntax
expansion and Core construction with macros that emit ordinary typed Nash. This
chunk extends the earlier plan; completing generic macros alone does not complete
the migration. It does not yet choose the public replacement spelling for `prop`,
`via`, or `assert`.

### First property slice

- [ ] Specify how a generated private test root is discovered without exposing it
  as an ordinary public definition. Retain name, expected failure, budgets, source
  origin, selected-test behavior, and counterexample display metadata. Choose a
  thin compiler/driver discovery contract or library descriptor representation;
  expression output alone does not register a test.
- [ ] Define the property block's syntax and lexical boundaries using generic
  macro input forms. Existing `via` is not automatically valid macro input. Do
  not add parser/solver/codegen rules keyed to a particular library macro name.
- [ ] Define interleaving: bindings that depend on earlier draws, ordinary lets,
  conditional draws, statements/assertions before later draws, and draws inside
  called functions. Preserve the chosen evaluation order; do not blindly hoist
  generation across branches or effects. Explicitly delimit any deferred forms.
- [ ] Implement a real library macro producing generator composition plus
  property/display callbacks. Begin with two dependent draws and a computed
  label, then test execution order and failures according to the settled contract.
- [ ] Reuse `Test.prepare`, `Test.both` where suitable, and `Prop` composition.
  Preserve the preparation/body/display protocol unless a documented semantic
  requirement necessitates a coordinated change. Binding macros do not by
  themselves settle the property block's preparation boundary.
- [ ] Execute through `nash test`, including selection, expected failure, budget,
  labels, counterexample display, and deterministic replay/shrinking regressions.
- [ ] Remove replaced generator/callback Core construction in
  `nash-codegen/src/tests.rs` and obsolete frontend special cases as consumers
  migrate. Keep only necessary root compilation/discovery. Do not declare this
  done while both production expansion implementations remain.

### Power-assert migration

- [ ] Define how a macro obtains operand syntax/origins and decides whether `Show`
  evidence exists. Typed AST alone does not provide today's optional trait probe.
  Choose a generic capability or an explicit library policy, not an assertion-name
  exception in codegen. Preserve caller operand spans separately from generated
  invocation spans.
- [ ] Preserve evaluate-once behavior, short circuiting, trace order, failure-only
  display, and source-linked capture metadata. Test values with and without Show.
- [ ] Generate ordinary calls to assertion/reporting helpers and delete replaced
  capture instrumentation in `nash-codegen/src/assertion.rs` after parity checks.
  Any intentional output change must be separately specified and snapshot-reviewed.

### Responsibilities that remain

The runner still discovers/selects roots through the chosen contract, evaluates
programs, enforces budgets/expected outcomes, and reports results. `Prop` and the
Rust runner retain choice recording, replay, shrinking, and seed management.
Macros transform syntax; they do not perform random sampling at compile time.
The ordinary backend still compiles the emitted functions and the macro programs.

Update `docs/testing.md`, Plan 10's migration notes, examples and fixtures when the
new public contract is settled. Plan 10's completed current implementation stays
recorded as completed; this new migration is tracked here.

## Order and dependencies

Follow milestones A → B → C → D above, using the numbered chunks as component
checklists. Cross-layer slices must compile and execute real macros early.

- A combines declaration/call syntax, typed input, minimal transport, hygiene,
  imported macro compilation, driver replacement, diagnostics, and snapshots.
- B completes transport and provisional checking, adds structured binding/case
  forms, and establishes declaration expansion. Coordinate chunk 14 across all
  earlier shape definitions rather than bolting it on after a two-shape design.
- C uses those capabilities for chunk 15. Discovery and assertion capabilities
  are explicit gates, not assumed consequences of expression macros.
- D completes the remaining accepted scope. Chunk 13's explicit dispatch needs
  the generic macro path and AST/Core support; it does not depend on deriving.
- Chunk 9 preserves existing comptime throughout. Ast encoding and Plan 12's Ast/
  Derive work must stay synchronized. Macro-free modules retain the ordinary
  single strict checking path; do not impose repeated provisional passes on them.
