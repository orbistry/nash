# Plan 09 — Validator modules and `nash build`

## Goal

Complete production validator builds using the existing Plan 07 pipeline.
Plan 08's optimizer is deferred; every build remains unoptimized. This plan
adds build configuration, target validation, script hashes, test-block
exclusion, and reliable artifact output. It does not implement Plan 10's test
runner or Plan 12's default imports.

Specifications: [validators](../docs/validators.md), [CLI](../docs/cli.md).

## Existing implementation

These requirements landed with Plan 07 and must be preserved:

- [x] Source and canonical modules retain `ModuleKind`.
- [x] Canonicalization requires an exposed validator `main`.
- [x] The driver checks solved `main` parameters for valid representations.
- [x] `build_with(db, graph, origins, finish)` retains canonical nodes, solved
  types, evidence, and interfaces in the build arena until `finish` returns an
  owned result. Frontend errors prevent the callback from running.
- [x] `Build::new(Input { module, types, tables })`, `compile`, and
  `program::assemble_core` compile the dependency closure of each validator.
  Unconstrained validator input variables default to `Data`.
- [x] `nash build`, `--out`, user traces, and compiler traces exist.
- [x] Named UPLC, raw Flat, and single-wrapped CBOR are generated.
- [x] Driver snapshots cover helper modules, ordinary modules producing no
  artifacts, serialization round trips, and unconstrained validator inputs.

The earlier sketches for `CompileMode`, a new retained-module pipeline,
`nash_codegen::validator`, optimizer options, and replacement CLI diagnostics
are obsolete. Extend the actual APIs; do not recreate those sketches.

## 1. Configuration and command-line precedence

- [ ] Add application and package `plutusVersion` (`v1`, `v2`, `v3`, default
  `v3`), `traceLevel` (`silent`, `compact`, `verbose`, default `silent`), and
  `compilerTraces` (boolean, default `false`) configuration.
- [ ] Validate configuration types and values through existing source-aware
  diagnostics; keep the JSON schema and serde representation consistent.
- [ ] CLI overrides take precedence over the selected validator project's
  config. Workspace members retain their own settings. A validator's settings
  apply to its complete dependency closure, not its dependencies' settings.
- [ ] Keep user tracing and compiler tracing independently configurable,
  including disabling configured compiler traces from the command line.
- [ ] Reject unavailable optimizer settings; do not advertise optimization
  levels or introduce an identity optimizer as a substitute for Plan 08.

## 2. Production frontend

- [ ] Exclude parsed `tests` blocks before canonicalization for `nash build`.
  Keep ordinary checking behavior unchanged. Test-only imports must not enter
  the production dependency graph.
- [ ] Retain the existing `build_with` callback and arena lifetime guarantees.
  Add only the policy needed to distinguish production builds from checking.
- [ ] Snapshot real Nash sources covering test-only names/imports, ordinary
  imports, validator checks, and failure gating. Do not implement test execution.

## 3. Target validation and script hashes

- [ ] Assemble UPLC with the selected target's version, independently of the
  script hash language tag.
- [ ] Validate every generated term, constant type, and builtin against an
  explicit supported ledger/protocol compatibility baseline. Reject unsupported
  features, including `constr`/`case` on the V1/V2 baseline, rather than merely
  changing a version field or hash tag. State that baseline in diagnostics/docs.
- [ ] Preserve `assemble_core` as the default V3 API for existing callers.
- [ ] Compute script hashes from the correct language tag and single-wrapped
  CBOR bytes using Blake2b-224. Verify against independent known Cardano vectors,
  including V3, and cross-check target tags and serialization round trips.

## 4. Artifacts

- [ ] Write `Module.Name.uplc` as named UPLC text, `.flat` as raw bytes, and
  `.cbor` as hex text of `CBOR(bytes(flat))`. Do not double-wrap CBOR.
- [ ] Print each validator's hash in the CLI.
- [ ] Remove only stale artifacts owned by earlier successful builds, including
  when the new build contains no validators. Preserve unrelated files.
- [ ] Preserve existing artifacts on frontend or codegen failure. Reject unsafe
  output paths and unsafe ownership metadata; do not follow artifact symlinks.
- [ ] Preserve deterministic output order and duplicate module-name rejection.

## 5. Acceptance

- [ ] Source-based snapshots and integration tests prove config/CLI precedence,
  member-specific targets, production test exclusion, target rejection, hash
  vectors, dotted filenames, artifact encodings, zero-validator cleanup,
  unrelated-file preservation, and failure preserving prior output.
- [ ] Existing validator checks, diagnostics, codegen, and snapshots remain green.
- [ ] Update the specifications and add changesets for changed crates.
- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test`
- [ ] Mark Plan 09 complete in `SPEC.md` only after the acceptance audit. Leave
  Plan 08 unchecked and deferred. Personal scratch cases are outside this plan.
