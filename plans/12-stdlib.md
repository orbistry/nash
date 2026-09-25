# Plan 12: `nash/base` standard library

Goal: write `crates/nash-driver/base/` in Nash per [docs/stdlib.md](../docs/stdlib.md) and
wire it into the compiler: embedded package, default imports, the
synthetic `Builtin` module, the trait modules and twin types, the type
modules, decoders, `Prop`, `Test`, `Ast`, `Cardano.*`.

Prerequisites, by chunk (each chunk's Nash must type-check with the
compiler features available when it lands):

| Chunk | Needs |
|---|---|
| 1 skeleton, 2 default imports, 3 `Builtin` | nothing beyond today's pipeline |
| 4 kinds-aware twin types | plans/02 (kinds) |
| 5 trait modules, operators, `Lift`, `ToData`, `FromData`, `Validate` | plans/03 (traits; chunk 12 there is this chunk's file list) |
| 6 type modules, 7 `Data`/`Map` modules | plans/03; `Data` patterns from data.md |
| 8 `Prop`, 9 `Test` | plans/03, plans/10 (tests block, sequencing `do`, `prng` protocol, runner) |
| 10 `Ast`, `Derive` | plans/11 chunk 4 (tags) and chunk 10 |
| 11 `Cardano.*` | chunk 7 |

Module layout is docs/stdlib.md "Layout": one file per trait, one module
per type pair named by the uppercase name, helpers accepting either outer representation
only. `Prelude` is the `infix` table, its helper functions, and the tuple
impls. There is no Big `String` and no `Data.List`-style module family.

Crates touched: `nash-can`, `nash-driver`, `nash-cli`,
`nash-codegen` (builtin lowering), `crates/nash-driver/base/`.

References:

- Elm: `elm/compiler/src/Elm/Compiler/Imports.hs` (`defaults`),
  `Canonicalize/Environment/Foreign.hs` (`createInitialEnv`),
  `Elm/Kernel.hs` (how Elm binds native code; we bind builtins by table
  instead).
- Aiken: `crates/aiken-lang/src/builtins.rs` (`from_default_function`,
  `prelude`, the builtin type table), `crates/aiken-project/src/lib.rs`
  (stdlib is a normal dependency; we embed instead), Aiken stdlib
  `aiken-lang/stdlib` for API shape (`list`, `option`, `cbor`, and generation helpers).
- Current code: `crates/nash-can/src/environment/foreign.rs:18`
  (`create_initial_env`, the `List` pre-seed at :34),
  `crates/nash-driver/src/project.rs:70` (`discover_modules`),
  `crates/nash-driver/src/source.rs:224` (`OverlaySource`),
  `crates/nash-driver/src/compile.rs:86` (`build_sync` interface map),
  `crates/nash-plutus/src/builtin/default_function.rs`,
  `SPEC.md` and `nash-can/src/defaults.rs`.

Conventions: type variables `'a`; lowercase bare type names are little,
uppercase Big.

---

Plan 10 implements the minimum `Prop` and `Test` modules required by its runner.
Chunks 8 and 9 here extend and verify those modules; they must not duplicate or
replace the tested PRNG, replay, label, and assertion protocols.

## Current status

Reconciled with chunk 8 implementation (2026-09-25). Base currently
ships 30 embedded Nash modules. Plan 12 remains incomplete in `SPEC.md`.

| Chunk | Status | Remaining work |
|---|---|---|
| 1 embedding | complete | none |
| 2 default imports | complete for shipped modules | extend catalog as modules land |
| 3 Primitive / Builtin | complete | none |
| 4 twin types | complete | none; helper APIs belong to chunks 5–6 |
| 5 traits / operators | complete | none |
| 6 type modules | complete | none |
| 7 Data / Map | complete | — |
| 8 Prop | complete | none |
| 9 Test | complete through Plan 10 | preserve existing runner protocol |
| 10 Ast / Derive | not implemented | requires Plan 11 |
| 11 Cardano | not implemented in Base | library modules and ledger golden tests |

Latest full validation is recorded with chunk 8 below.
Tests use compiler libraries and the evaluator, never a Nash CLI subprocess.

## Chunk 1: package skeleton and embedding — complete

- [x] Store the foundation Nash sources in `crates/nash-driver/base/src/`.
- [x] Embed them through `nash-driver/build.rs`; no separate Rust crate or Nash package manifest.
- [x] Discover bundled modules automatically, with stable `nash-base:///Module.nash` URIs and compiler-owned `nash/base` identity.
- [x] Read sources offline, independently of checkout and current directory; reject virtual-source writes.
- [x] Reserve bundled module names and package identity against application/dependency replacement.

Base changes ship with a compiler release. Applications do not list Base as
a dependency or install its sources. The language server uses the same driver
source provider.

## Chunk 2: default imports — complete for shipped modules

- [x] Share `nash-can::defaults::MODULES` between dependency discovery, canonical scope, and diagnostic localization.
- [x] Expose Prelude operators/helpers, traits and methods, primitive names, and little constructors in application modules.
- [x] Keep Big twin constructors qualified; make `Test.label` unqualified only in tests blocks.
- [x] Preserve original source imports; formatting must never serialize implicit imports.
- [x] Compile all bundled modules and an application without imports or dependencies through the in-process driver.

Base modules import explicitly to avoid bootstrap cycles. Extend the catalog
when later chunks add new modules; do not install placeholder interfaces.

## Chunk 3: synthetic `Primitive` and `Builtin` — complete

- [x] `Primitive` owns compiler-known types, constructors, representation traits, and unchecked `coerce`.
- [x] `Builtin` owns only actual Plutus Core builtin functions from `BUILTINS`.
- [x] Builtin signatures use primitive type identities; codegen dispatches `Primitive.coerce` separately.
- [x] Test the strict Builtin inventory and rejection of `Builtin.coerce`.

`Primitive.coerce` is runtime identity with independently polymorphic input
and output. It checks neither representation nor shape. The normal Nash
blanket impls of `ToData` and `FromData` use it; `Validate` remains opt-in.

## Chunk 4: compiler-known types and the twin type modules — complete

- [x] Seed compiler-known types from `nash_ast::primitives::PRIMITIVES`,
  homed in `Primitive`; there is no second builtin type table.
- [x] Ship `Bool`, `Unit`, `Option`, and `Ordering` modules with
  their Big/little twins. Primitive `bool` and `unit` remain compiler-known.
- [x] Resolve little constructors unqualified and Big twins qualified
  (`Some` versus `Option.Some`). Restrict duplicate twin constructor names
  to the bundled package; reject ordinary user duplicate constructors.
- [x] Give primitive Data constructors their native decoded payload types.
  `Constr` has one `pair int (list Data)` payload; `Constr pair(tag, fields)`
  explicitly destructures it with native UPLC case. `Constr payload` binds
  the whole pair. `Builtin.constrData tag fields` constructs from two values.
- [x] Compile bundled modules and import-free applications through the
  driver. Verify storage constraints and little/Big constructor resolution.

Evidence: `crates/nash-ast/src/primitives.rs`,
`crates/nash-can/src/environment/foreign.rs`,
`crates/nash-driver/base/src/{Bool,Unit,Option,Ordering}.nash`,
`crates/nash-driver/tests/bundled_base.rs`, `tests/base/app/src/Main.nash`,
and canonicalization/type-inference snapshots.

Further helper functions and trait API coverage belong to chunks 5–6.

---

## Chunk 5: trait modules, operators, `Lift`, `ToData`, `FromData`, `Validate` — complete

- [x] Ship the trait modules, literal instances, Prelude operators/helpers,
  tuple Eq/Ord/Show instances through arity four, and lazy boolean lowering.
- [x] Overload boolean and unit expressions through FromBool/FromUnit, with
  little defaults and Big twin instances. Keep qualified constructors and
  boolean/unit patterns fixed. Snapshot inference, custom conversions, and execution.
- [x] Ship Lift instances, blanket Big ToData/FromData, and opt-in Validate.
  Map Lift explicitly requires Big keys and values; native pair components
  are rejected. Pair destructuring uses native UPLC case.
- [x] Verify native `trace`, `todo`, and `fail` semantics at verbose and silent trace levels; no redundant Debug module.
- [x] Audit planned instances and execute embedded Base trait/operator tests through compiler libraries.

The implemented structure and executable coverage are listed below.

**Files**

- `crates/nash-driver/base/src/Eq.nash`, `Ord.nash`, `Show.nash`, `Num.nash`, `Integral.nash`, `Semigroup.nash`, `Monoid.nash`, `Functor.nash`, `Applicative.nash`, `Monad.nash`, `Lift.nash`, `Data.nash`, `Literal.nash`
- `crates/nash-driver/base/src/Prelude.nash` (the `infix` table and the tuple impls)
- `crates/nash-driver/base/src/Bool.nash` (`not`, `and`, `or`, `xor`)
- `crates/nash-driver/base/src/Option.nash`, `Ordering.nash` (their `Eq`/`Functor`/`Applicative`/`Monad`/`Lift` impls)
- `crates/nash-codegen/src/can_to_core.rs` (plans/07: `Bool.and`/`or` delay the second argument)

**Change**

Write each trait of traits.md "Core trait hierarchy" in its own module
with the impls for compiler-known types listed in docs/stdlib.md "Trait
modules"; `Lift.nash` holds representation.md's impl table (the reflexive
`Lift 'a 'a` is compiler-provided and not written); `Prelude`
gets the `infix` table, the operator helper functions, and the tuple
impls; `Bool` gets the `bool` functions; tracing and failure use native
`trace`, `todo`, and `fail` syntax directly. Impls for the twin types go in
the twin's module.

`trace`, `todo`, and `fail` are reserved expression syntax, not module
functions. Tests verify returned values, trace ordering before failure,
and silent trace removal. Builtin contains only actual Plutus builtins.

Base modules use explicit imports. Preserve the acyclic bootstrap chain:
`Bool`, `Unit`, and `Ordering` import `Lift`; `Bool` and `Unit` also import
`Literal` for their literal instances. `Eq` imports `Literal` for its boolean
expressions. `Ordering` also imports `Eq`. `Prelude` imports its required
traits and `Bool`; the application default-import catalog independently
exposes all shipped traits. Later type modules may import `Prelude` for
operators without adding reverse dependencies.

`crates/nash-driver/base/src/Functor.nash` carries `impl Functor list` with a local
recursive `mapList`; chunk 6's `List.map` is `Functor.map` specialized at
`list`, so `List` imports `Functor`, never the other way round.

`crates/nash-driver/base/src/Prelude.nash` provides the documented `infix`
block (`infix non 4 (==) = eq`, `infix left 6 (+) = add`,
`infix left 7 (/) = div`, `infix right 5 (::) = prepend`, ...), `identity`,
`always`, `applyForward`, `applyBackward`, `composeLeft`, `composeRight`,
`prepend = Builtin.mkCons` (kept here because `List` imports `Prelude`;
named `prepend` because `Cons` is the `Cons` module's constructor), and
`impl (Eq 'a, Eq 'b) => Eq ('a, 'b)` and friends up to 4-tuples for
`Eq`, `Ord`, `Show` (tuples count as defined in `nash/base` for the orphan
rule).

Lazy `and`/`or`: codegen recognizes `VarForeign { home: Bool, name: "and" | "or" }`
in call position with two arguments and emits `if a then b else False`
for `and`, or `if a then True else b` for `or`,
directly (the `if` is already lazy). Partial applications of `and` fall
back to the strict function.

**Elm/Aiken reference**

Elm `Basics.elm` for the operator table, precedences, and
`&&`/`||` (Elm's compiler special-cases them in `Optimize/Expression.hs`).
Aiken `builtins.rs` `prelude` for `Ordering`, `Option`, and the
`ToData`-like `Data` conversions (`builtins::data`).

**Tests**

- `crates/nash-driver/tests/base_traits.rs` compiles and evaluates seven import-free Nash fixtures against embedded Base. Snapshots record Nash source and outcomes for equality/ordering, numeric/literal traits, Show, semigroup/monoid, functor/applicative/monad, Lift/Data, and Prelude/native debugging syntax. The debugging fixture also runs with silent traces.
- Named and operator boolean calls preserve laziness in ordinary expressions and assertions; partial applications remain strict.
- nash-can/nash-solve: `1 + 2` resolves to `Num int`; Big arithmetic uses normalization helpers such as `Int.add`, returning `int`; numeric operator traits remain little-only; `lift` wraps `list Int` as `List Int` without converting elements; reflexive Lift works for Big and little types.
- codegen: `False && fail "x"` evaluates to `False` (laziness).

**Done when** the in-process Base compilation tests and the user-facing operator tests pass.

---

## Chunk 6: type modules — complete

- [x] Ship Int, Bytes, String and List APIs and complete Option helpers.
- [x] Accept Big/little outer inputs independently; return little outer results.
- [x] Preserve elements in all Lift/lower instances; make identity universal.
- [x] Keep imports acyclic and List.map delegated to Functor.map.
- [x] Snapshot source and evaluated helper outcomes through the existing in-process Base test helper.
- [x] Complete workspace validation and review snapshots.

Implementation lives in `crates/nash-driver/base/src/`. Exact signatures and
boundary behavior are documented in docs/stdlib.md. All Lift conversions
change only the outer representation. UTF-8 encoding is explicit in String.
List range is end-exclusive; negative take/repeat is empty, negative drop
preserves input; map2 truncates; sorting is stable. Scalar wrappers retain
documented builtin failure behavior.

Tests in `base_traits.rs` compile embedded Base, then execute Nash fixtures
for Lists and TypeHelpers as well as the existing trait fixtures. They cover
mixed representations, preserved elements, callback results, empty and invalid
inputs, stable ordering, hashes, UTF-8, and lazy booleans. No CLI subprocesses.

**Validation:** formatting and strict Clippy passed; 3,386 workspace tests
passed, 3 ignored. Snapshot checks passed with no unreferenced snapshots.
Read-only review covered API semantics, conversion evidence, and source snapshots.

---

## Chunk 7: `Data` and `Map` modules — complete

- [x] Ship Data.serialise, Data.tag and Data.fields alongside the Data traits.
- [x] Make tag/fields direct constructor accessors; non-constructor input fails.
- [x] Ship Data.Decode and Data.Encode with source snapshot and round-trip tests.
- [x] Use decoder function aliases with Functor/Applicative/Monad composition.
- [x] Decode map entries into Cons tuples; use Cons for decoder alternatives.
- [x] Reject malformed UTF-8 through the recoverable string decoder.
- [x] Ship right-biased Map.union returning a little list of pairs.
- [x] Ship Map construction, lookup, update, removal, folding and collection helpers.
- [x] Make Map.keys/values normalize Big input internally through Lift; relational
  inference determines the unused component, while preserving all element types.
- [x] Embed nested Base modules and include them in implicit qualified imports.
- [x] Complete full workspace validation.

Implementation: `base/src/Data.nash`, `base/src/Data/Decode.nash`,
`base/src/Data/Encode.nash`, `base/src/Map.nash` under `crates/nash-driver`.
Authoritative APIs and behavior are in `docs/stdlib.md` and `docs/data.md`.

Decoder composition uses the existing language traits and `do`, with no custom
numbered mapping/field helpers and no new compiler decoder behavior. `constr`
checks the tag and decodes the original node; `field` selects its zero-based
field. Missing fields return None, extra fields are allowed. Recoverable
UTF-8 validation is implemented in Nash. Functions supplied by the user can
still fail explicitly.

Decoded maps use Cons tuples because native pair construction requires Data
components. Map operations preserve elements and return little collections;
construction uses native Data pairs. Construction/encoding does not deduplicate;
get finds the first match, remove deletes all matches, insert appends one new
entry after removing existing matches, and union is right-biased. A native pair
list needs no fromList wrapper; explicit lift constructs its Big representation.

Tests use the shared in-process Base snapshot runner, including integer/bytes/list
round-trip properties, invalid UTF-8, decoder composition, function-valued map
entries, duplicate keys, map ordering, and constructor accessor failures.

Validation: formatting and strict Clippy passed; 3,432 workspace tests
passed, 3 ignored. Snapshot tests run without a custom Rust stack setting.

---

## Chunk 8: `Prop` — complete

- [x] Ship prng/generator types, choice bounds, seeded draws and validated replay.
- [x] Use nested little Choice/Group traces with strict replay, consumed-trace reduction, and ordinary generator Functor/Applicative/Monad instances.
- [x] Rebuild structures in Nash and evaluate the prepared property once, with consumed-draw feedback for reduction.
- [x] Ship choice, constant, intBetween, intAtLeast, int, bool, option, listOf,
  listBetween, tuple2, oneOf, frequency, suchThat, bytes, bytesBetween and
  bytesExactly; sequence draws with generator do or explicit state.
- [x] Use arbitrary-precision integer generation built from u64 choices; normalize
  Big/little bounds and weights. Arbitrary Data generation is out of scope.
- [x] Test Nash generators against the Rust runner and replay protocol in
  `crates/nash-driver/tests/testing_base.rs`.
- [x] Audit the complete docs/stdlib.md generator API and remaining properties.
- [x] Verify the specific shrinking/range properties listed below.

Extend the existing implementation; do not replace its tested protocol.

**Files**

- `crates/nash-driver/base/src/Prop.nash`
- `crates/nash-test/src/prng.rs` (plans/10 chunk 5: `Prng::from_seed`, `from_trace`, `to_term`, `from_term`)

**Change**

docs/testing.md "Generators", verbatim: the **little** `prng` ADT that the
runner builds as native constructor terms, the `generator 'a` function alias,
direct draws or ordinary Monad composition, `choice` as the single primitive
over `u64` integer choices, and the generators listed
in docs/stdlib.md "`Prop`" built on `choice`.

**Implementation and protocol**

Use `crates/nash-driver/base/src/Prop.nash` as the current implementation,
with `crates/nash-test/src/prng.rs` and docs/testing.md for the wire contract.
Choices are u64 integers. Invalid bounds fail; exhausted or invalid replay
returns None. Seeded/Replayed payloads use little types. The existing runner owns
sampling, replay and shrinking; preserve their behavior when extending APIs.

**Acceptance coverage**

`PropHelpers.nash` runs through the shared in-process Base snapshot runner.
It covers arbitrary-size bounds and offsets, strict replay after rejected wide
samples, weights above u64, invalid/zero weights, optional draws, byte bounds,
filter attempts 100/101, and isolated replay groups. The tuple2 do rewrite
preserves the two recorded groups and unused sibling choices.

Generated properties verify integer ranges and byte lengths. Counterexample
snapshots show listOf int reducing to both `[]` and `[0]`, and a wide-range
counterexample reducing to its lower bound. Existing protocol tests continue to
exercise seeded/replayed choices, malformed traces and the Rust runner.

Nonnegative offsets use small-biased bit widths rather than zero-or-u64 chunks.
A seeded 256-draw snapshot checks zero, small nonzero and larger offsets; explicit
replay cases cover 1, 2, 10, 100, 255, 256 and values beyond u64.
CEK measurements showed exact modular powers cheaper than the previous
Int.pow implementation. Int.pow2 now provides that reusable exact operation,
and Int.pow selects it for base 2. A budget snapshot covers both public helpers
and direct modular bound expressions. Prop uses Int.pow2 for its local width.

The generator distribution changed for the wide branch of Prop.int; older
recordings using its previous signed-64-bit layout are not replay-compatible.
No PRNG wire tags, runner protocol, reducer passes or codegen behavior changed.

Validation: formatting and strict Clippy passed; 3,445 workspace tests passed,
3 ignored.

---

## Chunk 9: `Test` — complete through Plan 10

- [x] Ship Test.label, property preparation and assertion-reporting helpers in Nash.
- [x] Expose label unqualified only inside tests blocks.
- [x] Preserve label/assertion logs even when ordinary user traces are silent.
- [x] Integrate power-assert operand capture, failures and runner reporting.
- [x] Execute protocol tests through codegen and the evaluator, without CLI subprocesses.

Evidence: `crates/nash-driver/base/src/Test.nash`,
`crates/nash-driver/tests/testing_base.rs`, `crates/nash-codegen/src/assertion.rs`,
`crates/nash-codegen/src/tests/integration.rs`, and `crates/nash-test/`.

Test bodies use sequencing do, not a test monad. Test.assertAt and
Test.assertCapture invoke a thunk after tracing so trace lines are emitted first. Do not assume
ordinary Builtin.trace calls have special lazy argument evaluation.

---

## Chunk 10: `Ast` and `Derive` — not implemented

- [ ] Ship Ast/Derive with Plan 11's tag contract and macro tests.

**Files**

- `crates/nash-driver/base/src/Ast.nash`
- `crates/nash-driver/base/src/Derive.nash`
- `tests/base/derive/src/DeriveTests.nash`

**Change**

The `Ast` types and builders from docs/macros.md, matching
`crates/nash-macro/src/tags.rs` (plans/11 chunk 4; constructor tags are
declaration indices, so the two files change together); `Derive` from
plans/11 chunk 10. Both are plain Nash. Every `Ast` type is a little ADT
with native `string`/`int`/`bytes` fields, `option` slots, and `cons`
child lists (chunk 6); `Ast` imports `Prelude`, `Cons`, `String`, and the
trait modules. No `Data`, `Lift`, or Big type appears.

**Code** (`crates/nash-driver/base/src/Ast.nash` builders excerpt)

```elm
expr : exprNode -> expr
expr node = Expr { span = None, typ = None } node

name : string -> name
name s = Local s

var : name -> expr
var n = expr (Var n)

int : int -> expr
int n = expr (IntLit n)

call : expr -> cons expr -> expr
call f args = expr (Call f args)

tuple : cons expr -> expr
tuple es = expr (Tuple es)

and : cons expr -> expr
and es =
    case es of
        Nil -> expr (Var (Global primitiveModule "True"))
        Cons e rest -> Cons.foldl (\b acc -> expr (BinOp (Global boolModule "and") acc b)) e rest

primitiveModule : modname
primitiveModule = { package = Some "nash/base", name = "Primitive" }

boolModule : modname
boolModule = { package = Some "nash/base", name = "Bool" }

exprName : expr -> option string
exprName (Expr _ node) =
    case node of
        Var (Raw s) -> Some s
        Var (Global _ s) -> Some s
        _ -> None
```

**Elm/Aiken reference**

None; see plans/11.

**Tests**

`tests/base/derive/src/DeriveTests.nash` per plans/11 chunk 10; `Ast.nash` `tests`:
`exprName (var (raw "Eq")) == Some "Eq"`, `and Nil` is the `True` node,
`Cons.length (Cons (int 1) Nil) == 1`.

**Done when** plans/11 chunk 12's `decl_macro_derive_eq` snapshot passes.

---

## Chunk 11: `Cardano.*` — not implemented in Base

- [ ] Ship Cardano.Tx, Cardano.Address, Cardano.Value and Cardano.Time.
- [ ] Add real ledger context fixtures and decoding tests.

The vesting example's Cardano helpers are fixtures, not the planned library.

**Files**

- `crates/nash-driver/base/src/Cardano/Tx.nash`, `Cardano/Address.nash`, `Cardano/Value.nash`, `Cardano/Time.nash`
- `tests/base/cardano/golden/*.cbor` (real V3 script contexts)
- `tests/base/cardano/src/CardanoTests.nash`

**Change**

Big ADTs for the V3 `ScriptContext` per docs/stdlib.md, `Lift value Value`,
interval helpers. Golden tests decode real contexts with
`Validate.validate` and check a few fields.

**Code** (`crates/nash-driver/base/src/Cardano/Value.nash` excerpt)

```elm
module Cardano.Value exposing (..)

import Builtin

type alias Value = Map Bytes (Map Bytes Int)

impl Lift value Value where
    lift = fromData << Builtin.valueData
    lower = Builtin.unValueData << toData

lovelace : Value -> int
lovelace v = Builtin.lookupCoin #"" #"" (lower v)

quantityOf : bytes -> bytes -> Value -> int
quantityOf policy name v = Builtin.lookupCoin policy name (lower v)
```

Note the naming: `lift : value -> Value` here goes from Const to Big,
matching `Lift 'small 'big`'s direction (little is small).

**Elm/Aiken reference**

Aiken `stdlib/lib/cardano/transaction.ak`, `cardano/address.ak`,
`cardano/assets.ak` for field order and constructor tags (they match the
ledger). Plutus `plutus-ledger-api` `V3/Contexts.hs` is the source of
truth for the encoding.

**Tests**

`tests/base/cardano/src/CardanoTests.nash`: `validate` on each golden context succeeds with a typed value; `Tx.inputs` length matches; `lovelace` of the first output
matches the fixture.

**Done when** all fixtures decode and the in-process Base compilation tests stays green.

---

## Test harness

- `crates/nash-driver/tests/bundled_base.rs` compiles every embedded module and import-free applications through the driver library, without invoking the CLI.
- `crates/nash-driver/tests/testing_base.rs` executes the Base generator and test protocol through codegen and the evaluator.
- `crates/nash-driver/tests/vesting.rs` compiles validators against bundled Base and executes serialized UPLC.
- `nash-can` tests preserve source imports and enforce the real-only Builtin inventory.
- CI runs these with `cargo test`; unit and integration tests do not spawn the Nash CLI.

## Open questions

Prop and Test are already in the default module catalog, with Test.label
exposed only inside tests blocks. Keep target-version availability checks
for value and other Plutus builtins aligned with the compiler target policy.
