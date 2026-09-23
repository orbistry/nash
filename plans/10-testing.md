# Plan 10 — `tests` blocks, power-assert, property testing, `nash test`

## Goal and boundaries

Compile module-local unit tests and properties into standalone UPLC, execute them
on the CEK machine, shrink counterexamples, and report budgets, labels, traces,
and source-based power assertions. [docs/testing.md](../docs/testing.md) defines
the semantics; [docs/cli.md](../docs/cli.md) defines command-line behavior.

Plan 08 remains deferred. Preserve Plan 09's production test exclusion, artifact
handling, target validation, member-specific configuration, and arena lifetime
contract. Personal scratch cases, versioned exports, and artifact history are
outside this plan. Do not add automatic Prelude imports.

## Reconciled implementation and prerequisites

The parser and source AST already represent test blocks, modifiers, budgets,
via binders and blocks. Canonicalization now retains test blocks with isolated scopes. Type
inference runs directly in `nash-solve`; `nash-constrain` supplies shared types
and diagnostics, not the constraint-generation pipeline in the former sketches.
`Build::new(Input { module, types, tables })` specializes canonical definitions;
`assemble_core_for_version` applies the existing passes and target validation.
`build_with` strips tests for production and retains solved canonical nodes until
its callback returns an owned result. Extend these APIs rather than replacing
them with the former `CompileMode`, retained-store, or optimizer sketches.

`Prop` and `Test` are now implemented in core. This plan owns the minimum real modules
needed for executable properties, labels and assertion failures, including the
ordinary primitive/composite generation functions. Plan 12 owns
remaining stdlib expansion. Use the existing Option, Lift, Show, Functor,
Applicative and Monad APIs. `Applicative`'s method is `apply`.

## 1. Canonical tests and scopes

- [x] Retain canonical test metadata, source regions, bodies and via binders.
- [x] Test names are unique per module, including unit/property collisions.
- [x] Tests see private module declarations and block-local imports. Test-only
  imports never become ordinary module imports or public exports.
- [x] Each test has a fresh local scope. Via generators resolve in module/test
  import scope; body patterns are bound in order and checked for duplicates.
- [x] Outer test `do` is sequencing: expression statements and final result are
  unit; `<-` evaluates its RHS before introducing bindings. Nested ordinary `do`
  remains monadic; ordinary `let` retains recursive binding behavior.
- [x] Add real Nash source success/error snapshots, including name collisions,
  scoped imports, private access, binding scope and nested monadic blocks.

## 2. Type inference and pattern checks

- [x] Infer generators as `Prop.generator 'a`, bind patterns to each generated type,
  and check body/result types in the existing direct solver.
- [x] Preserve per-node solved types, schemes and use-site evidence needed by
  codegen. Tests must not leak declarations into module interfaces.
- [x] Check generators, bodies, via patterns and sequence bindings through
  existing exhaustiveness/redundancy analysis; via patterns are irrefutable.
- [x] Render source-aware diagnostics with full Nash inputs in snapshots.

## 3. Power assertions

- [x] Instrument test-body assertions using solved types and real Show evidence.
  Keep ordinary validator assertions unchanged.
- [x] Capture the specified strict subexpressions exactly once in evaluation
  order. Do not force lazy branches, short-circuit operands or lambda bodies.
- [x] Evaluate Show only on failure; missing Show produces `?` without making
  compilation fail. Preserve source regions for rendering.
- [x] Emit an explicit assertion-site marker even when there are no captures,
  plus indexed capture payloads. Instrumentation survives silent user tracing.
- [x] Test order, single evaluation, laziness, unavailable Show, zero captures,
  multiple assertion sites, Unicode and multiline source/value rendering.

## 4. Test code generation

- [x] Compile each unit test to an owned Flat program.
- [x] Compile properties to one preparation program returning native PRNG state,
  a property-body function, and a deferred display function. Generated values
  remain captured in these functions; generators run once per candidate.
- [x] Apply the existing Core passes and target checks for the selected member's
  settings; never silently change a requested ledger target.
- [x] Preserve source/path/assertion metadata and binder text in owned outputs.
- [x] Test compilation, decoding and evaluation from real Nash source snapshots.

## 5. Core support, PRNG and evaluation

- [x] Implement real core Prop and Test modules with direct generation functions. Imports remain explicit. Keep reserved Test traces independent of
  user trace suppression.
- [x] Seed with Blake2b-256 of u32 big-endian bytes. Thread the 32-byte seed and
  recursively newest-first trace nodes in Seeded; replay next-first Choice/Group
  nodes with strict group-local boundaries and a separate consumed history.
  Normalize recorded trees into replay order in Rust, without Nash reversals.
- [x] Choice values are u64; explicitly validate primitive bounds and replay
  values. Never truncate arbitrary-precision integers. Larger generated values
  can be constructed from multiple primitive choices.
- [x] Encode/decode losslessly; reset choice history separately between seeded
  iterations. Reject malformed protocol results without panicking.
- [x] Evaluate using the chosen ledger language's bundled cost model and machine maximum budget.
  Bundled costs are not live mainnet protocol parameters.
  Check requested CPU/memory limits separately from body success/failure.

## 6. Choice-sequence shrinking

- [x] Implement tree-region deletion with coordinated numeric edits, region zeroing,
  per-choice binary reduction, chunk sorting, neighbour swaps and redistribution;
  repeat until no improvement.
- [x] Accept only shorter sequences, or equal-length lexicographically smaller
  consumed primitive sequences. Revalidate normalized traces. Cache exact trees: public replay-state inspection makes
  general prefix reuse unsound.
- [x] Draw failure/None is an invalid replay; classify body outcomes according to
  pass/fail/fail-once polarity. Do not accept budget/protocol errors as evidence
  for a body counterexample.
- [x] Preserve or recompute final shown values, traces and assertion payloads,
  including zero-choice counterexamples and unsuccessful shrink attempts.
- [x] Cover strict progress, invalid replay, caching and counterexample minima.
- [x] Reconstruct boundary proposals in Nash from explicit choices, then validate
  through strict replay; retain reduced replay trees in outcomes.
- [x] Retain valid unsuccessful replay feedback and repair size-dependent edits.
  Add adaptive draw deletion, one-to-five primitive deletion, joint equal-choice
  and alphabet reduction, common offsets, whole-draw reordering, and -XX/--X edits.

## 7. Runner

- [x] Implement unit pass/fail and property pass/fail/fail-once semantics.
- [x] Seeded generator error or None always fails as a generator error. Retain PRNG
  state before calling the property body, including expected failures under `fail`.
- [x] Enforce per-iteration budgets independently of expected body failures;
  report componentwise maxima from seeded runs, excluding shrinking/display.
- [x] Count labels only from seeded runs; split reserved assertion/label payloads
  from user traces, handling malformed payloads without panics.
- [x] Run with bounded parallelism and independent PRNG/arena state; jobs 1 and 8
  produce identical ordered results for a fixed seed.

## 8. Reporting

- [x] Report terminal results, budgets, iterations, label coverage, traces,
  shrunk counterexamples and power-assert source/value layouts.
- [x] Provide stable structured JSON with seed and replay information.
- [x] Render Unicode and multiline content safely; normalize nondeterministic
  timing only in snapshots, not semantic results.
- [x] Test reports with source-backed end-to-end cases and focused renderer tests.

## 9. Driver, CLI and acceptance

- [x] Add `nash test`/`nash t`, seed, max-success, repeatable match, exact matching,
  trace-level, jobs, coverage and JSON controls, validating option values.
- [x] Resolve test-only imports/dependencies for check/test; keep them outside
  production dependency graphs. Define and test that check type-checks tests
  without executing them. Test execution does not run dependency-package tests
  accidentally.
- [x] Respect member-specific target/config and CLI precedence; tests default to
  verbose user tracing when absent in config, with compiler traces enabled.
- [x] Add runnable `examples/order` and CI coverage proving `--seed 1` exits 1
  and reports a shrunk counterexample.
- [x] Add end-to-end pass/fail/error/filter/replay/shrink/budget/report/workspace
  tests and preserve Plan 09 production and output-preservation regressions.
- [x] Update specs and per-crate Sampo changesets; commit coherent chunks with jj.
- [x] Run `cargo fmt --all`,
  `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
- [x] Audit every requirement against implementation and test evidence; only then
  mark Plan 10 complete in SPEC.md. Keep Plan 08 unchecked and deferred.

## Acceptance evidence

Completed on 2026-09-18. Every requirement above is covered by the implementation
and the following checks:

| Requirements | Evidence |
| --- | --- |
| Canonical scopes, sequencing, types and patterns (1–2) | Frontend source snapshots cover private access, test imports, duplicate names, via binders, unit bodies and nested monadic sequencing. The full workspace suite includes solver, canonicalization and pattern diagnostics. |
| Power assertions and executable roots (3–4) | `crates/nash-codegen/src/tests/integration.rs` executes compiled Nash inputs and checks evaluation order, lazy branches, failure-only Show, source delimiters, generator protocols and target rejection. |
| Core generators and PRNG (5) | `crates/nash-driver/tests/testing_core.rs` exercises the real core modules, bounds, seeded generation, replay and reserved traces; runtime tests cover lossless state and malformed input. |
| Shrinking and runner semantics (6–7) | `crates/nash-test/tests/runtime.rs` covers modifiers, replay minima, exact caching, labels, deterministic parallel results and retained failure logs. Internal runner tests trigger actual CEK exhaustion and verify that it cannot pass under failure modifiers or become a shrink counterexample. |
| Reports (8) | Runtime and CLI snapshots verify terminal/JSON results, Unicode display widths, multiline source and captured values, and zero-capture assertions. |
| Driver, CLI and production isolation (9) | Eleven CLI integration tests cover options, filtering before codegen, trace precedence, workspace targets, dependency isolation and exit statuses. Driver production and dependency tests preserve build/check boundaries. |

Final validation passed: `cargo fmt --all`, strict all-target/all-feature Clippy,
and `cargo test` (3,366 passed, 0 failed, 3 ignored). No pending snapshots remain.
The `examples/order` seed-1 acceptance command exits 1 with three passing tests
and one intentionally failing property, reduced to `a = 0`, `b = 1`; CI checks
that exit status and counterexample output.

Native property programs support Plutus V1, V2, and V3 at protocol 11.
Local path and workspace test dependencies are supported; registry/git fetching
remains outside the existing dependency resolver. Plan 08 stays deferred.
