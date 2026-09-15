# Plan 14 — Aiken frontend, runtime, source and project compatibility

## Status and scope

This is the single tracker for the former Plans 14, 15 and 16. Baseline repairs
R1–R3, source/project gates G1–G11 and applicable Chunks 1–19 are complete.
The earlier frontend/runtime milestones and their reported defects remain
recorded below as history. The final validation record identifies the checks
performed on the completed implementation; it does not infer a pass from that
history. Follow-on artifact serialization, runners and user tools remain excluded.

The source/project finish line does not include full Aiken tool parity. Tooling
and optional dependency extraction remain explicit follow-on work at the end.
The supported implementation profile is documented in
[`docs/aiken-frontend.md`](../docs/aiken-frontend.md).

### How to use this tracker

The current assignment is to resolve baseline repairs R1–R3, then complete the
remaining source/project gates G1–G11 and all applicable work in Chunks 1–19.
The repairs are prerequisites within this plan, not a new milestone or a second
tracker. Historical gate labels refer to recorded milestones. The unchecked
follow-on items are outside this assignment.

Record the starting commit and check the working tree before editing. Preserve
unrelated changes. Verify the current code and run a focused baseline check.
The completed-work record below is not proof that the current checkout passes.

Keep this file as the only task tracker. Use the goal prompt to start the work,
not as a second scope document. Apply the reference rules near the end of this
plan when code and documents disagree.

Do not stop after an audit, new interfaces, one chunk, or a sample build. Complete
the remaining gates on the final code. Record a real blocker against its gate;
do not mark a blocked gate complete or replace it with a weaker check.

Current implementation run starts at `633212a050c60c41abcf52325a65d4140294489d`.
The initial tree has 106 unstaged and two untracked paths; preserve this work.
Baseline: `cargo test -p nash-frontend-aiken --test validators` passes all
11 existing pinned differential tests. This is not a workspace pass.
The reference, malformed-Data, validation-depth, source-acceptance and tool
declaration clarifications in this assignment are recorded below and remain
binding; R1–R3 precede the remaining source/project gates.

Focused reproduction: `cargo test -p nash-frontend-aiken --test validators
assignment_encoding` reports one pass (concrete List<Int>) and four failures:
empty list/None require an erased layout, module constant has a type mismatch,
and the rejected function result is incorrectly accepted. Each case first
checks exact-pinned source acceptance/rejection.

Pinned references: Aiken `v1.1.23` commit
`8949565a9969278846ffefe30bc3b892029dd318`; selected official stdlib `v3.1.0`
commit `7d5cee54b2bb4eea211ae3bd806c7c39e5fd899d` (manifest compiler `v1.1.21`,
Plutus `v3`, dependency `aiken-lang/fuzz` `v2.2.0`). Project adapter crate
`aiken-project = "=1.1.23"` is available from crates.io.
Pinned `aiken-project/src/config.rs::validate_v3_only` accepts only V3.
`Project::new` warns, rather than rejects, when the manifest compiler version
differs from the running compiler; preserve that behavior, not an invented
semver requirement. `aiken_files` requires `env/default.ak` only when an `.ak`
file exists in `env/`, not for an empty directory. These qualify Chunk 12's
target/environment checks.
The pinned project model uses a flat resolved lock package set:
`deps/manifest.rs::resolve_versions` seeds it from root dependencies and
`Project::with_dependencies` loads those packages, without recursively merging
dependency manifests. Workspace `watch.rs::with_project` checks members
independently; membership does not implicitly expose another member's modules.
Dependency/workspace acceptance must use explicit dependency declarations and
the prepared resolved package set, not invent implicit member imports or a
newer package-version solver. Required direct/transitive source-import and
workspace gates remain required under these verified rules.

Continuation baseline at the same commit: 182 unstaged tracked paths and 22
untracked paths, all preserved. `cargo test -p nash-frontend-aiken --test
validators assignment_encoding`: five passed. Existing repair claims remain
supported by this focused check, not a new workspace pass.
`cargo test -p nash-driver --test aiken_projects`: named validators passed;
transitive layout metadata assertion, env/config Flat encoding and imported
record update failed; the official stdlib check aborted with stack overflow.
LLDB locates the overflow in the large recursive `infer_expr_inner` frame.
At this continuation baseline, G1–G11 remained open pending those integration repairs.

Commit completed, verified sections as they land. Keep shared compiler/project
contract changes together when splitting them would leave an unusable checkout;
record final acceptance and documentation in a subsequent commit.

### Final implementation and validation

Implementation commits:
- `c2e04c66` — Plutus Data serialization and discharged-value repairs.
- `ff78b987` — Aiken source/runtime, shared identities and project compilation.

Shared project contracts live in `nash-frontend`, below both the driver and
project adapter. Package/source/version identities reach interfaces, layouts,
specialization and owned validator metadata. Independent workspace resolution
contexts preserve environment/config selection. The implementation uses existing
arena lifetimes and concrete APIs, not the illustrative trait scaffolding below.

The unchanged stdlib exposed two integration defects: nested pipelines reused
one generated binder, and lambda parameter/context constraints arrived after
body operations. Hygienic canonical names and earlier parameter constraints fix
both without relaxing function-result conversions. `FieldOrModule` preserves
pinned record-first/module-fallback resolution; lexical uses remain observable.
Aiken private-export checking is an explicit frontend policy, leaving native
export behavior unchanged. Cross-list decorators and the prelude `tautology`
signature/runtime discrepancy follow the pinned rules recorded in the architecture
document. Recursive compiler frames and large runtime metadata were corrected
without raising stacks or lowering resource limits.

Final commands, run sequentially to avoid Cargo artifact interference:
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets` — passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — passed.
- `cargo test --workspace` — 3,272 passed, three ignored documentation examples.
- `cargo insta test` — passed; no snapshots to review.

The passing workspace includes all 35 pinned differential tests, six project
integration tests (including the non-ignored stdlib check), 15 offline project
loader tests and 16 LSP tests. The ignored examples are the existing driver
overview and parser `one_of`/number-literal documentation; no required gate was
skipped. Earlier snapshot failures, the growing-layout stack overflow and editor
URI regressions were repaired and rerun. Doctest artifact errors from overlapping
Cargo invocations disappeared on the sequential full run.

Real `cargo run -p nash-cli --` checks used temporary copies of the committed
fixtures with prepared normal package directories; official source was unchanged:
- `check <stdlib-project> --no-warnings` — 62 modules, 810 declarations.
- `check <dependency-project> --no-warnings` — three modules, four declarations;
  only the direct package is a root requirement, with the transitive package
  retained in the pinned flat lock.
- `check <env-config-project> --env preview --no-warnings` — five modules,
  ten declarations.
- `build <multi-validator-project>` — two distinct UPLC/Flat/CBOR sets,
  `alpha` 198 Flat bytes and `beta` 135 bytes. The project combines default/named
  environments, config, a test and a benchmark; the tools are not executed.
  It currently emits one non-fatal generated unused-variable warning (`$aiken2`).
- `build examples/vesting --out <temporary-directory>` — native `Vesting`
  515 Flat bytes and `VestingParam` 520 bytes, unchanged.
- Final repair CLI smoke: accepted R1/R2/control project checks and builds
  (126 Flat bytes); isolated R3 exits 1 for both commands, with no build directory.

All required source/project gates pass. No in-scope external blocker remains.
Temporary source probes and generated acceptance artifacts are removed after
verification. This is source/project compatibility, not full Aiken tool parity.

## Required baseline repairs

A review of the unstaged branch reported the three defects below. The review
reported 11 pinned differential tests and 1,788 affected-suite tests passing. Its
additional isolated probes reproduced the defects against exact Aiken `v1.1.23`.
It did not run the full workspace checks. Those results belong to that reviewed
working tree; they are not a new full-workspace pass.

This section records the review supplied by the project owner. The current
unstaged source was not included with the report. Reproduce each case on the
working tree before changing code. Treat the file locations as search hints;
line numbers can change. Record a short result if a case is already fixed.

These defects affect the claimed runtime profile. They are not deferred package,
standard-library, or language-surface work. Preserve the recorded history below,
but do not treat it as proof that the current runtime profile has no defects.

### [x] R1 — Encode empty polymorphic values without needless layout demands

Reported locations:

- `crates/nash-codegen/src/demand.rs:533–545,559–565`
- `crates/nash-codegen/src/build.rs:597–603`

Reported reproductions, each used in an isolated validator:

```aiken
let encoded: Data = []
```

```aiken
let encoded: Data = None
```

The review reports that Aiken compiles and evaluates both. Nash fails with:

```text
BuildError: operation needs a concrete runtime layout, got 'erased
```

**Required result**

- Both cases must compile and evaluate with the same encoded result as the pinned
  compiler. The test must use the encoded value so that optimization cannot remove
  the operation being checked.
- Determine how the pinned compiler handles the unconstrained type argument.
  Check the shared demand, specialization, and encoding path before choosing a fix.
- Do not require an element or payload layout when the emitted operation does not
  use it. Keep concrete layout demands where the emitted operation needs them.
- Do not assign a guessed type to every erased variable. Do not special-case the
  source spellings `[]` or `None` in the frontend to bypass the shared rules.
- Retain a concrete `List<Int>` control to check that payload encoding still works.

### [x] R2 — Apply the verified Data-ascription rule to module constants

Reported location:

- `crates/nash-frontend-aiken/src/lower/declarations.rs:78–100`

```aiken
pub const encoded: Data = 1
```

The review reports that Aiken accepts this declaration and a validator can use
its integer `Data` value. Nash reports `nash::type::mismatch`. The constant lowerer
attaches the annotation without the conversion used for local binding ascriptions.

**Required result**

- The declaration must check, build, and produce the same `Data` value as the
  pinned compiler when used by a validator.
- Apply the verified assignment-ascription rule through the shared conversion
  policy. Preserve module-constant initializer restrictions and source regions.
- Retain enough source-site information to distinguish a module constant, a local
  binding, and a function result. Shared code must not erase those distinctions.
- Do not add a constant-only conversion shortcut that remains outside the planner
  when Chunk 6 is complete.

### [x] R3 — Reject implicit function-result encoding where Aiken rejects it

Reported location:

- `crates/nash-frontend-aiken/src/lower/declarations.rs:287–292`

```aiken
pub fn encoded() -> Data { 1 }
```

The review reports that Aiken rejects this with `CouldNotUnify`. Nash inserts an
encoding conversion and emits `iData 1`.

**Required result**

- Reject this declaration during checking with a located type diagnostic. A
  runtime failure is not an equivalent result. Do not emit a new successful build
  artifact for the rejected source.
- Remove the rule that treats a complete `Data` return annotation as permission
  to encode any body value. Verify the exact function-signature rule before the
  replacement is implemented.
- Keep conversions at source sites where the pinned compiler permits them. Do
  not ban `ToData` globally to repair this case.
- Do not infer rules for function arguments, lambdas, partial annotations, or
  nested contexts from this one example. Audit those sites in Chunk 6.

### Repair acceptance and order

Use one small regression group with five logical cases: the empty list, `None`,
the module constant, the rejected function return, and the concrete `List<Int>`
control. Reuse the current differential harness. Keep the rejected source in a
separate case so that it does not prevent the accepted cases from being checked.
Compare source acceptance or rejection first. Evaluate and compare the encoded
values only for accepted cases. Exact diagnostic text does not need to match.

Run the affected suites and the existing pinned differential tests after the
repairs. Preserve native Nash behavior and the recorded shallow, view, and full
validation rules. Do not claim a new full-workspace pass unless it was run.

Resolve R1–R3 before new language or package features. Bring forward the necessary
part of Chunk 6 if a shared conversion rule is needed. Do not create two planners.
Mark each repair complete only after its regression check passes. Record the fix
and the pinned rule briefly in `docs/aiken-frontend.md`; update `SPEC.md` and the
changeset as needed. Then continue through Chunks 1–19 and gates G1–G11. Do not stop
the full assignment after the repairs.

Repair implementation uses one deferred inference-time ascription planner;
decisions run before scope generalization, after initializer/call constraints.
An eager planner incorrectly constrained the concrete-list control's input to
Data before its generated sequential-binding argument was inferred.
Six tests that asserted the old private lowering shape were removed rather than
re-pinned. A compact runtime comparison retains their import/rename, nullary
call, shadowing, short-circuit and builtin behavior coverage and checks lambda
result annotations independently. It also exposed a baseline trace-order defect:
pinned `gen_uplc.rs:4379–4384` emits `>`/`>=` by swapping operands into
`lessThanInteger`/`lessThanEqualsInteger`, evaluating the right operand first.
The old negated comparison preserved truth values but not that trace order.
The adapter now follows the pinned operation; native Nash is unchanged.

Repair verification: `cargo test -p nash-frontend-aiken --test validators`:
17 passed. Affected suites (`cargo test -p nash-source -p nash-ast -p nash-can
-p nash-constrain -p nash-solve -p nash-ir -p nash-codegen
-p nash-frontend-aiken -p nash-driver`): 953 passed, one ignored. This is not a
workspace pass and does not satisfy the remaining source/project gates.
Real CLI `check` and `build` accepted the combined R1/R2/control project and
emitted a 123-byte Flat validator; the isolated R3 project exited 1 for both
commands with a located type mismatch and no build directory.
The old mixed-language layout fixture relied on the rejected implicit function
return; it now uses a legal local Data ascription and its runtime test passes.
At this repair checkpoint, R1–R3 were complete; shared identities and the remaining
source/project chunks still awaited implementation. The final state is recorded above.

## Completed work

The former frontend and runtime plans recorded passes for frontend gates G1–G7,
bounded-validator gates V1–V5, and runtime gates G1–G7. Their overlapping checklists
are consolidated below. Preserve these historical results. The later defects in
R1–R3 qualify the runtime completion claim until repaired. This section does not
claim that the remaining source/project gates pass.

- [x] **Shared frontend and common syntax.** `nash-frontend` and
  `nash-frontend-nash` provide one parser-neutral registry for discovery,
  compilation, CLI and LSP. `Frontend::inspect` retains module-local failures
  without stopping independent graph nodes; `Frontend::parse` returns
  arena-backed `nash_source::Module` with owned diagnostics/inspection. The
  driver converts diagnostics without changing `nash-report`; the native adapter
  retains its syntax-report conversion and validator role metadata. One
  build-wide arena holds source, canonical nodes, evidence and interfaces.
  `ModuleCatalog`/`SourceSpec` replace suffix inference with source-root identity
  and package ownership, exact mixed-language imports and ambiguity diagnostics.
  The exact-pinned `aiken-lang = "=1.1.23"` parser stays behind the Aiken adapter;
  Rust 1.94.1 resolves the former 1.92.0 toolchain mismatch. Functions, constants,
  imports, primitive annotations, patterns, expressions, types, aliases and
  exports lower with source regions and explicit unsupported-feature policies.
  Fixed primitive `Constant` expressions/patterns use Nash's solver and backend
  without native `Literal`/`Eq` evidence; native overloaded literals are unchanged.
  Sequential bindings use strict hygienic lambda applications; nullary functions
  retain the unit-argument ABI. A private concrete profile, upstream sync policy
  and focused coverage matrix replace an unused profile abstraction.
- [x] **Runtime layouts, conversions and integers.** Original Aiken type names
  and frontend-neutral `DataLayout { encoding, tags }` propagate through source,
  canonical unions and interfaces; `DataEncoding::{Constr, List, Transparent}`
  replaces the historical lowercase Term shortcut. Qualified, instantiated
  `AdtRef`/`AdtLayout` identities specialize canonical constructor field types.
  Shared `Expr::Convert` operations (`ToData`, `FromDataShallow`, `ViewData`,
  `ValidateData`) cover recursive primitive, list, pair, tuple and user-type
  encoding/decoding, custom tags, list encoding, zero/multiple fields, substituted
  generics and imported/mixed-language layouts. Annotated `expect` performs full
  validation even for discarded/unused bindings, with located conversion errors.
  Arena-safe arbitrary-precision constants and patterns replace the i128 limit
  without changing native syntax. Native representations remain stable when no
  external layout is present.
- [x] **Validator ABI and backend integration.** The initial mint/optional-else
  or else-only NashV1 profile now extends to all six V3 purposes: mint, spend,
  withdraw, publish, vote and propose, plus fallback. Parameters, optional datum,
  redeemer, purpose and transaction positions use shared conversions and official
  demand-driven context decoding; `False` fails and `True` returns unit.
  Else-only validators retain raw context. Located layout, conversion and
  handler diagnostics replace blanket rejection; `NAF2301` denotes unsupported
  handler sets, boundary annotations or role metadata. The Plan 7 codegen rebase
  preserved the backend finish callback and catalog metadata; Aiken-backed native
  validators reach Core/UPLC and execute in CEK. Production uses Nash semantic
  stages and code generation; the pinned official compiler/CEK is a test-only
  differential oracle, with the driver a dev-dependency for real-path coverage.
- [x] **Regression and end-to-end verification.** Focused tests cover native
  parsing/validator headers, frontend selection and overrides, common lowering,
  fixed primitive typing (including mismatches), scope/nullary behavior, exports,
  mixed imports, exact nested identity, ambiguity, isolated failures and useful
  CRLF/non-ASCII regions. CLI supported/unsupported fixtures and unsaved/standalone
  LSP diagnostics pass. Runtime fixtures cover default/custom/list/transparent
  layouts, nested values, generic cross-module identity collisions, strict
  `expect`, every purpose and native/mixed execution. Eleven pinned official
  differential tests cover accepted/rejected Data, results and user traces,
  including maps, UTF-8, opaque layouts, shallow parameters and optional datum.
  Final recorded validation passed formatting, workspace/all-target check,
  strict workspace/all-feature clippy, `cargo test --workspace` (3,222 passed,
  3 ignored) and `cargo insta test` (no pending snapshots; runner temporarily
  installed in isolation). Real CLI checks/builds exercised the Aiken library,
  custom-layout validator (372 Flat bytes) and native vesting (515/520 Flat bytes),
  emitting UPLC/Flat/CBOR. Temporary projects and generated artifacts were removed.

### Stable runtime decisions and corrections

Preserve these semantics except where a defect is established against the pinned
compiler. The baseline repairs above are required corrections, not permission to
change unrelated validation or evaluation rules:

- Opaque single-field wrappers use explicit Transparent wire encoding. Shallow
  tuple extraction retains encoded fields; Bool/Void/Pair shallow casts
  intentionally accept more than full `expect` validation.
- Optional datum is a typed Option view with nested decoding on demand, preserving
  user trace order. Exhaustive external matches use the official final-case
  default rather than adding tag validation. This is not exhaustive ledger-schema
  validation; full validation remains a separate operation.
- Mint policy patterns decode to ByteArray despite the upstream internal prelude's
  Data annotation. Policy decoding is mandatory even when discarded: missing
  fields and non-byte policy Data fail before redeemer decoding. All-six-purpose
  dispatch rejects an explicit redundant fallback.
- Negative CBOR bignums use minus-one magnitude; the shared runtime bug is fixed.
  A native depth-limit stack overflow was traced with LLDB to nested iterator
  frames in `TypeEnv::instance`; an explicit argument loop restored the limit
  without enlarging the stack or lowering the limit.
- Native AST snapshots now carry explicit None/false metadata. Meaningful source
  and canonical snapshots were refreshed; passthrough-only interface tests were
  removed and constructor privacy uses a focused assertion. Shared conversions
  have real interface callers, no dummy success or silent fallback, and normal
  user errors do not panic.

### Historical verification context

The original solved-interface-only `main` baseline was rebased onto the Plan 7
codegen branch, then `origin/release/main` at `f2b3e766`, preserving release versions
and frontend dependencies. Work was restored unstaged; the backup branch
`backup/aiken-frontend-before-release-main-20260914` and named stash were retained
at that time. Earlier workspace passes recorded 3,211 and then 3,215 tests passed
(3 ignored); the later 3,222-test runtime pass above supersedes those counts.
An Aiken-backed native validator accepted 41 and rejected 40 (51 Flat bytes);
the bounded mint fixture built to 138 Flat bytes with user traces. The unsupported
fixture exited 1 with located `NAF2201` and no artifacts. Integer overflow
rejection belonged to the superseded i128 profile, not the current contract.

Concrete fixtures live under `crates/nash-driver/tests/fixtures/aiken/`, including
`supported/src/add_one.ak` and `unsupported/src/unsupported.ak`. The latter's Aiken
test declaration was deliberately unsupported by the bounded profile; accepting
and type-checking these declarations is remaining work below. Historical CLI and
rebase checks alone did not establish ABI conformance; pinned runtime comparisons
do. No external blocker remained for the completed milestones.

## Goal

An unmodified Aiken 1.1.23 project can use the Nash compiler to check and build
its production code.

The supported project can contain:

- Library modules.
- Validator modules.
- All Aiken 1.1.23 production language forms.
- The official Aiken prelude and builtin surface.
- The official Aiken standard library.
- Direct and transitive package dependencies.
- `aiken.toml` and `aiken.lock`.
- Aiken workspaces.
- Environment modules.
- The synthetic `config` module.
- More than one named validator in one source module.

For in-scope Aiken source, Nash must match the pinned compiler's source acceptance
and rejection rules. A program that Aiken rejects must not become valid only
because Nash adds an implicit conversion. Native Nash source keeps its own rules.

For source accepted by both compilers, the same source must have equivalent
observable on-chain behavior when Aiken 1.1.23 and Nash compile it. Compare the same
input under matching target, environment, parameter, and trace settings.

Observable behavior means:

- The same boundary `Data` has the same observable outcome, including malformed
  or partly decoded data.
- Validators agree on success and failure.
- Successful evaluations return equivalent values.
- Relevant user traces have the same order and content, including before failure.

Do not require all malformed `Data` to fail at entry. Do not require all valid
`Data` to succeed when the validator's own rules reject it. Match the pinned Aiken
behavior. Preserve shallow extraction, demand-driven decoding, full `expect`
validation, and failure order. Do not add validation where Aiken does not perform
it.

The following results do not have to match:

- UPLC bytes.
- Script hashes.
- Execution budgets.
- Optimization choices.
- Diagnostic text.

This plan completes **Aiken source and project compatibility**. It does not
provide full Aiken command-line tool parity.

## Product boundary

### Included

- All Aiken 1.1.23 definition, annotation, expression, pattern, assignment, and
  module variants.
- All type-directed syntax and implicit conversions that Aiken 1.1.23 uses.
- Aiken prelude behavior.
- Complete Aiken 1.1.23 builtin classification and lowering.
- Official Aiken standard-library source without local edits.
- Aiken manifest, lock, dependency, cache, workspace, environment, and
  configuration behavior needed by `check` and `build`.
- Separate build entry points for all named validators.
- Metadata that a later Nash artifact layer needs for CIP-57 output.
- Parse and type-check support for test and benchmark declarations.

### Not included

- CIP-57 or `plutus.json` serialization.
- Parameter application to an existing blueprint.
- Test execution.
- Benchmark execution.
- Source formatting.
- HTML documentation generation.
- Editor completion.
- Exact Aiken diagnostic text.
- Exact UPLC, Flat, CBOR, hash, or budget parity.

Valid test and benchmark declarations must parse and type-check. Invalid
bodies and signatures must produce normal diagnostics. These declarations do not
run or become production entry points in this plan. Follow the dependency rules
in Chunk 9 for declarations outside the root project.

Documentation comments must survive as metadata. Nash does not generate Aiken
HTML documentation in this plan.

## Upstream reference

Use the exact Aiken tag `v1.1.23` as the semantic reference.

Primary reference files:

- `crates/aiken-lang/src/ast.rs`
- `crates/aiken-lang/src/expr.rs`
- `crates/aiken-lang/src/builtins.rs`
- `crates/aiken-lang/src/tipo/infer.rs`
- `crates/aiken-lang/src/gen_uplc.rs`
- `crates/aiken-project/src/config.rs`
- `crates/aiken-project/src/deps.rs`
- `crates/aiken-project/src/paths.rs`
- `crates/aiken-project/src/lib.rs`

Production Nash code can use the official Aiken parser and project-model code.
It must not call the Aiken type checker or code generator.

## Remaining source/project completion gates

### G1 — Exact language inventory

- [x] Every Aiken 1.1.23 definition variant has an explicit compatibility
  classification.
- [x] Every Aiken 1.1.23 annotation variant has an explicit compatibility
  classification.
- [x] Every Aiken 1.1.23 expression variant has an explicit compatibility
  classification.
- [x] Every Aiken 1.1.23 pattern variant has an explicit compatibility
  classification.
- [x] Every assignment and argument form has an explicit compatibility
  classification.
- [x] Every module kind has an explicit project and compiler rule.
- [x] Production lowering has no wildcard branch that returns a general
  unsupported-language diagnostic.
- [x] An Aiken dependency update that adds an AST variant causes a compile-time
  match failure or a focused coverage-test failure.

### G2 — Complete production language surface

- [x] All production Aiken 1.1.23 syntax reaches Nash canonicalization and type
  solving.
- [x] Type-dependent syntax is not guessed by the untyped frontend.
- [x] Evaluation order matches Aiken for pipelines, assignments, calls, traces,
  record updates, and logical chains.
- [x] Valid test and benchmark declarations parse and type-check but do not
  become production validator roots. Invalid bodies and signatures produce
  normal diagnostics.
- [x] `NAF2201` is not used for a valid Aiken 1.1.23 production language form.

### G3 — Type-directed operations and conversions

- [x] One compiler operation owns all implicit conversion decisions.
- [x] Each conversion has a documented source site and source region.
- [x] Baseline repairs R1–R3 pass their focused regression checks.
- [x] Local binding, module-constant, and function-result ascriptions follow
  their verified acceptance and rejection rules. A `Data` target alone does not
  authorize encoding.
- [x] Layout demands match the emitted operation. Empty polymorphic values work
  without guessed type defaults or the removal of required payload layout checks.
- [x] Shallow extraction, view conversion, and full validation remain distinct.
- [x] Polymorphic equality and inequality select Aiken operations from solved
  types. They do not use user Nash trait instances.
- [x] Labeled calls, tuple indexing, record updates, trace arguments, and
  pipelines use resolved types or declarations.
- [x] Every compatibility conversion follows a rule verified against the
  pinned Aiken behavior, with a defined validation depth. No arbitrary cast or
  added full validation replaces a required shallow or view operation.

### G4 — Aiken prelude and builtins

- [x] `.ak` modules use the Aiken prelude. They do not receive Nash default
  imports.
- [x] Native `.nash` modules keep their current default imports and behavior.
- [x] Every builtin exposed by Aiken 1.1.23 has one explicit classification.
- [x] Each builtin either maps to a Nash builtin, uses a defined lowering, or
  returns a precise Plutus-version diagnostic.
- [x] The builtin table has a focused completeness test.

### G5 — Official standard library

- [x] One exact official Aiken standard-library release is pinned in the
  acceptance fixture.
- [x] Its source checks through Nash without source edits.
- [x] The package enters through the normal Aiken package path.
- [x] Nash does not replace the standard library with `nash/core` shims.
- [x] No module-name-specific exception exists for a standard-library file.

### G6 — Aiken project loading

- [x] `nash check <path>` finds the nearest `aiken.toml` when no nearer
  `nash.jsonc` owns the path.
- [x] A direct manifest path selects the correct project loader.
- [x] A directory that contains both manifest kinds gives a clear ambiguity
  diagnostic unless the caller selects one format explicitly.
- [x] Root `lib/`, `validators/`, and `env/` source roots use Aiken naming rules.
- [x] The selected configuration creates a synthetic `config` module.
- [x] The selected environment follows Aiken `default` and `--env` behavior.
- [x] Aiken workspace members load with stable package ownership.

### G7 — Package and lock resolution

- [x] Direct and transitive dependencies resolve from `aiken.toml`.
- [x] `aiken.lock` is read and written with the selected Aiken 1.1.23 behavior.
- [x] The normal Aiken package cache and build dependency paths work.
- [x] CI tests use local fixtures or a prepared cache. They do not need network
  access.
- [x] A dependency package contributes only its library surface to production
  compilation.
- [x] Duplicate or ambiguous module providers return a clear located project
  diagnostic.
- [x] Package identity is part of compiler-owned type, layout, decoder, entry
  point, and artifact identities.

### G8 — Multiple validators and artifact metadata

- [x] One source module can define more than one named validator.
- [x] Each validator becomes a separate compiler entry point and build artifact.
- [x] Native Nash validator modules still use their current `main` model.
- [x] Validator name, module, package, documentation, parameter order, parameter
  labels, handlers, datum, redeemer, solved types, and layout identities survive
  the build.
- [x] Project title, version, license, description, repository, compiler target,
  and Plutus version survive as project metadata.
- [x] This plan does not serialize a blueprint.

### G9 — CLI, diagnostics, and editor safety

- [x] `nash check` and `nash build` accept an Aiken project root without a Nash
  configuration file.
- [x] `nash build` emits one UPLC, Flat, and CBOR set for each Aiken validator
  entry point.
- [x] File names are stable and contain enough identity to avoid collisions.
- [x] Source and package errors have useful regions and file paths.
- [x] Existing `.ak` LSP diagnostics continue to use the shared project and
  frontend paths.
- [x] This plan adds no formatter, documentation generator, or completion
  engine.

### G10 — Native Nash regression safety

- [x] Native Nash projects keep their current project, import, type, validator,
  and build behavior.
- [x] Native script sizes do not change without a recorded reason.
- [x] Mixed Nash and Aiken graphs still compile where the public types are
  compatible.
- [x] No Aiken parser or project type enters Nash canonicalization, solving, IR,
  or code generation.

### G11 — Final compatibility acceptance

- [x] One unmodified Aiken project with the official standard library checks
  through Nash.
- [x] One unmodified Aiken project with a direct and a transitive dependency
  checks through Nash from a prepared local cache.
- [x] One project uses `env`, synthetic `config`, tests, benchmarks, and more
  than one validator.
- [x] The representative validators build and execute with equivalent outcomes
  under Aiken 1.1.23 and Nash.
- [x] All required workspace validation commands pass.
- [x] `docs/aiken-frontend.md`, `SPEC.md`, and changesets describe the final
  supported claim and the remaining tool-only exclusions.

## Architecture

The Rust snippets below are design examples, not required signatures. Check the
current APIs and arena ownership before changing them. Reuse working code and
choose the smallest sound interface. Do not add unused types, traits, or stubs
only to copy a snippet. Keep the required behavior and dependency boundaries.

### Dependency direction

```text
Aiken project files
  -> nash-project-aiken
       -> exact-pinned Aiken config/dependency/path APIs
       -> Nash-owned LoadedProject and ModuleCatalog

Aiken source
  -> nash-frontend-aiken
       -> exact-pinned Aiken parser
       -> nash_source::Module + source entry-point metadata

Nash source
  -> nash-frontend-nash
       -> nash_source::Module + native entry-point metadata

Both paths
  -> nash-driver
       -> nash-can
       -> nash-solve
       -> nash-nitpick
       -> nash-codegen
       -> nash-plutus
```

Rules:

- `nash-project-aiken` can depend on `aiken-project` and `aiken-lang`.
- `nash-frontend-aiken` can depend on `aiken-lang`.
- No other production compiler crate can depend on Aiken crates.
- `nash-project-aiken` must not call `aiken_project::Project::check`, `build`,
  `type_check`, or Aiken code generation.
- Aiken project and AST types must be converted to Nash-owned types at the crate
  boundary.

Use `aiken-project = "=1.1.23"` when that crate is available from the selected
registry. Otherwise, use an exact Git tag or commit for `v1.1.23`. Do not use a
moving branch.

### Shared contract ownership

Shared project, package, module, import, and diagnostic contracts must sit below
both the project adapters and the driver. This includes the common types needed
by `ProjectLoader`, `LoadedProject`, `SourceSpec`, and `ModuleCatalog`.

The driver can depend on the adapters and the shared contracts. The adapters can
depend on the shared contracts. They must not depend on the driver. The shared
contracts must not depend on either adapter or on the driver.

Use an existing lower-level crate when it fits. Add a small shared crate only
when needed. Keep driver-specific compilation state in the driver. Do not create
a dependency cycle between `nash-project-aiken` and `nash-driver`.

### New crate

Add:

```text
crates/nash-project-aiken/
```

Responsibilities:

- Find and load `aiken.toml`.
- Distinguish project and workspace manifests.
- Normalize Aiken package identity.
- Resolve the lock file and dependency set.
- Find package cache and build dependency paths.
- Discover `lib/`, `validators/`, and `env/` modules.
- Create the synthetic `config` module.
- Select the active environment.
- Produce Nash-owned project metadata and source specifications.

It does not parse function bodies. It does not type-check source.

### Project-loader seam

Add a small project-loader contract. It must have two real implementations:
Nash and Aiken.

```rust
pub trait ProjectLoader: Send + Sync {
    fn descriptor(&self) -> &'static ProjectLoaderDescriptor;

    fn locate(
        &self,
        start: &Path,
    ) -> Result<Option<ProjectLocation>, ProjectDiagnostic>;

    fn load(
        &self,
        request: ProjectLoadRequest<'_>,
    ) -> Result<LoadedProject, Vec<ProjectDiagnostic>>;
}
```

```rust
pub struct ProjectLoaderDescriptor {
    pub id: &'static str,
    pub manifests: &'static [&'static str],
}

pub struct ProjectLoadRequest<'a> {
    pub location: &'a ProjectLocation,
    pub selected_environment: Option<&'a str>,
    pub mode: ProjectMode,
    pub package_store: &'a dyn PackageStore,
}

pub enum ProjectMode {
    Check,
    Build,
    Editor,
}
```

`ProjectLoader::load` is unimplemented at the start of this plan.

Purpose: load one manifest format into the common driver model.

Example: `AikenProjectLoader` loads `aiken.toml`, selects `env/default.ak`, and
returns source specifications for `lib/`, `validators/`, dependencies, and the
synthetic `config` module.

### Composition for project loading

The Aiken loader must be an orchestration function. Keep manifest parsing,
dependency resolution, source discovery, configuration synthesis, and catalog
assembly in separate functions.

```rust
pub fn load_aiken_project(
    request: ProjectLoadRequest<'_>,
) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    let manifest = manifest::load(request.location)?;
    let lock = lock::load_if_present(request.location)?;
    let packages = dependencies::resolve(
        &manifest,
        lock.as_ref(),
        request.package_store,
    )?;
    let environment = environment::select(
        &manifest,
        request.selected_environment,
    )?;
    let sources = discovery::discover(
        request.location,
        &manifest,
        &packages,
        &environment,
        request.mode,
    )?;
    let synthetic = config_module::build(
        &manifest,
        &environment,
    )?;

    project::assemble(
        manifest,
        lock,
        packages,
        environment,
        sources,
        synthetic,
    )
}
```

### Package store

Network and cache access must sit behind one interface.

```rust
pub trait PackageStore: Send + Sync {
    fn resolve(
        &self,
        request: PackageResolutionRequest<'_>,
    ) -> Result<ResolvedPackageSet, Vec<ProjectDiagnostic>>;
}
```

The production implementation can wrap the exact-pinned Aiken dependency code.
The test implementation uses fixture directories.

`PackageStore::resolve` is unimplemented at the start of this plan.

Purpose: produce the complete locked dependency set and local package roots.

Example: resolve `aiken-lang/stdlib` and one transitive package from a prepared
cache without a network request.

### Package and module identity

Add compiler-owned identity types. Do not use a file URI or a source spelling as
semantic identity.

```rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageId {
    pub name: PackageName,
    pub version: PackageVersion,
    pub source: PackageSourceId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleKey {
    pub package: PackageId,
    pub module: ModuleName,
}
```

The root application also gets a stable package identity.

`SourceSpec` must contain `ModuleKey`. The URI remains the source location.

```rust
pub struct SourceSpec {
    pub key: ModuleKey,
    pub uri: Url,
    pub source_root: PathBuf,
    pub role: Option<ModuleRole>,
    pub frontend: Option<String>,
    pub origin: SourceOrigin,
}
```

`SourceOrigin` distinguishes root project source, dependency source, compiler
source, and synthetic source.

The source import remains a module name. Project resolution selects one provider.

```rust
pub struct ResolvedDependency {
    pub requested: ModuleName,
    pub provider: ModuleKey,
    pub uri: Url,
    pub region: Region,
}
```

Aiken source cannot select a package in an import. If more than one visible
package provides the same module, report an ambiguity. Do not select one by
filesystem order.

### Interface lookup

Replace interface maps that use only module text with package-aware keys.

```rust
pub struct InterfaceStore<'a> {
    by_module: BTreeMap<ModuleKeyRef<'a>, Interface<'a>>,
}
```

Canonicalization must receive the resolved import map for the module being
compiled.

```rust
pub struct CanonicalContext<'a> {
    pub home: ModuleKeyRef<'a>,
    pub imports: &'a ResolvedImportMap<'a>,
    pub interfaces: &'a InterfaceStore<'a>,
}
```

This change must also update:

- `AdtRef`.
- Specialized layout keys.
- Decoder identities.
- Constructor identities.
- Definition identities.
- Entry-point identities.
- Artifact metadata.

Aiken still reports duplicate visible module names as an error. Package-aware
identity prevents internal collisions and gives stable metadata.

### Language compatibility classification

Use one explicit classification for each exact-pinned Aiken AST variant.

```rust
pub enum CompatibilityClass {
    LowerDirectly,
    Desugar,
    TypeDirected,
    ToolDeclaration,
}
```

Do not add `Unsupported` for an Aiken 1.1.23 production language form.

The classification table belongs in `docs/aiken-frontend.md`. The implementation
must use exhaustive matches over the pinned enums.

### Type-directed source nodes

The frontend can desugar syntax only when no solved type or resolved declaration
is needed.

Add or retain frontend-neutral source nodes for forms such as:

```rust
pub enum Expr<'a> {
    // Existing variants.

    Pipeline {
        input: &'a Located<Expr<'a>>,
        stages: &'a [&'a Located<Expr<'a>>],
    },

    LabeledCall {
        function: &'a Located<Expr<'a>>,
        arguments: &'a [&'a CallArgument<'a>],
    },

    TupleIndex {
        tuple: &'a Located<Expr<'a>>,
        index: usize,
    },

    RecordUpdate {
        record: &'a Located<Expr<'a>>,
        fields: &'a [&'a RecordUpdateField<'a>],
    },

    PolymorphicEquality {
        operation: EqualityOperation,
        left: &'a Located<Expr<'a>>,
        right: &'a Located<Expr<'a>>,
    },

    TraceValues {
        label: &'a Located<Expr<'a>>,
        arguments: &'a [&'a Located<Expr<'a>>],
        then: &'a Located<Expr<'a>>,
    },
}
```

The exact enum shape can differ. The required property is that the Aiken
frontend does not guess arity, parameter labels, record identity, tuple arity,
equality operation, or trace rendering from untyped syntax.

### Function labels

Preserve declaration labels in local definitions and interfaces.

```rust
pub struct ParameterShape<'a> {
    pub label: Option<&'a str>,
    pub binding: Option<&'a str>,
    pub position: usize,
}

pub struct CallableShape<'a> {
    pub parameters: &'a [ParameterShape<'a>],
}
```

Canonicalization resolves a labeled call against `CallableShape`. It then places
arguments in declaration order.

A labeled call to a value with no known callable shape must produce a clear type
or name error. It must not fall back to source order.

### Conversion planner

Use one frontend-neutral conversion planner. Preserve source-site distinctions
until the rule is selected. A shared implementation does not mean that all sites
permit the same source and target types. A rejected conversion returns a normal
checking diagnostic through `Result`; it must not become `Identity` or `Apply`.

The example below gives module constants their own site. Equivalent context is
acceptable. Separate `let` and `expect` when their validation rules differ. The
presence of `FunctionResult` names a context; it does not authorize a conversion.

```rust
pub enum ConversionSite {
    AnnotatedBinding,
    ModuleConstant,
    PatternBinding,
    ValidatorParameter,
    ValidatorDatum,
    ValidatorRedeemer,
    ValidatorPurpose,
    ConfigValue,
    FunctionArgument,
    FunctionResult,
}

pub struct ConversionRequest<'a> {
    pub site: ConversionSite,
    pub source: SolvedTypeRef<'a>,
    pub target: SolvedTypeRef<'a>,
    pub region: Region,
}

pub enum ConversionPlan {
    Identity,
    Apply(ConvertKind),
}

pub fn plan_conversion(
    request: ConversionRequest<'_>,
    layouts: &LayoutStore<'_>,
) -> Result<ConversionPlan, ConversionDiagnostic>;
```

`plan_conversion` is unimplemented at the start of this plan.

Purpose: select a legal conversion, or reject it, for a source site and the solved
source and target types.

Example: an annotated `expect value: MyRecord = raw_data` selects
`ValidateData`, while a validator parameter can select `FromDataShallow`.

Every generated `Expr::Convert` must have one planner result or one verified
explicit source construct from the runtime baseline. A lowerer-added conversion
is not an explicit user operation. Correct R2 and R3 before treating any existing
ascription path as a rule to preserve.

The matrix must include the reported differences between a local binding, a
module constant, and a function return. Record both accepted and rejected cases.

### Aiken compiler environment

Aiken source uses a compiler-owned prelude and builtin environment.

Do not give Aiken modules Nash default imports.

The implementation can use synthetic source modules or compiler-owned interface
construction. It must expose the exact Aiken 1.1.23 surface needed by user code
and the official standard library.

```rust
pub trait CompilerEnvironment: Send + Sync {
    fn language(&self) -> &'static str;

    fn compiler_modules(
        &self,
    ) -> Result<Vec<CompilerModule>, CompilerEnvironmentDiagnostic>;

    fn default_imports(
        &self,
        module: &ModuleKey,
    ) -> Vec<DefaultImport>;
}
```

`compiler_modules` is unimplemented for Aiken at the start of this plan.

Purpose: add the Aiken prelude and builtin interfaces before project modules.

Example: an Aiken module can use `Option`, `Some`, `None`, `Bool`, `True`, and
`False` without a Nash `Prelude` import.

### Builtin mapping

Replace the bounded whitelist with one exhaustive table.

```rust
pub enum AikenBuiltinLowering {
    NashBuiltin(BuiltinId),
    FrontendDesugaring(AikenBuiltinDesugaring),
    InvalidForPlutusVersion {
        minimum: PlutusVersion,
    },
}

pub fn classify_builtin(
    name: &str,
    plutus: PlutusVersion,
) -> Result<AikenBuiltinLowering, FrontendDiagnostic>;
```

`classify_builtin` is unimplemented for names outside the current bounded builtin table.

Purpose: map each Aiken 1.1.23 builtin by exact name and version.

Example: `equals_integer` maps to the Nash integer equality builtin, while a
builtin that is not valid for the selected target returns a version diagnostic.

### Tests and benchmarks as tool declarations

Add frontend-neutral metadata for declarations that must type-check but must not
enter production code generation.

```rust
pub enum RunnableKind {
    Test,
    Benchmark,
}

pub struct RunnableDecl<'a> {
    pub kind: RunnableKind,
    pub name: &'a str,
    pub function: DefId,
    pub region: Region,
}
```

The compiler checks their bodies and signatures. `nash build` does not make them
entry points. A later tool plan can run them.

### Validator entry points

A module can have zero, one, or many compiler entry points.

```rust
pub struct EntryPointId {
    pub module: ModuleKey,
    pub name: String,
}

pub enum EntryPointKind {
    NativeValidator,
    AikenValidator,
}

pub struct SourceEntryPoint<'a> {
    pub id: EntryPointId,
    pub kind: EntryPointKind,
    pub function_name: &'a str,
    pub metadata: SourceValidatorMetadata<'a>,
}
```

After solving:

```rust
pub struct SolvedEntryPoint<'a> {
    pub id: EntryPointId,
    pub function: DefId,
    pub parameters: Vec<SolvedBoundaryBinding<'a>>,
    pub datum: Option<SolvedBoundaryBinding<'a>>,
    pub redeemer: Option<SolvedBoundaryBinding<'a>>,
    pub handlers: Vec<SolvedHandlerMetadata<'a>>,
    pub docs: Option<String>,
}
```

Entry-point metadata is not a blueprint. It is the stable input for a later Nash
artifact plan.

The build composition becomes:

```rust
pub fn compile_entry_points(
    solved: &SolvedProject<'_>,
) -> Result<Vec<CompiledArtifact>, Vec<BuildDiagnostic>> {
    solved
        .entry_points()
        .map(|entry| compile_entry_point(solved, entry))
        .collect()
}
```

`compile_entry_point` is unimplemented for multiple Aiken validators at the start
of this plan.

Purpose: compile each named validator as an independent script.

Example: `validator alpha` and `validator beta` in one `.ak` module produce two
artifact sets with distinct names.

## Exact language inventory

The first implementation change must create and check this inventory against the
exact-pinned Aiken enums.

### Definitions

- `Fn`
- `TypeAlias`
- `DataType`
- `Use`
- `ModuleConstant`
- `Test`
- `Benchmark`
- `Validator`

### Annotations

- `Constructor`
- `Fn`
- `Var`
- `Hole`
- `Tuple`
- `Pair`

### Expressions

- `UInt`
- `String`
- `Sequence`
- `Var`
- `Fn`
- `List`
- `Call`
- `BinOp`
- `ByteArray`
- `CurvePoint`
- `PipeLine`
- `Assignment`
- `Trace`
- `TraceIfFalse`
- `When`
- `If`
- `FieldAccess`
- `Tuple`
- `Pair`
- `TupleIndex`
- `ErrorTerm`
- `RecordUpdate`
- `UnOp`
- `LogicalOpChain`

### Patterns

- `Int`
- `ByteArray`
- `Var`
- `Assign`
- `Discard`
- `List`
- `Constructor`
- `Pair`
- `Tuple`

Constructor patterns must include:

- Positional arguments.
- Labeled arguments.
- Renamed bindings.
- Spread.
- Module qualification.
- Type qualification.

### Assignments and arguments

- `is` alternatives.
- `let`.
- `expect`.
- Backpassing `let`.
- Backpassing `expect`.
- More than one assignment pattern.
- Named arguments.
- Discarded arguments.
- Pattern arguments.
- Labeled parameters with a different local binding name.
- `via` arguments on tests and benchmarks.

### Module kinds

- `Lib`
- `Validator`
- `Env`
- `Config`

## Remaining work sequence

Resolve the required baseline repairs R1–R3 first. Each chunk below remains
pending unless its full done criteria pass. The G1–G11 checklist records the shared
acceptance requirements. Reuse the repaired conversion path in Chunk 6; do not
implement it a second time.

Chunk numbers are tracking identifiers, not a strict execution order. Use the
order required by dependencies. Keep the existing numbers so references remain
stable.

Define the shared contracts and the package-aware identities from Chunk 14 before
the compiler environment, project loaders, or dependency resolver need them.
Complete their use in interfaces, layouts, decoders, definitions, and entry points
before final package, workspace, and multi-validator acceptance. Do not make
Chunk 13 depend on identity types that are still only planned.

Use the pinned standard library early to find language gaps. Its final acceptance
must still use the normal project and package path without source changes.
Verify existing support before adding it again.

## [x] Chunk 1 — Freeze the compatibility matrix

**Files**

- `docs/aiken-frontend.md`
- `crates/nash-frontend-aiken/src/validate.rs`
- `crates/nash-frontend-aiken/src/lower/*`
- Focused adapter tests

**Change**

- Add the exact inventory above to the architecture document.
- Mark each form as direct lowering, desugaring, type-directed lowering, or tool
  declaration.
- Remove general wildcard fallback from production lowering.
- Add a focused completeness test or exact exhaustive match for each pinned enum.
- Record the exact Aiken crate version and update procedure.

**Tests**

- One test proves that every builtin table entry has a classification.
- One test proves that every compatibility-matrix row has an implementation
  route.
- Do not generate one fixture for each private helper.

**Done when**

The architecture document and code have the same exhaustive inventory. A new
upstream AST variant cannot enter silently.

---

## [x] Chunk 2 — Add type-directed surface nodes

**Files**

- `crates/nash-source`
- `crates/nash-ast`
- `crates/nash-can`
- `crates/nash-constrain`
- `crates/nash-solve`
- `crates/nash-codegen`
- `docs/aiken-frontend.md`
- `SPEC.md`

**Change**

Add only the neutral nodes that the remaining Aiken forms require. Reuse native
Nash nodes when they already preserve the required semantics.

The required type-directed cases include:

- Pipelines with inferred call arity.
- Labeled function calls.
- Renamed parameter labels.
- Tuple indexing.
- Record updates.
- Polymorphic equality and inequality.
- Trace formatting arguments.
- Qualified constructor resolution.

Keep the nodes neutral. Do not name them `Aiken*` in common compiler crates.

**Tests**

- One canonicalization test for labeled call ordering.
- One type test for an invalid tuple index.
- One type test for an invalid record update.
- One codegen test for a solved polymorphic equality operation.

**Done when**

The Aiken lowerer can preserve each type-directed form without guessing its
resolved declaration or type.

---

## [x] Chunk 3 — Complete annotations, parameters, and calls

**Files**

- `crates/nash-frontend-aiken/src/lower/annotation.rs`
- `crates/nash-frontend-aiken/src/lower/declaration.rs`
- `crates/nash-frontend-aiken/src/lower/expression.rs`
- Common callable-shape and interface code

**Change**

Implement:

- Type holes.
- Partial annotations.
- Polymorphic local annotations.
- Pair annotations.
- Pattern function parameters.
- Labeled parameters.
- Renamed local parameter bindings.
- Labeled function calls.
- Labeled constructor calls.
- Correct argument reordering and duplicate-label errors.

A type hole creates a fresh inference variable. It does not create a runtime
conversion.

Preserve callable labels in imported interfaces.

**Tests**

- One local labeled call.
- One imported labeled call.
- One hole that infers successfully.
- One duplicate or unknown label diagnostic.

**Done when**

The official standard library no longer fails because of annotation or call-label
syntax.

---

## [x] Chunk 4 — Complete expressions

**Files**

- `crates/nash-frontend-aiken/src/lower/expression.rs`
- `crates/nash-source`
- `crates/nash-ast`
- `crates/nash-can`
- `crates/nash-codegen`

**Change**

Implement the remaining expression forms:

- Pipelines.
- Pair construction.
- Tuple indexing.
- Record updates.
- Curve-point literals.
- Trace formatting.
- `trace_if_false`.
- Logical operation chains.
- Any remaining unary or binary form.

Preserve source evaluation order.

A curve literal must become the correct primitive constant. Do not convert it to
an unchecked byte array.

A pipeline stage must use the solved call shape. Do not assume that the piped
value is always the first source argument.

**Tests**

Use one compact source fixture that covers the expression forms. Add separate
runtime tests only for forms with evaluation-order or representation risk.

**Done when**

Every `UntypedExpr` variant in Aiken 1.1.23 has a working route and no valid form
returns `NAF2201`.

---

## [x] Chunk 5 — Complete patterns and assignments

**Files**

- `crates/nash-frontend-aiken/src/lower/pattern.rs`
- `crates/nash-frontend-aiken/src/lower/expression.rs`
- `crates/nash-source`
- `crates/nash-ast`
- `crates/nash-can`
- `crates/nash-nitpick`
- Pattern compilation in codegen

**Change**

Implement:

- Pair patterns.
- Labeled constructor patterns.
- Renamed labeled bindings.
- Spread constructor patterns.
- Module-qualified constructor patterns.
- Type-qualified constructor patterns.
- Pattern function arguments.
- Multi-pattern assignment.
- Backpassing `let`.
- Backpassing `expect`.

Desugar only when the transformation does not need solved types. Otherwise,
preserve a neutral node until canonicalization or solving.

Backpassing and multi-pattern lowering must evaluate the source value once.

**Tests**

- One pair-pattern match.
- One labeled spread pattern.
- One qualified imported constructor pattern.
- One multi-pattern assignment that proves single evaluation.
- One backpassing `expect` failure.

**Done when**

Every Aiken 1.1.23 pattern and assignment form type-checks and reaches existing
pattern compilation.

---

## [x] Chunk 6 — Centralize implicit conversions

**Files**

- The completed runtime conversion representation
- `crates/nash-constrain` or the current solved-type elaboration location
- `crates/nash-codegen`
- `docs/aiken-frontend.md`
- `SPEC.md`

**Change**

- Add `ConversionSite`, `ConversionRequest`, and `ConversionPlan`, or equivalent
  types.
- Route all implicit Aiken compatibility conversions through one planner.
- Keep explicit `expect` validation and validator boundary behavior from the completed runtime baseline.
- Add rules found by the exact Aiken 1.1.23 type-inference and code-generation
  audit.
- Reject source-site and type combinations that Aiken rejects. The same pair of
  source and target types can be legal at one site and illegal at another.
- Preserve the R1–R3 fixes and use their regression cases for this chunk. Verify
  constant assignments and function-signature checking as separate source sites.
- Preserve qualified specialized layout identity in every conversion.

Create a conversion matrix in the architecture document. It must state:

- Source type class.
- Target type class.
- Source site, including local bindings, module constants, and function results.
- Whether the source is accepted or rejected.
- Conversion kind when accepted.
- Required layout information, including unresolved empty payload types.
- Validation depth.
- Failure stage and behavior.

**Tests**

Use a small matrix test for legal and illegal source-site conversion plans. Reuse
the R1–R3 tests. Keep the existing runtime-baseline differential tests. A successful
build alone does not prove that a source form should have been accepted.

**Done when**

Every implicit conversion has one documented, source-site-specific planner rule.
Rejected sites produce checking diagnostics. No lowerer contains a private
conversion decision. R1–R3 still pass after the shared policy is complete.

---

## [x] Chunk 7 — Complete polymorphic operations and traces

**Files**

- `crates/nash-constrain`
- `crates/nash-solve`
- `crates/nash-codegen`
- `crates/nash-frontend-aiken`

**Change**

- Implement Aiken `==` and `!=` from solved operand types.
- Use Aiken's allowed type set and runtime operations.
- Do not use Nash user `Eq` implementations.
- Implement trace label and argument rendering with Aiken evaluation order.
- Implement `trace_if_false` with one evaluation of the condition.

**Tests**

- Equality for representative primitive and `Data` cases.
- One rejected equality type.
- One trace with more than one argument.
- One `trace_if_false` success and failure case.

**Done when**

Representative programs have the same result and relevant trace order under
Aiken 1.1.23 and Nash.

---

## [x] Chunk 8 — Add the Aiken compiler environment

**Files**

- `crates/nash-frontend-aiken`
- Driver compiler-module registration
- Interface construction
- `docs/aiken-frontend.md`

**Change**

- Provide the Aiken prelude types, constructors, values, and function shapes.
- Provide the complete `aiken/builtin` surface.
- Remove the bounded frontend builtin whitelist.
- Prevent Nash default imports from entering `.ak` modules.
- Keep Nash defaults unchanged for `.nash` modules.
- Add exact package and module identity for compiler-owned modules.

The implementation can build synthetic source modules or direct interfaces. Use
one method consistently.

**Tests**

- One Aiken module uses prelude names without imports.
- One module imports representative builtin groups.
- One native module still uses the Nash prelude.
- One exhaustive builtin-classification test.

**Done when**

The official Aiken standard library reaches normal project type checking without
missing prelude or builtin definitions.

---

## [x] Chunk 9 — Accept tests and benchmarks without runners

**Files**

- `crates/nash-source`
- `crates/nash-ast`
- `crates/nash-can`
- `crates/nash-constrain`
- `crates/nash-frontend-aiken`
- `crates/nash-driver`

**Change**

- Lower Aiken `test` and `bench` declarations to tool declarations.
- Type-check their arguments, `via` generators, bodies, and result types.
- Do not add them to production entry points.
- Do not emit them in validator artifacts.
- For dependency packages, follow Aiken behavior and omit dependency tests,
  benchmarks, and validators from the imported production surface.

**Tests**

- One project with a test and benchmark passes `nash check`.
- The same project produces no extra build artifact.
- One invalid test body still returns a normal type diagnostic.

**Done when**

A project does not fail only because it contains valid Aiken tests or benchmarks.

---

## [x] Chunk 10 — Compile the official standard library from a local source tree

**Files**

- A pinned test fixture or prepared package cache
- `crates/nash-frontend-aiken` fixes found by the gate
- Relevant common compiler crates

**Change**

- Select one exact official Aiken standard-library release.
- Record its version and source checksum in the fixture README.
- Check all library modules through Nash without source changes.
- Fix general language, prelude, builtin, representation, or interface defects.
- Do not add module-name checks or standard-library-specific lowering.

This chunk uses a local source tree. It does not wait for network package
resolution.

**Tests**

One integration test checks the pinned standard library. It can be ignored in a
fast unit-test group if its normal runtime is high, but it must run in the final
workspace gate.

**Done when**

The exact standard-library source checks without edits and without special-case
lowering.

---

## [x] Chunk 11 — Add project-loader registration and manifest selection

**Files**

- `crates/nash-driver/src/project.rs`
- New project-loader contract or the smallest equivalent location
- Native Nash project adapter
- `crates/nash-project-aiken`
- `crates/nash-cli`
- Driver project tests

**Change**

- Move current `nash.jsonc` loading behind the common project-loader seam.
- Add `AikenProjectLoader` for `aiken.toml`.
- Select the closest owning manifest.
- Accept a direct manifest path.
- Report same-directory manifest ambiguity.
- Add an explicit selection option only if it is needed to resolve ambiguity.
- Keep current Nash project behavior unchanged.

**Tests**

- Nash project discovery.
- Aiken project discovery.
- Direct `aiken.toml` path.
- Ambiguous same-directory manifests.

**Done when**

`nash check` can enter an Aiken project without a Nash config file.

---

## [x] Chunk 12 — Implement Aiken manifest, environment, and config behavior

**Files**

- `crates/nash-project-aiken/src/manifest.rs`
- `crates/nash-project-aiken/src/discovery.rs`
- `crates/nash-project-aiken/src/environment.rs`
- `crates/nash-project-aiken/src/config_module.rs`
- Driver catalog integration

**Change**

- Load project name, version, compiler requirement, Plutus version, license,
  description, repository, dependencies, and config values.
- Validate the supported compiler and Plutus target.
- Discover `lib/`, `validators/`, and `env/` with Aiken file-name rules.
- Require `env/default.ak` when the environment directory exists and Aiken
  requires it.
- Select a named environment when requested.
- Implement Aiken `use env` resolution.
- Create a synthetic `config` module from the selected config section.
- Preserve generated definitions as synthetic source with useful project
  diagnostics.

**Tests**

- Default environment.
- Named environment.
- Missing default environment.
- Config values for bool, integer, bytes, list, and tuple shapes supported by
  Aiken 1.1.23.

**Done when**

One unmodified project that imports `env` and `config` checks through Nash.

---

## [x] Chunk 13 — Implement dependency, lock, and cache resolution

**Files**

- `crates/nash-project-aiken/src/dependencies.rs`
- `crates/nash-project-aiken/src/lock.rs`
- `crates/nash-project-aiken/src/store.rs`
- Driver package and module identity
- Project diagnostics

**Change**

- Resolve direct and transitive dependencies.
- Read and update `aiken.lock` with the exact selected behavior.
- Use Aiken package cache and build dependency paths.
- Normalize each dependency to `PackageId` and local package root.
- Discover dependency `lib/` only.
- Keep tests, benchmarks, validators, and environment modules from dependency
  packages out of the production import surface, as Aiken does.
- Support prepared-cache and offline test operation.
- Report missing packages, invalid locks, source failures, and version conflicts.

Do not call the Aiken type checker or compiler.

**Tests**

- One direct dependency.
- One transitive dependency.
- One locked prepared-cache build.
- One missing package diagnostic.

**Done when**

An unmodified locked Aiken project resolves and checks without network access in
CI.

---

## [x] Chunk 14 — Make module and interface identity package-aware

**Files**

- `crates/nash-driver`
- `crates/nash-can`
- `crates/nash-ast`
- Completed runtime layout and decoder identity code
- `crates/nash-codegen`
- `crates/nash-report`

**Change**

- Add `PackageId`, `ModuleKey`, and resolved dependency records.
- Key interfaces by package and module identity.
- Pass resolved imports to canonicalization.
- Extend definition, ADT, layout, decoder, and specialization identities with
  package identity.
- Keep Aiken source imports module-only.
- Report duplicate visible module providers instead of selecting one.
- Keep source URI only for diagnostics and I/O.

**Tests**

- Two packages with distinct modules.
- A transitive imported type keeps its package-qualified layout and decoder.
- Two visible packages that provide the same source module produce an ambiguity
  diagnostic.
- Native package ownership tests still pass.

**Done when**

No semantic cache or lookup can confuse definitions from different packages.

---

## [x] Chunk 15 — Add Aiken workspace support

**Files**

- `crates/nash-project-aiken/src/workspace.rs`
- Project-loader registry
- Module catalog assembly

**Change**

- Detect an Aiken workspace manifest.
- Expand member paths and globs with Aiken 1.1.23 behavior.
- Load each member as a package.
- Keep member package identity stable.
- Resolve member and external dependencies through one package graph.
- Report missing, duplicate, and nested conflicting members.

**Tests**

One small workspace with two members is sufficient. One member imports the other.

**Done when**

`nash check` on the workspace root checks all members in dependency order.

---

## [x] Chunk 16 — Support multiple validators and stable metadata

**Files**

- `crates/nash-source`
- `crates/nash-ast`
- `crates/nash-can`
- `crates/nash-driver`
- `crates/nash-codegen`
- `crates/nash-frontend-aiken/src/lower/validator.rs`
- Build output naming

**Change**

- Replace the one-generated-`main` assumption with source entry-point metadata.
- Allow more than one Aiken validator declaration in a module.
- Keep one generated entry function per validator.
- Permit source references to validator handler functions where Aiken permits
  them.
- Compile each solved entry point separately.
- Keep native Nash `main` as one native entry point.
- Preserve project and validator metadata for a later artifact layer.
- Emit collision-safe UPLC, Flat, and CBOR file names.

Recommended file identity:

```text
<package>.<module>.<validator>.uplc
<package>.<module>.<validator>.flat
<package>.<module>.<validator>.cbor
```

The exact escaping rules must be documented and stable.

**Tests**

- Two validators in one module produce two artifacts.
- Parameterized validators keep parameter order and labels.
- Two modules with the same validator name do not collide.
- Native validator output remains unchanged.

**Done when**

The solved build exposes one typed metadata record and one compiled artifact per
validator.

---

## [x] Chunk 17 — Preserve future blueprint input without generating a blueprint

**Files**

- Solved build output
- Driver artifact metadata
- `docs/aiken-frontend.md`
- `SPEC.md`

**Change**

Preserve:

- Project name.
- Project version.
- License.
- Description.
- Repository.
- Compiler target.
- Plutus version.
- Package and module identity.
- Validator name and documentation.
- Handler purpose and documentation.
- Parameter name, label, order, solved type, and data layout.
- Datum and redeemer names, solved types, and data layouts.
- Compiled program and output identity.

Do not serialize CIP-57 JSON in this plan.

**Tests**

One solved validator metadata snapshot or structural assertion is sufficient. Do
not create a large artifact snapshot suite.

**Done when**

A later Nash artifact plan can generate a blueprint without reparsing Aiken
source or calling the Aiken type checker.

---

## [x] Chunk 18 — CLI and language-server integration

**Files**

- `crates/nash-cli`
- `crates/nash-driver`
- `crates/nash-language-server`
- `docs/cli.md`
- `docs/aiken-frontend.md`

**Change**

- Route `check` and `build` through project-loader selection.
- Add environment selection to the shared build options.
- Show package and module identity in ambiguous project diagnostics.
- Build every solved validator entry point.
- Keep the existing editor parse and diagnostic path.
- Load Aiken project context for an open `.ak` file when practical through the
  shared project loader.
- Add no formatter or completion work.

**Tests**

- Real CLI `check` on an Aiken project.
- Real CLI `build` on a multi-validator Aiken project.
- One LSP diagnostic smoke test for an Aiken project file.

**Done when**

The public Nash commands exercise the same project, frontend, compiler, and build
paths as the integration tests.

---

## [x] Chunk 19 — Final standard-library and project acceptance

**Files**

- `crates/nash-driver/tests/fixtures/aiken/`
- Adapter differential tests
- Project integration tests
- Architecture and specification documents
- Sampo changesets

**Change**

Create a small final acceptance set:

1. `full-language`
   - Covers all remaining language categories in a compact set of modules.
2. `stdlib-project`
   - Uses the exact pinned official standard library through normal package
     resolution.
3. `dependency-project`
   - Uses one direct and one transitive dependency from a prepared cache.
4. `env-config-project`
   - Uses default and named environments plus synthetic config.
5. `multi-validator-project`
   - Contains tests, benchmarks, and at least two validators.

Use the official Aiken compiler as a differential oracle only for representative
semantic risks. Do not create a large generated evidence corpus.

Recommended differential cases:

- Pipeline with labeled arguments.
- Record update and qualified pattern.
- Polymorphic equality.
- Trace argument order.
- One package-defined custom data type at a validator boundary.
- Two validators from one module.

**Done when**

G1 through G11 pass and the final supported claim is accurate.

## Validation strategy

Run focused tests after each chunk. Run the broad workspace commands once near
completion and again after final fixes.

Reuse existing tests and fixtures first. One compact fixture can cover repeated
test requirements in several chunks. Add focused tests for new behavior and
specific risks, not one test for each private helper. Do not add a generated
corpus, an evidence generator, or a large report.

Run required standard-library and integration tests explicitly when the normal
command skips them. A skipped, ignored, unavailable, or failing required check
does not pass a gate. Keep records short: command, result, and unresolved failure.

Required final commands:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo insta test
```

Required real command checks:

```sh
cargo run -p nash-cli -- check <aiken-stdlib-project>
cargo run -p nash-cli -- check <aiken-dependency-project>
cargo run -p nash-cli -- check <aiken-env-config-project> --env <name>
cargo run -p nash-cli -- build <aiken-multi-validator-project>
cargo run -p nash-cli -- build examples/vesting
```

Keep the existing runtime-baseline differential tests. Add the small R1–R3
regression group and the focused differential cases listed in Chunk 19. Compare
source acceptance and rejection first. For accepted cases, compare matching
target, environment, parameter, input, and trace settings. Compare results,
failures, and relevant user traces; do not require identical bytes, hashes, or
execution budgets. Reuse cases when they cover several requirements.

Do not require network access in the final test suite.

## Diagnostics

Add stable project-level diagnostic codes. Keep frontend syntax and lowering codes
separate.

Suggested groups:

```text
NAP1xxx  Aiken project and manifest loading
NAP2xxx  dependency, lock, and cache resolution
NAP3xxx  environment and config selection
NAP4xxx  package and module identity
NAF2xxx  Aiken source parsing and lowering
NAF3xxx  Aiken type-directed compatibility lowering
NAB1xxx  Aiken validator entry-point and artifact errors
```

Examples:

```text
NAP1001  no aiken.toml found
NAP1002  both aiken.toml and nash.jsonc own this path
NAP2001  locked dependency is not available
NAP2002  dependency graph has conflicting versions
NAP3001  selected environment does not exist
NAP3002  env/ has no required default module
NAP4001  more than one visible package provides this module
NAF3001  labeled call does not match the resolved function labels
NAF3002  no Aiken equality operation exists for this solved type
NAB1001  two validator entry points map to the same output name
```

Normal user input must return diagnostics. It must not panic.

## Implementation rules

- Do not add reachable `todo!()`, `unimplemented!()`, or placeholder panics.
- Do not return dummy success values.
- Do not use silent fallbacks for an Aiken source or package rule.
- Do not call the Aiken type checker or code generator in production.
- Do not use standard-library module names as compiler conditions.
- Do not infer semantic identity from file names after project resolution.
- Do not let project-network code enter the syntax frontend.
- Do not let Aiken crate types enter common compiler crates.
- Do not change native Nash semantics to imitate Aiken.
- Update the architecture document when repository facts require a design
  change.
- Record each material design change in the pull-request description.

## Reference rules

Use a separate reference for each kind of decision:

- **Nash ownership and integration:** use the current repository for lifetimes,
  arena ownership, internal architecture, and native Nash behavior.
- **Aiken compatibility:** use exact Aiken `v1.1.23` for language, typing,
  project, and runtime behavior. Confirm the inventory, selected standard-library
  release, and compiler and Plutus targets against that source.
- **Required scope:** use this plan. Keep `docs/aiken-frontend.md` and `SPEC.md`
  aligned with the implemented design and verified compatibility rules.

Existing tests are regression checks. They do not override a verified Aiken
compatibility defect. Preserve the recorded runtime contract unless a concrete
upstream rule shows that it is wrong. For such a correction, record the pinned
source location, explain the change, and add or adapt a focused regression test.
Do not change native Nash semantics to match Aiken.

If a concrete inventory or project claim conflicts with the pinned source, record
the source location and correct the claim. Do not silently add behavior from a
newer release or invent replacement behavior. Do not reduce the target version,
remove a required language category, or make a required gate optional to claim
completion.

Update this plan and the architecture document when repository facts require a
design change. Record material changes and their reasons in the pull-request
description. Do not force the code to match an old interface sketch.

## Final claim

After all remaining source/project gates G1–G11 pass on the final code, Nash can
state:

> Nash supports Aiken 1.1.23 source and project compilation. An unmodified Aiken
> project can use the official prelude, standard library, dependencies,
> environments, configuration, libraries, and validators, and can check and
> build through Nash with equivalent observable on-chain behavior.

The documentation must also state:

> Nash does not yet provide Aiken test execution, benchmark execution, blueprint
> serialization, formatting, HTML documentation, editor completion, or exact
> script-byte compatibility.


## Follow-on work beyond source and project compatibility

These items remain tracked here but do not block the G1–G11 source/project gates.
They are not part of the current assignment. Leave them unchecked unless a
separate assignment completes them. Do not treat every unchecked box in this
file as part of the source/project finish line:

- [ ] **Artifacts (Plan 09).** Add CIP-57/`plutus.json` blueprint and schema
  serialization, script-hash and target-version artifact support, and parameter
  application to an existing blueprint. Use the solved metadata preserved above
  and verify artifacts against the same validator contract. Current builds emit
  UPLC, Flat and CBOR; identical bytes, hashes and budgets are not requirements.
- [ ] **Test and benchmark execution (Plan 10).** Run Aiken test and benchmark
  declarations after the source/project work enables parsing and type checking.
- [ ] **User tools (Plan 13 and LSP work).** Add Aiken source formatting, HTML
  documentation generation and editor completion. Preserve documentation comments
  as metadata now; shared compiler diagnostics are not a complete Aiken editor.
- [ ] **Optional upstream parser extraction.** Propose a parser-only
  `aiken-syntax` crate upstream, then replace `aiken-lang` when it is available.
  Keep the exact parser pin, one private AST version, sync policy and compatibility
  coverage until then. This external dependency cleanup does not block language,
  validator or project compatibility.
