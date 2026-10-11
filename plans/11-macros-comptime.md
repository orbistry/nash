# Plan 11: macros and comptime

Goal: build the macro machinery. A macro is an ordinary Nash function that the
compiler runs on the CEK machine during compilation. It receives syntax and
returns syntax. Macro authors write the returned syntax with `quote` and `~`.
The compiler then checks the expanded module in the ordinary strict way.

This plan replaces the earlier versions of Plan 11 (the typed-input design and its
code sketches, and the 7 October rewrite that moved `do` and properties into
macros). Both stay in jj history as reference.

## Scope

In scope:

- `macro` declarations, macro interfaces, and macro imports.
- `quote` and `~` (unquote) for every syntax kind that a supported shape needs.
- Four invocation shapes: call, case, binding, declaration.
- The explicit integer dispatch operation and its client, the positional
  `dispatch!(n, [branch0, branch1, ...])` macro in Base (chunk 5).
- Syntax transport, CEK execution, output decoding, hygiene, the expansion loop,
  limits, diagnostics, and tooling.
- `@derive` in Base, written in Nash (the decided derive mechanism).

Out of scope. These stay compiler-defined special forms or features:

- `do`, `trace`, `assert`, `fail`, `todo`, and `comptime`.
- `tests`, `test`, `prop`, `via`, generators, and the test runner.
- A block shape (`name! do`), arbitrary token streams, user grammar rules.
- Inferred-type queries from macros, and a lenient solver mode.

Add a macro feature only when a real use case needs it.

## Guide macros

Each shape lands together with a guide macro. A guide macro proves that the
machinery is right. Its exact library behavior is not the goal of this plan,
except `dispatch!` (chunk 5) and `@derive` (chunk 10).

| Shape | Source form | Guide |
|---|---|---|
| Call | `name!(a, b)` | `unfold10!(...)`; `dispatch!(n, [branch0, branch1])` |
| Case | `name! subject of` followed by `pattern -> body` arms | `caseIf! value of ...`: a pattern match on opaque Data |
| Binding | `name! pattern = value` in a `let` or `do` block | `expect! Some x = value`: on a mismatch it traces and fails |
| Declaration | `@name(args)` on the line before a declaration | `@derive(Eq, Show)` |

The compiler recognizes the shapes, not the library names. A shape fixes only
the input. Each macro chooses its output: an ordinary `case`, integer dispatch, or
any other syntax. The compiler does not choose for it.

## Design: source syntax in, source syntax out

Settled: macros are syntax macros, as in Racket. The input has no types. A macro
definition itself is an ordinary annotated Nash function that its own module
checks.

```text
parse
  -> resolve macro imports and names; track syntax scopes
  -> expand outer invocations; repeat on the emitted syntax
  -> ordinary canonicalization, kinds, inference, evidence
  -> nitpick
  -> Core, specialization, UPLC
```

A macro receives the parsed source syntax of its input. It does not receive
inferred types, canonical nodes, or the bodies of other declarations.

Cost of this design: names must be resolved by scope before canonicalization.
The expander keeps binding identities, resolves macro names, and allows
references to declarations that a macro generates later in the same module.

The earlier Plan 11 gave macros typed canonical input. That design is dropped: it
needs a lenient canonicalize and solve pass, a representation for unresolved names
and deferred predicates, and a rebuild of canonical nodes and `SolvedTypes` after
each round.

`quote` needs the definition-site global bridge (chunk 3).

## Settled contracts

### Quote and unquote

Macros use `quote` and `~` as needed. `quote (e)` builds syntax; `~x` and `~(e)`
insert syntax values. Quote lowers to calls of the Base `Ast` builders. The
builders are ordinary Nash functions, so a macro can use them inside a quote, for
example `~(Ast.integerDispatch n branches)`. Quote and `~` land before any macro
runs (chunk 3), and the first macro that runs (chunk 4) uses them.

Quote covers expressions first (chunk 3), then patterns (chunk 6) and declarations
and impls (chunk 8). Types are added when a shape needs them.

### Binding identity

Copied or generated binders get fresh identities, and their references change
with them. Free references in caller syntax keep their caller binding. Equal
spellings never mean equal identities. A supplied pattern and its body are
freshened together. A global string rename is not sufficient.

### Source origins

Copied or moved caller syntax keeps its original locations, operands included.
Freshening does not change locations. The expander records the invocation chain
separately. New syntax with no caller location points to the invocation.

### Globals and explicit inputs

Globals in quoted syntax resolve in the macro's defining module, also private
helpers that the caller cannot see. Caller syntax keeps the caller's resolution.
Caller dependencies are explicit macro inputs. A macro cannot look up a caller
name by a generated string. Macro output cannot make a reference to a global
that the macro's own checked code did not name.

### Expansion order

Outer invocations expand first. Only invocations in the emitted syntax expand
next; children that a macro drops do not run. Declaration attributes run top to
bottom, before expression expansion in the same round. Parse errors stay errors,
also in input that a macro drops.

### Checking

The expanded module goes through ordinary strict canonicalization, solving, and
nitpick once. Generated type, name, and trait errors are ordinary diagnostics with
origins. A module with no macros keeps its current path.

### Execution

Macros compile through the current Build and closed program assembly, and run on
the existing CEK machine. The result must be constructor and scalar values only;
closures, delays, builtins, and other constants are output errors. Limits:
expansion depth (default 32) and an execution budget, separate from comptime.
Programs are cached for one build, keyed by the defining module and its
dependencies. There is no cache across builds.

### Imports

A macro is used only through an import; a call in its own module is an error.
Macros have their own namespace next to values. Imports in a `tests` block can
supply macros to tests.

### Comptime

`comptime` keeps its current rules: closed dependencies, no runtime captures,
a constant result. Macro output can contain `comptime`, and macro definitions
can use it.

## Conventions

`'a` is the module arena lifetime. The `Ast` types are little ADTs with
representation `Term`. A constructor is `constr i [fields]`, `i` the declaration
index, fields in order. `string`, `int`, and `bytes` fields are UPLC constants.
`option` is `Some` tag 0 and `None` tag 1. `cons` is `Nil` tag 0 and `Cons` tag 1.
Nothing in the macro path is `Data`. The host tag table and `Ast.nash` change
together.

## Chunk 1: declarations, syntax, and resolution

- [ ] Parse and format `macro name : shape` declarations, `quote (e)`, `~x`, and
  `~(e)`. Keep the existing `name!(..)` and `@name(..)` parsing.
- [ ] Define the macro shape signatures and check them. Export macro identity and
  shape through interfaces.
- [ ] Resolve imported macro names, also qualified names and `tests` imports.
  Reject same-module calls, wrong shapes, and calls of ordinary values.

Evidence: parser, formatter, and resolution snapshots. No macro runs.

## Chunk 2: `Ast` in Base and syntax transport

- [ ] Inventory the source expressions, patterns, types, and declarations,
  including `assert`, `fail`, `todo`, `trace`, `do`, and nested macro calls.
- [ ] Define the `Ast` little ADTs in Base and the matching host tag table, with
  origins and binding identities.
- [ ] Encode source syntax to `Term::Constr` values, and decode values to source
  syntax. Decoding checks tags, arities, field types, binding references, and
  origins, and reports errors without panics.

Evidence: round-trip and malformed-value snapshots in Rust. No macro runs.

## Chunk 3: quote lowering and macro compilation

- [ ] Lower expression `quote` and `~` to checked `Ast` builder calls. Reject `~`
  outside a quote and nested quotes that are not supported.
- [ ] Quoted globals resolve in the defining module, private helpers included.
  Carry their identity to the output so that final checking resolves them there.
- [ ] Compile exported macros through Build to programs, cached for the build.

Evidence: Core and UPLC snapshots of compiled macros written with quote; quote
error snapshots. No macro runs.

## Chunk 4: first execution, call shape, hygiene

Guide: `unfold10!`, written with quote.

- [ ] Run a compiled macro on the CEK machine with its budget, and decode its
  result.
- [ ] Expand call invocations before strict checking, outer first, with the depth
  limit and the invocation chain.
- [ ] Apply the binding identity contract: fresh binders, caller references kept,
  separate scopes for equal names.
- [ ] Report invocation, shape, output, budget, depth, and macro failure errors
  with the macro name and origins.
- [ ] Keep `comptime` behavior.

Evidence: expansion, Core/UPLC, and run snapshots of `unfold10!`; fixtures for
capture, shadowing, private helpers, nested and dropped calls, and limits.

This chunk can split into more diffs at review.

## Chunk 5: integer dispatch

Client: `dispatch!(n, [branch0, branch1, ...])`, exported by Base `Dispatch`.

- [ ] Add integer dispatch: a subject and an ordered list of branches, in the
  `Ast` schema, the source and canonical ASTs, and Core. Only macro output makes
  it; there is no parser syntax.
- [ ] `dispatch!` receives two expressions. The second must be a list literal;
  the brackets are syntax that expansion consumes, never a runtime list. A wrong
  argument count or a non-literal list is a macro error.
- [ ] Typing: the subject is native `int`; all branches have one result type.
  Dispatch itself adds no trait or representation constraint. A Big integer is
  decoded explicitly by the caller (`Builtin.unIData x`).
- [ ] Lowering: one direct UPLC `case` on the subject with the branches in order.
  The subject is evaluated once; only the selected branch runs. A negative or
  out-of-range subject fails as UPLC does. No guards, fallback, decoding, or
  runtime list. An empty branch list is allowed. Ordinary integer literal `case`
  lowering does not change.
- [ ] Every Core and AST pass handles the new node in an explicit arm, not through
  a catch-all.

Evidence: `dispatch!` expansion and UPLC snapshots (one direct `case`, no list,
no guards); fixtures for first and last branch, subject evaluated once,
unselected branch effects, out-of-range and negative subjects, empty branches,
function and Term results, branch type errors, and macro argument errors.

## Chunk 6: case shape and pattern quote

Guide: `caseIf! value of ...`, a pattern match on opaque Data.

- [ ] Parse and transport `name! subject of` with arms. Each arm has its own
  scope. `name! (subject) of` is the case form; `name!(...)` stays a call.
- [ ] Add patterns to quote and transport.
- [ ] Coverage runs only on the final expanded module.

Evidence: `caseIf!` expansion and UPLC snapshots; fixtures for arm scopes,
copied arms, and shape errors.

## Chunk 7: binding shape

Guide: `expect! pattern = value`; on a mismatch it traces and fails.

- [ ] Parse and transport `name! pattern = value` in `let` and `do` blocks. The
  macro receives the pattern, the value, and the rest of the nearest block.
- [ ] The pattern is not in scope in its own value. A binding splits a recursive
  `let` group: later groups see earlier bindings, not the reverse.
- [ ] Quote and splice the remaining body without a change of its scope.

Evidence: `expect!` snapshots for a match and a mismatch (trace, then failure);
fixtures for two dependent bindings, shadowing, and evaluation order.

## Chunk 8: declaration shape and declaration quote

Guide: a fixture `@derive(Eq, Show)` on a union.

- [ ] Transport values, unions, aliases, traits, and impls, with docs,
  constraints, and constructor fields.
- [ ] Run imported attribute macros. A macro can keep, drop, or replace its
  declaration and add new ones. Retained attributes on returned declarations run
  next; a dropped declaration runs nothing more.
- [ ] Add declarations and impls to quote.
- [ ] Generated declarations take part in ordinary dependency order, recursion,
  and trait default checking.

Evidence: declaration expansion snapshots; generated impl used elsewhere in the
module; errors in generated impls with origins.

## Chunk 9: tooling

- [ ] LSP diagnostics after expansion, also after overlay edits.
- [ ] `nash docs` documents exported macros.
- [ ] The formatter keeps all macro syntax.

Evidence: LSP, docs, and formatter fixtures.

## Chunk 10: `Derive` in Base

- [ ] `Derive.nash` with Eq, Ord, Show, and Validate, written with quote.
- [ ] Constraints come from the field syntax only. Other types' bodies are not
  read. Big types keep automatic structural Eq and blanket ToData/FromData.
- [ ] Unknown or duplicate trait names give an invocation diagnostic.

Evidence: derive fixtures with Core/UPLC and execution; the `tests/base/derive`
project.

Dependency: derives with `list` or `map` fields type-check only with the coverage
rule (audit step 08, which needs step 05). Land 05 and 08 before this chunk, or
those fixtures wait for them.

After this chunk: audit step 10 (a macro `fail` text becomes the main error text)
and step 11 (derive for record aliases; it also needs step 08).

## Open decisions

| Decision | Before |
|---|---|
| Exact shape signatures for the four shapes | chunk 1 |
| Mechanism that stops macro output from making references to globals it did not name | chunk 3 |

## Reference implementation

The macros commit `yyypmxpx` and its 20 original commits (`8fadc763` to
`e275c289`, on the old base `f34365ea`) implement an earlier version of this
design. Read a commit before you take code from it. It also contains work that
this plan drops: `do` as a macro, block inputs, and property expansion.

References:

- Elm: `elm/compiler/src/Parse/Declaration.hs`, `Parse/Expression.hs`,
  `Canonicalize/Expression.hs`, `Canonicalize/Environment/Foreign.hs`,
  `Elm/Interface.hs`.
- Aiken: `crates/aiken-lang/src/gen_uplc.rs` (`generate_raw`) for compiling one
  definition to a program.
- Current code: `crates/nash-driver/src/compile.rs` (`compile_module`),
  `crates/nash-can/src/module.rs` (`canonicalize`), `crates/nash-plutus/src/term.rs`
  (`Term::Constr`), `crates/nash-plutus/src/machine/discharge.rs` (`value_as_term`).
