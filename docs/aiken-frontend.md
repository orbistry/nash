# Aiken-syntax frontend

## Decision and current finish line

Both source languages enter Nash at `nash_source::Module`. The official Aiken
parser supplies syntax only; Nash owns name resolution, representation checking,
type inference, exhaustiveness and interfaces. No Aiken AST, type checker or code
generator is used by Nash semantic stages.

```text
Project / editor buffers
  -> ModuleCatalog<URI, SourceSpec>
  -> shared FRONTENDS registry
       inspect -> owned dependencies + diagnostics -> dependency graph
       parse   -> arena-backed nash_source::Module
                    -> canonicalize -> solve -> nitpick -> interface
                    -> build_with -> specialize -> Core -> UPLC / Flat / CBOR
```

The branch is based on `release/main`. It includes the source-to-UPLC code
generation and native validator builds from `plan-7`. `nash check` ends at solved
interfaces. `nash build` uses the same frontend path and calls the backend while
the solved arena is alive. Supported Aiken libraries can run in native validators.
The bounded Aiken validator profile below also builds to UPLC, Flat and CBOR.
Nash codegen is available and used for both source languages.

The concrete finish-line project is
`crates/nash-driver/tests/fixtures/aiken/supported`, containing:

```aiken
pub fn add_one(value: Int) -> Int {
  value + 1
}
```

Run it with:

```sh
cargo run -p nash-cli -- check crates/nash-driver/tests/fixtures/aiken/supported
cargo run -p nash-cli -- check crates/nash-driver/tests/fixtures/aiken/unsupported --report=json
```

The first succeeds; the second deliberately contains an Aiken test declaration
and exits unsuccessfully with located diagnostic `NAF2201`.

The executable integration regression imports this Aiken function into a native
validator, matches its result against a fixed integer pattern, serializes UPLC and
runs it in CEK: input `41` succeeds and input `40` fails.

```sh
cargo test -p nash-driver --test build aiken_library_add_one_and_fixed_match_execute_in_native_validator
cargo run -p nash-cli -- build examples/vesting
```

A project containing only ordinary `.ak` library modules has no validator entry
point, so `nash build` does not emit a script for that library alone.

The validator project `crates/nash-driver/tests/fixtures/aiken/validator` contains
an Aiken mint handler with an `Int` redeemer, a user trace and a rejecting fallback:

```sh
cargo run -p nash-cli -- check crates/nash-driver/tests/fixtures/aiken/validator
cargo run -p nash-cli -- build crates/nash-driver/tests/fixtures/aiken/validator --trace-level verbose
cargo test -p nash-frontend-aiken --test validators
```

The last command compiles the same sources using exact-pinned Aiken 1.1.23 and
Nash, then compares CEK success, failure, unit results and user traces. Production
compilation never calls the official Aiken type checker or code generator.

## Crates and dependency direction

- **`nash-frontend`** owns `Frontend`, `FrontendRegistry`, `SourceInput`,
  `ModuleName`, `ModuleRole`, owned inspection/diagnostic types, and `ParseOutput`.
  It depends only on `nash-source`, `nash-region`, `bumpalo` and `url`.
- **`nash-frontend-nash`** exports `NashFrontend`. It wraps `nash-parse`, validates
  native headers/identity/explicit roles, and copies imports for inspection.
  It uses existing `nash-report` syntax conversion inside the adapter, projecting
  reports to owned frontend fields; parser errors never cross the boundary.
- **`nash-frontend-aiken`** exports `AikenFrontend`. It alone depends on
  `aiken-lang`. Private `spans`, `profile`, `validate`, and `lower` modules separate
  coordinates, compatibility policy, preflight and AST conversion. Declaration,
  type, pattern, expression and validator lowerers are separate files.
- **`nash-driver`** composes the static `FRONTENDS` registry, discovers files,
  derives source metadata, builds the graph, projects frontend diagnostics back
  to reports, and runs the existing compiler pipeline in dependency order.
- **`nash-codegen` / `nash-ir`** consume only Nash canonical nodes and solved
  metadata. Fixed constants lower directly to primitive Core/UPLC literals, and
  fixed patterns use primitive equality rather than native literal/`Eq` evidence.
- **CLI and LSP** use the same project/catalog/graph/build entry points.
- **`nash-report`** is unchanged. Nash canonicalization, solving and nitpick have
  no Aiken dependency or parser-specific branches.

The native adapter's report dependency reuses the existing error hierarchy rather
than duplicating it. This does not introduce a report dependency into the common
contract or an adapter dependency into semantic stages.

Adapter-only differential tests additionally depend on pinned `uplc = "=1.1.23"`
and use `nash-driver`/`nash-codegen`/`nash-plutus` as dev-dependencies. This test-only
backedge exercises the real compiler without exposing Aiken types or adding a
second production semantic pipeline.

## Contract and ownership

```rust
pub trait Frontend: Send + Sync {
    fn descriptor(&self) -> &'static FrontendDescriptor;
    fn inspect(&self, input: SourceInput<'_, '_>)
        -> Result<InspectOutput, FrontendFailure>;
    fn parse<'arena>(&self, arena: &'arena Bump, input: SourceInput<'arena, '_>)
        -> Result<ParseOutput<'arena>, FrontendFailure>;
}
```

`SourceInput` contains source text, URI, expected canonical module name and an
optional role. `None` lets syntax determine the role; `Some(Library|Validator)`
enforces explicit metadata. Current project configuration does not specify roles,
so discovery uses `None`, preserving native `validator module` headers.

`inspect` returns owned `ModuleDependency { module, region }` records and owned
diagnostics. It retains no arena or parser references. Both adapters use the same
parser and module-level validation for inspection and compilation. Aiken inspection
does not lower expression bodies, so a later lowering error can still retain its
valid dependency edges.

`parse` returns `ParseOutput { module: &nash_source::Module, diagnostics }`.
Compilation copies source into the existing **build-wide arena**; native parsing
borrows that copy, and Aiken lowering copies its owned AST strings/bytes/nodes into
the arena. The temporary Aiken AST is dropped before returning. Source, canonical
nodes, solved evidence and interfaces share the real build lifetime; canonical
addresses remain stable. No self-referential arena owner is introduced, and no
Drop-owning containers are placed in the arena.

`build_with(db, graph, catalog, finish)` retains that arena and the solved module
maps through the backend callback. The callback receives the original canonical
nodes, expression/pattern types and evidence, and returns owned artifacts. Frontend
or dependency-inspection failures prevent backend invocation.

`FrontendFailure` owns a boxed first diagnostic plus optional additional
diagnostics, making failure nonempty by construction. Adapters can also return
warnings with successful output. Registry selection accepts an explicit adapter
ID or the `.nash`/`.ak` extension; registration is composed by the driver, not the
contract crate.

## Project identity, imports and editor integration

The driver-owned `ModuleCatalog` maps URI to `SourceSpec`: source root, canonical
module name, optional role, optional package owner and optional explicit frontend
ID. Identity is derived relative to a configured source directory, with the
extension removed and path components joined by `.`:

```text
src/Json/Decode.nash -> Json.Decode
src/folder/math.ak  -> folder.math
Aiken use folder/math -> folder.math
```

Native headers must match that identity. Aiken has no header; its module identity
comes from metadata. No import resolution appends a source suffix or picks a
matching path tail. `aiken/builtin` maps to the compiler-owned `Builtin` interface.
Other imports resolve exactly through the catalog, without automatic stdlib,
package-download or case-folding behavior.

Names remain case-sensitive. Native code can import an Aiken file with a
Nash-compatible canonical path, such as `Math.ak`. The official Aiken import
grammar cannot spell uppercase native module paths, so arbitrary native imports
from Aiken are not claimed. Mixed native-to-Aiken graphs and Aiken-to-Aiken nested
imports are supported and tested. Mixed type APIs use the profile's lowercase
Nash type spelling described below.

Canonical interfaces are currently keyed by module name, not package. Duplicate
providers across roots/packages are therefore diagnosed instead of silently
choosing one; package-aware duplicate-name resolution is deferred. Package owners
still reach canonicalization for normal ownership/trait rules. Overlapping roots
that assign conflicting identities to one source are rejected.

Inspection failures are retained on their graph nodes. Failed sources publish no
interface, their dependents are blocked, and independent modules continue. Unknown
and ambiguous imports retain the import region. Existing cycle handling remains
unchanged.

Project LSP buffers, including unsaved `.ak` files, use the same discovery and
registry. Standalone buffers use their containing directory as the source root
and include open sibling buffers. Existing UTF-16 diagnostic conversion consumes
the same owned reports; there is no separate editor parser path. Formatting,
completion and other unrelated editor features remain outside this milestone.

## NashV1 compatibility profile

Policy is a private concrete `profile` module, not an unused extensibility trait.
It deliberately targets Nash semantics and representations, not every program
accepted by Aiken's separate type checker.

### Supported common syntax

- Public/private functions and constants; named/anonymous functions and calls.
- Complete polymorphic annotations and partial monomorphic function annotations;
  function, tuple, named type, alias and primitive annotations.
- Fixed integer, byte-array and string literals, including decoded hex integers.
- Integer arithmetic/comparisons, Boolean short-circuiting/negation, conditionals.
- Lists and list tails, tuples, ordinary field access, fully labeled constructor
  calls, sequential `let`, `when` and alternative patterns.
- Variables/discards, alias patterns, positional constructors, tuple/list/tail
  patterns and fixed primitive literal patterns.
- Undecorated algebraic data declarations, aliases, opaque constructor visibility
  and explicit public exports. Documentation comments are not projected into Docs.
- Module aliases and unqualified import renames.
- `fail` and simple traces. Trace continuations are thunked so tracing precedes
  evaluation of the continuation.

Nullary functions and calls lower through an explicit unit argument; a function
is never silently collapsed to a constant. Other function types use Nash's curried
function representation. Sequential bindings lower through strict, hygienic lambda
applications, not recursive Nash `let`; initializer references and shadowing keep
their lexical meaning.

### Primitive representations

| Aiken | Nash compiler-owned type |
|---|---|
| `Int` | `Builtin.int` |
| `ByteArray` | `Builtin.bytes` |
| `Bool` | `Builtin.bool` |
| `String` | `Builtin.string` |
| `Data` | `Builtin.Data` |
| `Void` | `Builtin.unit` |
| `List<a>` | `Builtin.list a` |
| `Pair<a, b>` | `Builtin.pair a b` |
| `G1Element` | `Builtin.bls_g1` |
| `G2Element` | `Builtin.bls_g2` |
| `MillerLoopResult` | `Builtin.bls_mlr` |

Native literal nodes are overloaded through `Literal` traits. Reusing them would
add constraints and require a Nash stdlib module even for `add_one`. Instead,
frontend-neutral `nash_source::Constant::{Int, Bytes, Str}` is carried by source
and canonical `Expr::Constant`/`Pattern::Constant`. Nash's existing solver assigns
the primitive type directly; fixed patterns use primitive equality rather than
user `Eq` instances. Native literal nodes and evidence behavior are unchanged.
Integers currently must fit signed `i128`; overflow returns `NAF2101`.

Aiken user type names lowercase their first ASCII character (`Choice` -> `choice`)
in declarations, references, imports and exports. Constructors retain their
spelling. This is deliberate: uppercase Nash data declarations require Big fields,
while Aiken primitive fields map to constant types. Little Nash data declarations
admit those fields without implicit Data casts. They do **not** promise Aiken wire
encoding. Nash `Storable` excludes Term, so `List`/`Pair` containing little user
ADTs can fail Nash representation checking. Full Aiken container/data-layout
compatibility needs future boundary conversions, not unchecked coercions here.

### Builtins

Integer operators call real `Builtin` operations directly, without importing
native operator traits. Division/modulo use `divideInteger`/`modInteger` as the
official compiler does. Greater comparisons negate the corresponding less
comparison, retaining left-to-right operand evaluation.

`aiken/builtin` has an explicit 33-name whitelist in
`crates/nash-frontend-aiken/src/profile.rs`: integer arithmetic/comparison;
byte-array operations; supported hashes/signature verification; string operations,
UTF-8 conversion, Data equality and serialization. Names are mapped explicitly,
not guessed by snake-to-camel conversion. Imported aliases and qualified accesses
share that mapping. Other builtin names return `NAF2201`. For example:

```aiken
use aiken/builtin.{equals_integer}
pub fn equal(left: Int, right: Int) -> Bool {
  equals_integer(left, right)
}
```

### Bounded validator profile

One validator declaration per `.ak` module lowers to an ordinary Nash validator
module with an exported `main`. Helpers, constants and imports use the existing
library lowering. The generated `main` takes zero or more raw `Data` parameters,
then one raw `Data` script context, and returns `unit` on success.

Accepted handler sets:

- One `mint(redeemer: Int, policy: ByteArray, transaction: Data)` handler and an
  optional `else(context: Data)` handler.
- An `else`-only validator, which receives the context unchanged, without
  destructuring its purpose.
- Redeemers may be explicitly annotated `Int`, `ByteArray` or `Data`. Named
  `Int`/`ByteArray` redeemers are decoded even if the body does not use the value;
  `_ : Data` discards a redeemer without decoding.
- Validator parameters, transactions and fallback contexts are raw `Data`;
  omitting their annotations selects this profile's raw boundary. Mint policy
  arguments must explicitly use `ByteArray` and are decoded even when discarded.
- Handlers return `Bool`: `True` becomes `unit`, `False` becomes an explicit
  error. An omitted fallback fails. User traces preserve evaluation order.

For mint dispatch, the official V3 context layout is constructor fields
`[transaction, redeemer, purpose]`, with mint at purpose tag `0`. Other purpose
tags invoke the fallback. `unConstrData`/list projections validate the parts
needed for dispatch; `unIData` and `unBData` validate decoded boundary values.
Discarded raw `Data` fields remain unforced; mint policies are always decoded
before redeemers, as in the official compiler. This is **not**
an independent, exhaustive ledger-context schema validator: raw `Data` stays raw,
and the profile does not promise to reject extra fields or every malformed
ignored component.

The official compiler's `TypedPattern::mint_purpose` decodes the policy to bytes
even though its internal prelude constructor advertises `Data`. Differential
execution caught the mismatch; requiring `ByteArray` avoids accepting a source
annotation whose runtime representation would silently differ.

Located `NAF2301` rejects multiple validator declarations, multiple/non-mint
handlers, wrong arities, non-Boolean return annotations, non-Data parameters,
unsupported boundary annotations and primitive-shadowing boundary imports.
Explicit library metadata cannot contain a validator; explicit validator metadata
requires a declaration. `main` is reserved for the generated entry point.
Handler calls such as `example.mint(...)`, argument patterns/renamed labels and
references to generated `main` are unsupported rather than silently reinterpreted.

The compact differential tests cover mint/fallback dispatch, raw parameters and
contexts, named policies/transactions, integer/byte-array redeemer checks,
unused-but-named redeemer decoding, discarded-policy validation, explicit/default
fallback failure and traces.
They use the official parser, inference and codegen **only as a test oracle** and
compare execution outcomes rather than script bytes or costs.

### Intentional exclusions

Located `NAF2201` diagnostics reject tests, benchmarks, environment/configuration
modules, custom encoding decorators, type holes, partial polymorphic annotations,
polymorphic local ascriptions, polymorphic `==`/`!=`, pipelines requiring inferred
arity, `expect`/Data casts, backpassing/multi-pattern assignments, Pair
construction/patterns, unknown-arity tuple indexing, curve literals, record updates,
trace formatting/trace-if-false, labeled function calls/renamed parameter labels,
and labeled/spread/type-qualified constructor patterns. Invalid duplicate bindings,
qualifiers and primitive-shadowing type declarations are also rejected.

Spend, withdraw, publish, vote and propose handlers, optional datum conversion,
custom user-data boundary layouts and complete validator ABI compatibility remain
outside this profile. They require their own checked lowering and official runtime
comparisons. Exact package/stdlib compatibility, blueprints and formatter support
are not claimed.

## Diagnostics and dependency maintenance

Frontend diagnostics carry a stable code, severity, title/message, optional primary
region, primary/secondary labels, context, help and suggestions. Regions are
one-based UTF-8 byte columns, converted from Aiken byte spans using a precomputed
line-start table; CRLF and non-ASCII text preserve byte coordinates. Native syntax
reports retain their existing codes/spans/labels and owned plain diagnostic text.
Nash semantic errors remain ordinary Nash reports for terminal, JSON and LSP.

- `NAF1001`: unknown frontend.
- `NAF1002`: unknown canonical import.
- `NAF1003`: ambiguous/reserved module identity or import.
- `NAF2001`: official Aiken parser failure.
- `NAF2101`: integer outside the temporary source literal range.
- `NAF2201`: unsupported NashV1 feature.
- `NAF2301`: unsupported validator profile or role/boundary mismatch.

The production Aiken dependency is exact-pinned `aiken-lang = "=1.1.23"`; its MSRV requires
Rust 1.94.1, now selected by `rust-toolchain.toml`. The lockfile is checked in.
An update must review upstream untyped AST/parser changes, builtin naming/signatures
and diagnostics, then run the focused adapter/driver tests plus workspace checks.
The compact compatibility coverage is native regression, `add_one` plus typed
imports, unsupported declarations/regions, fixed primitives, scope/nullary behavior,
exports and CLI/LSP paths. Backend regressions cover Aiken-backed validator execution
and fixed integer/byte/string pattern equality, aliases and native-literal
fallthrough. The bounded validator profile has pinned official runtime differential
tests; extending handler/data-layout support requires extending those checks.

An upstream parser-only `aiken-syntax` extraction remains desirable but is not
available or required at runtime. Proposing that upstream split and replacing the
dependency are Phase 3 follow-on work; one private AST version keeps that change
local to this adapter.
