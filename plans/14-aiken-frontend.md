# Implementation checklist

## Completion gates for this branch

- [x] G1: `nash-frontend` owns the parser-neutral contract; adapters return owned
  diagnostics/inspection and arena-backed `nash_source::Module` only.
- [x] G2: Native graph construction, compilation, diagnostics, CLI and LSP pass
  existing regressions through the shared frontend selection path.
- [x] G3: `tests/fixtures/aiken/supported/src/add_one.ak` reaches the official
  parser, lowering, canonicalization, solving, nitpick and interface construction
  through project discovery and the real `nash check` command.
  A native validator consumer also reaches Core/UPLC and executes in CEK.
- [x] G4: Functions, constants, imports, primitive annotations, patterns,
  expressions, undecorated data types, aliases and exports have explicit lowering
  policies; unsupported syntax and overflowing integers have located diagnostics.
- [x] G5: Source-root module identity, mixed-language imports, retained inspection
  failures and ambiguity diagnostics agree between discovery and compilation.
- [x] G6: No reachable placeholders or Aiken types outside the adapter; formatting
  and strict lint checks pass.
- [x] G7: Focused frontend/driver tests, full workspace checks/tests, native CLI
  and Aiken CLI fixtures pass; the unsupported fixture fails with a stable code.

### Current completion pass

The existing G1–G7 checkmarks record the common-syntax implementation. This pass
also completes Phase 2 for an explicitly bounded validator profile, rather than
treating the available backend as a blocker. Full package/layout compatibility
and upstream parser extraction remain follow-on work.

- [x] V1: Define a minimal official-Aiken-compatible handler/parameter/input
  profile and reject every other handler combination with located diagnostics.
- [x] V2: Lower supported validators to ordinary Nash `main`, including purpose
  dispatch, checked boundary decoding, and Boolean-result-to-unit/failure.
- [x] V3: Build the same supported validator with the pinned official Aiken
  compiler and Nash; compare CEK success, failure and user traces, including
  malformed boundary Data.
- [x] V4: Exercise real native and Aiken CLI check/build paths and retain existing
  native, mixed-language, source-region and editor regressions.
- [x] V5: Update final architecture, profile exclusions and progress; pass focused
  regressions, workspace check, strict clippy, formatting and full workspace tests.

The concrete fixtures live under `crates/nash-driver/tests/fixtures/aiken/`.
`unsupported/src/unsupported.ak` contains an Aiken test declaration, deliberately
outside the concrete NashV1 compatibility profile.

### Scope facts and prerequisite work

The first implementation was based on `main`, where production compilation ended
at solved interfaces. The branch now uses `release/main`, which includes the
source-to-UPLC code generation and native validator builds from `plan-7`.
The current completion pass adds bounded Aiken mint/fallback validators and
official runtime comparisons. `NAF2301` now denotes unsupported handler sets,
boundary annotations or role metadata, rather than rejecting every validator.

Phase 3's upstream parser-only extraction is external follow-on work. This branch
pins the official `aiken-lang` parser behind one adapter and records its sync policy.
The acceptance strategy below is refined to focused behavioral/integration tests,
not four fixtures per private helper: consumers and semantic boundaries are the
contract, and the requested finish-line fixture is mandatory.

### Plan 7 rebase integration

- [x] Preserve the original work and rebase onto the current codegen branch.
- [x] Preserve the backend finish callback while threading frontend catalog metadata.
- [x] Lower fixed primitive expressions and patterns through Core/UPLC without
  native literal/equality evidence.
- [x] Execute an Aiken-backed native validator in CEK and retain native build checks.
- [x] Re-run workspace validation and replace the obsolete no-codegen documentation.

### Required semantic bridge discovered during implementation

Native Nash literal expressions and patterns are overloaded through
`Literal.FromInt`/`FromBytes`/`FromString` (and pattern `Eq`). Aiken primitive
literals cannot use those nodes without changing their semantics or requiring a
synthetic standard library. Add frontend-neutral fixed `Constant` nodes to the
source and canonical ASTs, with direct primitive typing in Nash's existing solver.
Native literal nodes and their evidence behavior remain unchanged.

- [x] Validate fixed primitive expression and pattern types without `Literal`/`Eq`
  imports, including rejection of a mismatched primitive annotation.

## Phase 0: frontend seam

- [x] Add `nash-frontend`.
- [x] Add `nash-frontend-nash`.
- [x] Register one shared frontend registry for graph building, compilation, CLI, and LSP.
- [x] Replace direct parser use in graph construction with `Frontend::inspect`.
- [x] Retain inspection failures as module-local diagnostics; do not stop independent graph nodes.
- [x] Replace direct parser use in `compile_module` with `Frontend::parse`.
- [x] Add `ModuleCatalog` and remove `.nash` suffix inference.
- [x] Convert frontend diagnostics in `nash-driver`; keep `nash-report` unchanged.
- [x] Keep the current build-wide arena for source, canonical nodes, evidence, and interfaces.

## Phase 1: Aiken dependency and common syntax

- [x] Resolve the Rust 1.92.0 versus 1.94.1 toolchain mismatch.
- [x] Add exact-pinned `aiken-lang` behind the adapter crate.
- [x] Add Aiken dependency inspection.
- [x] Add Aiken import lowering and module mapping.
- [x] Add type annotation lowering and primitive representation mapping.
- [x] Add pattern lowering.
- [x] Add integer range diagnostics or change Nash's literal representation.
- [x] Add expression lowering.
- [x] Add functions and constants.
- [x] Add undecorated data types and aliases.
- [x] Add public export generation.
- [x] Preserve source regions in all lowered nodes.
- [x] Run `nash check` on `.ak` fixtures.

Implementation choices, justified by the current compiler:

- Fixed `Constant` nodes preserve Aiken primitive literals without native literal
  trait requirements. Native literal semantics are unchanged.
- User type names map to little Nash names (`Choice` -> `choice`) so primitive
  fields are representable without unchecked Data conversion. Nash representation
  restrictions still apply to containers of these types; no Aiken wire-layout
  compatibility is claimed.
- Sequential bindings use strict hygienic lambda applications. Nullary functions
  use the native unit-argument ABI, not constant declarations.
- `ModuleCatalog` carries source-root identity and package ownership. Duplicate
  canonical names are diagnosed because current semantic interfaces cannot safely
  distinguish same-name providers across packages.
- Optional role metadata preserves native validator headers. The native adapter
  reuses existing syntax report conversion internally; the common contract and
  driver receive no parser-specific types.
- A private concrete profile replaces the proposed unused profile trait. The
  source of truth for the complete supported subset and explicit exclusions is
  `docs/aiken-frontend.md`, including case-sensitive mixed-language import limits.

## Phase 2: bounded validators

NashV1 accepts one mint handler plus optional else, or else-only validators.
Validator parameters, transactions and fallback contexts remain raw Data; redeemers
support explicit Int, ByteArray or Data, and mint policies use ByteArray. Other
purposes and custom datum/layout conversion are explicitly rejected. This is the
smallest nontrivial dispatch profile, not full Aiken validator ABI compatibility.

- [x] Define the accepted Aiken handler set.
- [x] Synthesize purpose dispatch.
- [x] Turn a handler result of `False` into explicit Nash failure.
- [x] Insert boundary conversion and validation.
- [x] Reject unsupported handler combinations with precise diagnostics.
- [x] Compare CEK results and traces with the official Aiken compiler.

The adapter's private validator lowerer emits only existing Nash source nodes.
The exact-pinned official compiler and CEK are test-only oracles inside the adapter;
the driver is a dev-dependency there solely to exercise the real Nash path.

The first differential run exposed that official mint policy patterns decode to
ByteArray despite the internal prelude's Data annotation. The accepted profile
therefore requires ByteArray, rather than duplicating a misleading annotation.
Context decoding follows the official demand-driven boundary, not a new exhaustive
ledger schema check. The architecture document records discarded/raw field behavior.

## Phase 3: dependency cleanup — partial, upstream follow-on

- [ ] Propose an upstream parser-only `aiken-syntax` crate.
- [ ] Replace `aiken-lang` when the parser-only crate is available.
- [x] Keep one Aiken AST version behind `nash-frontend-aiken`.
- [x] Add an upstream sync policy and focused compatibility coverage matrix.

## Full Aiken support: remaining scope

Phases 0–2 complete the bounded NashV1 profile, not full Aiken compatibility.
The exclusions in `docs/aiken-frontend.md` remain intentional follow-on work:

1. **Additional validator contracts and runtime checks.** Extend beyond mint and
   fallback to other purposes and optional datum conversion. Add corresponding
   pinned official compiler fixtures and CEK comparisons before enabling each ABI.
   The existing mixed native/Aiken test alone is not an ABI conformance check;
   adapter-local official comparisons now cover the enabled profile.
2. **Data layout and containers.** Define Aiken-compatible encoding for user
   data types and custom encoding decorators. Add checked boundary conversions.
   Resolve the current little-ADT limits in `List` and `Pair`. Add Pair values
   and patterns. The current lowercase type mapping does not provide wire
   compatibility. This work must support the boundary rules in item 1.
3. **Remaining language and builtins.** Remove the temporary `i128` limit.
   Extend the 33-name builtin mapping with verified signatures and semantics.
   Add the excluded expressions, patterns and annotations, including `expect`,
   Data casts, equality, pipelines, record updates, tuple indexing, labels,
   curve literals and trace formatting. Use the exclusion list in the design
   doc as the full inventory; representative syntax tests are not full coverage.
4. **Projects, packages and standard library.** Add Aiken project/dependency
   support and verify real Aiken standard-library imports. Resolve package
   identity and the current duplicate-module-name limit. Exact catalog imports
   and Nash's planned `nash/core` library do not provide this compatibility.
   Include the currently rejected environment/configuration modules.
5. **Build artifacts.** Define and add blueprint/schema, script-hash and target
   version support with Plan 09. Current builds emit only UPLC, Flat and CBOR.
   Verify artifact compatibility against the same validator contract.
6. **Tests and user tools.** Extend Plan 10 for Aiken test and benchmark
   declarations; both are currently rejected. Define Aiken formatting,
   documentation and editor support with Plan 13 and the LSP work. Current LSP
   support provides shared compiler diagnostics, not a full Aiken editor.

Phase 3's parser-only extraction is optional dependency work. It does not block
validator or language support. Keep the exact parser pin until an upstream
replacement is available.

## Acceptance tests

The original per-helper four-fixture proposal is replaced with focused semantic
coverage, as requested for this implementation:

- [x] Native parsing, nested identity, validator header and compiler regressions.
- [x] Extension selection and explicit frontend override.
- [x] Real Aiken `add_one` project through solved interfaces and typed consumers.
- [x] Representative expression, scope, pattern, data, alias and export lowering.
- [x] Unsupported declarations/features and integer overflow diagnostics.
- [x] Useful byte regions, including CRLF/non-ASCII source and native syntax errors.
- [x] Mixed-language imports, exact nested identity, ambiguity and isolated failures.
- [x] Real CLI supported/unsupported fixtures and unsaved/standalone LSP diagnostics.

Validation on the `plan-7` baseline: `cargo fmt --all -- --check`,
`cargo check --workspace --all-targets`, strict workspace/all-feature clippy,
focused adapters/solver/backend tests, and `cargo test --workspace`
(3,211 passed, 3 ignored). The Aiken-backed native validator executes in CEK
(41 succeeds, 40 fails) and the real CLI emits UPLC/Flat/CBOR (51 Flat bytes).
The native vesting CLI build emits both validators (515 and 520 Flat bytes);
its existing ledger-case execution regressions pass. Temporary CLI projects and
generated artifacts were removed after smoke checks.

Additional validator purposes and data-layout features require further official
Aiken differential tests; the bounded mint/fallback profile is covered.

### Release/main rebase checks

The branch was rebased onto `origin/release/main` at `f2b3e766`. The manifest
conflict was resolved with the release versions and the frontend dependencies.
The local work was restored as unstaged changes. The backup branch
`backup/aiken-frontend-before-release-main-20260914` and the named stash were kept.

Checks on this base passed:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`: 3,211 passed, 3 ignored.
- Real CLI check of the supported Aiken fixture.
- Real CLI check of the unsupported fixture: exit 1 with located `NAF2201`.
- Real CLI build of a temporary native validator that imports Aiken: UPLC,
  Flat and CBOR files, with 51 Flat bytes.

These historical rebase checks did not establish official validator conformance.
The current completion pass adds the adapter-local comparisons described above.

### Completed bounded frontend validation

All G1–G7 and V1–V5 gates were rechecked for this completion pass:

- `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- Focused frontend/driver tests and `cargo test --workspace`: 3,215 passed,
  3 ignored, including native/mixed imports, diagnostics and editor buffers.
- Exact-pinned Aiken/Nash differential CEK tests cover the enabled validator
  boundaries, return values and user traces.
- Real CLI native vesting check/build (515/520 Flat bytes), Aiken library and
  validator checks, and Aiken mint build (138 Flat bytes with user traces).
- Unsupported Aiken CLI build exits 1 with located `NAF2201`, without artifacts.
- Final review reproduced and fixed mandatory policy decoding when the policy
  argument is discarded. Differential cases now reject both missing policy fields
  and non-byte policy Data, matching the official purpose pattern before redeemer
  decoding. Final checks were rerun after this correction.

No external blocker remains for the bounded profile. Phase 3's upstream extraction
and the explicitly listed full-compatibility extensions remain follow-on work.
