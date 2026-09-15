# Aiken source and project frontend

## Runtime compatibility contract (Plan 14)

The compatibility target is **Aiken 1.1.23**, with **Plutus V3**. The same input
must have the same observable result, failure behavior and relevant user traces.
This includes malformed and partly decoded Data: do not add validation where
the pinned compiler performs only shallow extraction or a typed view.

Compatibility does not require identical UPLC bytes, script hashes, execution
budgets, compiler diagnostics or blueprints. Source/project acceptance is tracked
by Plan 14 G1–G11; this is not a claim of full Aiken tool parity.

The consolidated [Plan 14](../plans/14-aiken-frontend.md) records the completed
frontend and runtime milestones and the remaining source/project work. Explicit
frontend-neutral layouts and conversions supersede the original lowercase
user-type shortcut, bounded validator dispatch and integer limit. The profile
below describes the implementation; the plan records behavioral and workspace
verification.

R1–R3 now pass the permanent assignment regression group: empty `[]`, `None`,
a `Data` module constant and concrete `List<Int>` encode correctly; a named
function's incompatible `Data` return annotation fails during checking.
Thirty-five pinned differential tests pass, including the original eleven and
the repair group. Plan 14 records the completed source/project gates separately
from the historical runtime milestone.

### Exact source and project reference

The semantic reference is Aiken `v1.1.23`,
`8949565a9969278846ffefe30bc3b892029dd318`. The selected standard library is
official `v3.1.0`, `7d5cee54b2bb4eea211ae3bd806c7c39e5fd899d`, with its
unchanged `aiken-lang/fuzz` `v2.2.0` dependency. The normal locked package path
checks all 62 project/dependency modules (810 declarations). Only Plutus V3 is accepted by pinned
`aiken-project/src/config.rs::validate_v3_only`. A manifest compiler-version
mismatch is a warning in `Project::new`, not a dependency constraint.
An empty `env/` directory does not require a default module; an environment
directory containing `.ak` files does (`Project::aiken_files`).

Nash is the ownership, lifetime and native-semantics reference. Exact Aiken is
the source-acceptance and runtime reference, including malformed and partly
decoded Data. Tests are regressions, not authority over a verified pinned rule.
Conversions must preserve shallow extraction, demand-driven decoding, full
expect validation, failure timing and relevant user trace order.

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
The supported runtime validator profile below builds to UPLC, Flat and CBOR.
Nash codegen is available and used for both source languages.

The compact acceptance projects are under
`crates/nash-driver/tests/fixtures/aiken/`:

- `full-language`: annotations, calls, patterns, operations and checked tools.
- `stdlib-project`: unchanged official stdlib `v3.1.0` and fuzz `v2.2.0`.
- `dependency-project`: direct/transitive packages and a typed validator boundary.
- `env-config-project`: default/named environments and synthetic configuration.
- `multi-validator-project`: named validators, parameter metadata and tools.

`cargo test -p nash-driver --test aiken_projects` materializes the committed
package archives into temporary normal Aiken build directories. No acceptance
test needs a network request. The archive provenance is recorded in
`crates/nash-driver/tests/fixtures/aiken/packages/README.md`; all 56 stdlib and
five fuzz library files were compared byte-for-byte against their pinned commits.

The older `unsupported` fixture contains a test declaration. That declaration
is now in scope: valid tests check without being executed or producing scripts.

The existing `add_one` integration regression imports an Aiken function into a native
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
- **`nash-frontend-aiken`** exports `AikenFrontend`. It alone uses the
  `aiken-lang` parser for source syntax. Private span, validation, documentation,
  declaration, type, pattern, expression and validator modules project the AST
  into arena-backed Nash nodes.
- **`nash-project-aiken`** uses exact-pinned project configuration/path models.
  Manifest parsing, workspace expansion, package access, source discovery and
  synthetic configuration are separate operations. Its public boundary contains
  only Nash-owned contracts from `nash-frontend`; it does not depend on the driver.
- **`nash-driver`** selects the closest owning project, composes `FRONTENDS`,
  resolves package-aware imports, projects diagnostics and runs the existing
  compiler pipeline in dependency order.
- **`nash-codegen` / `nash-ir`** consume only Nash canonical nodes and solved
  metadata. Fixed constants lower directly to primitive Core/UPLC literals, and
  fixed patterns use primitive equality rather than native literal/`Eq` evidence.
- **CLI and LSP** use the same project/catalog/graph/build entry points.
- **Semantic stages** receive only Nash nodes, declared call shapes and solved
  choices. They implement explicit neutral operations for grouped functions,
  conversion sites, equality, indexing, record updates and trace formatting.

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

`SourceInput` contains source text, URI, expected module name, origin and optional
role. Roles distinguish libraries, validators, environments and configuration.
Dependency origin controls the exclusion of dependency tests, benchmarks and
validators; it does not relax errors in imported production definitions.

`inspect` returns owned dependencies and diagnostics. It retains no arena or
parser references. Aiken inspection does not lower expression bodies, so a later
lowering error can still retain valid dependency edges.

`parse` returns the Nash module, source entry-point metadata and owned diagnostics.
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

`nash-frontend` owns `PackageId`, `ModuleKey`, `SourceSpec`, `ModuleCatalog`,
`ResolvedDependency`, project metadata, loader requests and diagnostics.
The catalog maps URI to a source specification; URI is an I/O/diagnostic location,
not semantic identity. `PackageId` contains name, version and source
(local path, GitHub, GitLab, Bitbucket or compiler).

Names are derived relative to configured roots:

```text
src/Json/Decode.nash -> Json.Decode
lib/folder/math.ak  -> folder.math
Aiken use folder/math -> folder.math
```

Native headers must agree with the catalog. Source imports remain module-only;
resolution records the exact provider package and URI at the import region.
Ambiguous visible providers are errors, never filesystem-order choices.
`aiken/builtin` and prelude names map to compiler-owned operations; the official
stdlib is ordinary imported source, not compiler-generated replacement bodies.

Names remain case-sensitive. Native code can import an Aiken file with a
Nash-compatible canonical path, such as `Math.ak`. The official Aiken import
grammar cannot spell uppercase native module paths, so arbitrary native imports
from Aiken are not claimed. Mixed native-to-Aiken graphs and Aiken-to-Aiken nested
imports are supported. Mixed type APIs retain original Aiken type spelling;
`Choice` is exported as `Choice`, not the former compatibility spelling `choice`.

Canonical module names contain package source/version and compilation context.
Definition, constructor, ADT, specialized layout and decoder keys inherit that
identity. Interfaces are stored by resolution scope plus `ModuleKey`, with
source-spelled aliases supplied to canonicalization only after resolution.
Independent workspace members retain distinct environment/configuration scopes,
even when they load the same hosted dependency. The canonical compilation
fingerprint includes root package identity, selected configuration/environment
source and import aliases; public metadata retains the source package identity.
Overlapping roots assigning conflicting source identities are rejected.

Inspection failures are retained on their graph nodes. Failed sources publish no
interface, their dependents are blocked, and independent modules continue. Unknown
and ambiguous imports retain the import region. Existing cycle handling remains
unchanged.

Project LSP buffers, including unsaved `.ak` files, use the same discovery and
registry. Standalone buffers use their containing directory as the source root
and include open sibling buffers. Existing UTF-16 diagnostic conversion consumes
the same owned reports; there is no separate editor parser path. Formatting,
completion and other unrelated editor features remain outside this milestone.

## Source and runtime implementation

Adapters retain source policy and provenance. Nash resolves names, checks types
and representations, specializes and generates code. The source/project gates
cover Aiken 1.1.23; excluded tool commands are listed below.

### Source syntax

Functions, constants, imports, aliases, data types and validators all use Nash
canonicalization, inference and codegen. Tests and benchmarks retain their
generator/signature checks but are not build roots. Invalid tool bodies remain
normal checking errors. Dependency tool declarations are omitted under the
pinned project rule.

Grouped function types retain Aiken call arity separately from native curried
functions. Labels come from resolved declarations/interfaces, never parameter
name guesses. Pipelines select argument insertion versus applying a returned
function using the declared or inferred group. Type holes remain inference
variables; local annotation variables keep their lexical scope.

Source nodes preserve type-directed equality, tuple indexing, record updates,
trace formatting and refutable pattern conversions. Backpassing and sequential
bindings preserve single evaluation. Native overloaded literals, trait evidence,
record-update restrictions and validator `main` behavior remain unchanged.

The recursive inference and codegen dispatchers use smaller expression-family
frames. LLDB located stack overflows in the former monolithic dispatchers on
unchanged stdlib/prelude programs; no larger stack or reduced source limit is used.

Nested pipelines use a deterministic per-module fresh-name supply. Reusing one
generated `$pipe` binder incorrectly unified independent callback inputs in the
unchanged fuzz library.

Record/module ambiguity is represented by a neutral `FieldOrModule` node.
Inference selects a valid record field before trying the imported module, as
specified by pinned `tipo/expr.rs:1163–1184`; selected call labels and argument
order survive into specialization. A failed field lookup still counts as a
lexical use, preserving the pinned initializer traces.

The frontend's explicit private-export policy is enforced on solved interfaces.
Aiken public value and constructor signatures cannot leak private types;
aliases expand before checking, and opaque wrapper implementation fields remain
private. Native Nash exports retain their existing rules.

Large partially applied constructor metadata is arena-referenced rather than
stored inline in every runtime `Ty`. Owned evidence keys and cold diagnostics
likewise avoid multiplying package-qualified identity payloads through hot
recursive compiler frames.

### Exact pinned syntax inventory

The exhaustive adapter matches follow `aiken-lang 1.1.23`'s `ast.rs` and
`expr.rs:639–783`, not a newer release. Direct means representation-preserving
lowering; desugaring preserves source order/scope; type-directed means a Nash
declaration or solved type selects the operation.

| Inventory | Forms | Classification and implementation |
|---|---|---|
| Definitions (8) | `Fn`, `TypeAlias`, `DataType`, `Use`, `ModuleConstant` | Direct/declaration elaboration in `lower/declarations.rs`; constant checks preserve their source site |
| Definitions | `Validator` | Desugared handlers and named entry metadata in `lower/validators.rs` |
| Definitions | `Test`, `Benchmark` | Tool declarations; `RunnableCheck` validates bodies, signatures and generators |
| Annotations (6) | `Constructor`, `Fn`, `Var`, `Hole`, `Tuple`, `Pair` | Direct type nodes in `lower/types.rs`; holes and local variables resolved by Nash |
| Expressions (24) | `UInt`, `String`, `ByteArray`, `CurvePoint`, `Var`, `ErrorTerm` | Direct constants/names/failure; G1/G2 bytes are validated curve constants |
| Expressions | `Sequence`, `Assignment`, `Fn` | Scope/signature desugaring, strict single-evaluation bindings and backpassing |
| Expressions | `List`, `Tuple`, `Pair` | Direct encoded-container construction, with solved payload layouts |
| Expressions | `Call`, `PipeLine` | Type-directed group/label resolution and pipeline insertion/application |
| Expressions | `BinOp`, `UnOp`, `LogicalOpChain` | Primitive arithmetic and short-circuit desugaring; equality selected by solved type |
| Expressions | `Trace`, `TraceIfFalse` | Type-directed formatting and single condition evaluation |
| Expressions | `When`, `If` | Pattern/branch lowering, including `is` alternatives |
| Expressions | `FieldAccess`, `TupleIndex`, `RecordUpdate` | Resolved fields/declarations and solved container types |
| Patterns (9) | `Int`, `ByteArray`, `Var`, `Assign`, `Discard`, `List`, `Pair`, `Tuple` | Direct patterns or encoded-container patterns in `lower/patterns.rs` |
| Patterns | `Constructor` | Declaration-directed positional/labeled/spread/qualified pattern resolution |
| Assignment/argument forms | `let`, `expect`, `is`, backpassing, multiple patterns, named/discarded/pattern parameters, renamed labels, `via` | Sequential binding/refutation/callback lowering; generated terms use the same call and conversion rules |
| Module kinds | `Lib`, `Validator`, `Env`, `Config` | Project role/origin controls discovery, defaults, synthetic source and entry eligibility |

Adding a pinned enum variant must break an exhaustive match. A classification
alone is not an acceptance gate: the unchanged stdlib, compact source fixtures
and runtime comparisons exercise these routes.

### Types and wire layouts

| Aiken | Nash compiler-owned type / representation |
|---|---|
| `Int` | `Builtin.int`, arbitrary precision |
| `ByteArray` | `Builtin.bytes` |
| `Bool`, `String`, `Void` | `Builtin.bool`, `Builtin.string`, `Builtin.unit` |
| `Data` | `Builtin.Data`, unchanged Data |
| `List<a>` | `Builtin.data_list a`, encoded elements |
| `Pair<a,b>` | `Builtin.data_pair a b`, pair of encoded Data |
| `(a,b,...)` | `Builtin.data_tuple (a,b,...)`, encoded Data fields |
| `Option<a>` | `Builtin.data_option a`, Some tag 0 / None tag 1 |
| Curve types | Compiler primitive G1/G2/ML-result types; validated G1/G2 constants and pinned serialisability restrictions |

The separate encoded container types preserve native Nash `list`, `pair` and
tuple behavior. They admit nested user types and heterogeneous tuples without
weakening native representation constraints. A standalone Pair encodes as a
two-element Data list. A List of Pair encodes as a Data map; other lists and
tuples encode as Data lists.

Fixed constants use `Constant::Int(i128)` for the inline range and
`Constant::BigInt(&str)` for normalized, arena-owned decimal digits outside it.
Both have primitive integer type. Expressions and patterns emit arbitrary-
precision Plutus integers. Native Nash literal syntax is unchanged.

User type names are never lowercased to select representation. Source `Union`,
canonical `Union` and `InterfaceUnion` carry:

```rust
DataEncoding::{Constr, List, Transparent}
DataLayout<'a> {
    encoding: DataEncoding,
    tags: &'a [u64],
}
```

Constructor field types stay in the owning union instead of a parallel duplicated
schema. The owner supplies qualified identity. Backend `AdtRef` contains the
qualified type name and instantiated arguments; `TypeEnv::layout` substitutes the
canonical fields into `AdtLayout { fields, data_layout }`. `Adts.layouts` is keyed
by that full reference, not an unqualified name. Interfaces preserve layout
metadata and canonical field templates. Constructor generation, projections,
pattern matching and validation consume the same specialized layout.

- Ordinary declarations encode `Constr(declaration_index, encoded_fields)`.
- Constructor `@tag(n)` overrides the tag; single-constructor type-level `@tag`
  is supported too. Tags include zero-field constructors.
- Type-level `@list` requires one constructor and encodes its fields as a Data list.
- Aiken opaque single-field wrappers without `@list` use `Transparent`: their wire
  value is the encoded field itself. This is an explicit layout, not native
  constructor unboxing or a name-based representation exception.
- Conflicting decorators within one decorator list, misplaced `@list`, colliding
  tags and unrepresentable counts are diagnosed. Type and constructor decorators
  can coexist: the constructor tag precedes the type tag, while type-level
  `@list` selects list storage. This corrects the bounded-profile rejection,
  following `tipo/infer.rs:1057–1097` and `gen_uplc/builder.rs:1378–1385`.
- External aliases carry `Alias.transparent`, deriving representation from their
  body rather than the alias name. Native aliases retain the original rule.

Native unions have `data_layout: None`; their existing representation and
constructor rules remain intact.

### Conversion interfaces and evaluation

Source/canonical `Expr::Convert { kind, typ, value }` retains the expression and
type source regions. `typ` is the output type; ToData's input type comes from
ordinary Nash inference, not an adapter type checker.

| ConversionKind | Contract |
|---|---|
| `ToData` | Recursively encode the solved runtime input |
| `FromDataShallow` | Extract the outer runtime representation only |
| `ViewData` | View raw Data as a Data-backed type, without inspection |
| `ValidateData` | Fully validate every required nested field, then decode |
| `FromDataBytesView` | Mandatory mint-policy byte extraction with its pinned internal type view |

Views require a Data-backed result at canonicalization, including through aliases.
The other operations lower to existing Core `CastKind` operations and the shared
`nash-codegen::casts` expansion. Decoder/checker helpers are shared by complete
runtime type and validation mode. Validation-only helpers avoid constructing
discarded decoded containers. Specialization preserves strict conversion effects,
including unused polymorphic bindings.

Ascriptions retain `ConversionSite` until the single inference planner resolves
them. The planner returns either a legal operation or a located checking error.
It runs after enclosing initializer/call constraints and before generalization;
deciding on a still-flexible input during continuation-first inference would
incorrectly constrain a later concrete list argument to Data. Resolved operations
are stored in `SolvedTypes::conversions` and consumed by specialization/emission.

| Source site | Source → target | Result and validation |
|---|---|---|
| Local annotated binding / module constant | Concrete serializable value → Data | `ToData`; no decode validation |
| Local annotated binding / module constant | Same type → same type | Identity |
| Named function result (complete or partial signature) | Int → Data | Checking error; no artifact |
| Named function result | Data → Data | Identity |
| Lambda result annotation | Int body annotated Data | Retain inferred Int result; no encoding |
| Function argument / record update field | Concrete serializable value → Data parameter/field | `ToData`; no added decode validation |
| `expect` / `is` cast pattern | Data → inferred or annotated serializable type | Full checking/refutation; unannotated Data patterns must determine their type before the body |
| Any implicit encoding site | String, function, or unconstrained root → Data | Checking error, not runtime coercion |
| Data cast pattern | Data → opaque type or container containing one | Checking error |
| Explicit annotated expect | Data → supported concrete type | Full validation, including unused fields/bindings |
| Validator parameter | Data → supported boundary type | Shallow extraction |
| Optional datum / Data-backed purpose | Data → Data-backed view | No inspection until demanded |

These assignment/signature rules follow pinned `tipo/infer.rs:690–708`,
`tipo/expr.rs:1457–1474,178–190,1982–1996` and
`tipo/environment.rs:1704–1717`. Lambda behavior is independently established by
`tipo/expr.rs:359–408,1901–1924`; its permissive annotation check does not replace
the inferred body result with Data. Parameter annotations and contextual argument
types constrain a lambda before checking its body; its written result annotation
remains a separate conversion decision. Unbound/generic root inputs, functions
and String do not gain implicit assignment encoding merely from a Data target.

Encoding demands follow emitted operations. Encoded containers and external
constructors retain their outer representation without demanding unused payload
layouts. A shape-only list-element demand selects map storage only for a known
Pair; unresolved nonpair payloads remain erased. Actual payload construction
still demands its encoding layout. This follows pinned `gen_uplc.rs:4004–4085,
4771–4793`, `gen_uplc/builder.rs:751–807` and `tipo.rs:386–395,535–542,1050–1054`.
No generic type default is introduced.

Pinned `gen_uplc.rs:4379–4384` implements `>`/`>=` by swapping operands into
less-than builtins, including right-before-left user traces. The adapter follows
that order rather than the former truth-equivalent negated comparison. The
runtime regression also checks short-circuiting, sequential shadowing, imported
renames, nullary calls and lambda annotation behavior. Native Nash is unchanged.

Full decoding supports Data, Int, ByteArray, Bool, String/UTF-8, Void, encoded
lists, pairs, tuples, Option, compressed G1/G2 points and explicitly laid-out user
types after substitution. It rejects wrong Data forms, constructor tags, arities,
invalid points and nested field types. Functions and ML results retain the pinned
serialisability restrictions. Checked decoding is distinct from shallow parameter
extraction: ignored nested tuple/list fields are not validated by a shallow cast.

### Validator calling convention and artifact metadata

Each named validator has a distinct generated entry function: zero or more raw
Data parameters followed by a raw V3 context. True returns unit; False fails.
The context fields are `[transaction, redeemer, purpose]`. Several validators in
one module compile independently; native validator modules still select `main`.

| Handler | Purpose tag | Handler inputs | Purpose fields |
|---|---:|---|---|
| mint | 0 | redeemer, policy, transaction | policy bytes |
| spend | 1 | optional datum, redeemer, output reference, transaction | output reference, optional datum |
| withdraw | 2 | redeemer, credential, transaction | credential |
| publish | 3 | redeemer, certificate, transaction | integer index, certificate |
| vote | 4 | redeemer, voter, transaction | voter |
| propose | 5 | redeemer, proposal, transaction | integer index, proposal |
| else | fallback | raw context | context unchanged |

Parameters use shallow extraction. Redeemers use full checked conversion even
when named but unused; use a named binding for decoded redeemers, or `_ : Data`
to discard raw Data. Mint policy decoding is mandatory, including discarded
policies. Publish/propose extract their integer index before invoking the handler.
Transactions and non-mint purpose arguments are raw Data or Data-backed user-type
views, not arbitrary primitive casts. Redeemer annotations are explicit; omitted
raw boundary annotations retain the frontend baseline's Data default.
The purpose argument field is projected even when discarded; this preserves the
official failure on a missing field without adding nested validation.

Spend requires `Option<Datum>`. Aiken's generated optional-datum binding is a
typed view, not `expect` from raw Data: nested datum decoding occurs when fields
are used. This preserves trace ordering and the official behavior of malformed
ignored values. Exhaustive matching of externally laid-out data follows the
official final-constructor default; it must not substitute for full validation.
Use explicit `expect` from Data where full validation is intended.

Each purpose can occur once. Spend has four arguments, others three. An explicit
else with all six purposes is rejected, matching Aiken. Else-only validators do
not unpack context. Other missing purposes reach else; an omitted fallback fails.
This is not an independent exhaustive ledger-context schema validator.

Compatibility tests use compiler traces disabled, as the real CLI does by default.
User traces preserve evaluation order; optional Nash `--compiler-traces` adds
implementation-specific diagnostic traces, not Aiken trace compatibility.

### Compiler prelude and builtin surface

`profile.rs` maps the exact pinned builtin names to compiler-owned operations.
Its completeness check compares the full upstream builtin inventory. The prelude
supplies Option, Bool, Ordering, Data, PRNG, Pair/container aliases and its
higher-order and diagnostic functions without native default imports.
The `from_int` implementation uses the internal name `integerDecimal`, avoiding
a collision with native `Literal.fromInt`.

Pinned `builtins.rs:429–443` advertises `tautology : fn(a) -> Void`, but
`1474–1507` implements a Bool result. Compatibility preserves that discrepancy,
including failure when the result is consumed as Void; it does not silently
repair the upstream program. Void equality checks its operands using `ChooseUnit`.

### Project loading and tools

Nearest-manifest selection accepts `aiken.toml` or `nash.jsonc`; a direct manifest
path selects explicitly. Both in one owning directory are an ambiguity.
Aiken sources use the pinned root/name rules for `lib/`, `validators/` and `env/`.
Config values are projected through the pinned `SimpleExpr` model into synthetic
source with Nash-owned metadata. Only a populated environment directory requires
`default.ak`; `--env` selects the environment import alias and config section.

The lockfile is the pinned flat resolved package set. Unchanged requirements keep
its versions; changed requirements regenerate that set. Dependency manifests do
not recursively override the root's lock. Packages materialize under
`build/packages/`, using the normal zipball cache or a prepared offline provider.
Missing/malformed inputs are diagnostics. Cache fallback retains a warning;
untracked package directories are not overwritten.

Workspace members compile independently, with explicit dependencies; workspace
membership alone does not create an import. Stable package ownership and separate
resolution contexts prevent cross-member config/environment leakage.

Solved validator metadata owns package/module/name, documentation, parameter
labels/order/types, handler purpose and datum/redeemer bindings. A layout registry
retains qualified type names, parameters, constructor tags, field order/types and
encoding, including aliases and declaration refinements. Project metadata retains
name, version, license, description, repository, compiler and Plutus target.
No later artifact consumer must reparse the Aiken AST.

Aiken output stems are `<package>@<version>.<module>.<validator>` with UTF-8 bytes
outside ASCII letters/digits/underscore/hyphen percent-encoded using uppercase
hex. Each entry emits `.uplc`, `.flat` and `.cbor`; collisions are errors before
writing. Native output stems are unchanged.

Blueprint serialization, parameter application, test/benchmark execution,
formatting, HTML documentation, completion and parser extraction are excluded.

### Runtime corrections to the original frontend baseline

Original type spelling and explicit layouts replace the former lowercase Term
shortcut, including across mixed-language interfaces. Encoded containers replace
native container representations only for Aiken. Runtime tests exposed the need
for shallow tuples and demand-driven optional datum behavior, distinct from full
expect validation. Opaque single-field Data transparency is now explicit.

The arbitrary-integer differential also exposed a shared Plutus CBOR bug:
negative bignum tag 3 stores `-1 - value`, not the absolute magnitude. Encoding and
decoding now follow CBOR; native source-language behavior is otherwise unchanged.

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
- `NAF2101`: invalid integer digits (there is no i128 compatibility limit).
- `NAF2201`: invalid/unknown adapter forms, including duplicate declarations and unknown builtin names; no valid production category uses this rejection.
- `NAF2301`: invalid validator declaration or role/boundary mismatch.
- `NAF2401`: invalid, conflicting or unrepresentable external data layout.
- `NAF2402`: invalid checked-conversion source.
- `NAP41xx`: located manifest, dependency, workspace, environment and configuration diagnostics.
- `nash::type::private_type_leak`: Aiken public signatures exposing private types.

The production Aiken dependency is exact-pinned `aiken-lang = "=1.1.23"`; its MSRV requires
Rust 1.94.1, now selected by `rust-toolchain.toml`. The lockfile is checked in.
An update must review upstream untyped AST/parser changes, builtin naming/signatures
and diagnostics, then run the focused adapter/driver tests plus workspace checks.
Acceptance combines the unchanged pinned packages, six real-path project tests,
the complete builtin/prelude classification checks, 35 pinned source/runtime
comparisons, native and mixed-source regression suites, and real CLI artifacts.
The final workspace run passed 3,272 tests; the three ignored tests are existing
documentation examples, not compatibility gates. `cargo insta test` passes with
no pending snapshots. Full command records and preserved historical failures live
in Plan 14.

An upstream parser-only `aiken-syntax` extraction remains desirable but is not
available or required at runtime. Proposing that upstream split and replacing the
dependency are optional follow-on work in Plan 14; one private AST version keeps
that change local to this adapter.
