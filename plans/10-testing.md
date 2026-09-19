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
via binders and blocks. Canonicalization currently rejects test blocks. Type
inference runs directly in `nash-solve`; `nash-constrain` supplies shared types
and diagnostics, not the constraint-generation pipeline in the former sketches.
`Build::new(Input { module, types, tables })` specializes canonical definitions;
`assemble_core_for_version` applies the existing passes and target validation.
`build_with` strips tests for production and retains solved canonical nodes until
its callback returns an owned result. Extend these APIs rather than replacing
them with the former `CompileMode`, retained-store, or optimizer sketches.

`Fuzz` and `Test` are not yet in core. This plan owns the minimum real modules
needed for executable properties, labels and assertion failures, including the
fuzzer trait instances and useful primitive/composite generators. Plan 12 owns
remaining stdlib expansion. Use the existing Option, Lift, Show, Functor,
Applicative and Monad APIs. `Applicative`'s method is `apply`.

## 1. Canonical tests and scopes

- [ ] Retain canonical test metadata, source regions, bodies and via binders.
- [ ] Test names are unique per module, including unit/property collisions.
- [ ] Tests see private module declarations and block-local imports. Test-only
  imports never become ordinary module imports or public exports.
- [ ] Each test has a fresh local scope. Via generators resolve in module/test
  import scope; body patterns are bound in order and checked for duplicates.
- [ ] Outer test `do` is sequencing: expression statements and final result are
  unit; `<-` evaluates its RHS before introducing bindings. Nested ordinary `do`
  remains monadic; ordinary `let` retains recursive binding behavior.
- [ ] Add real Nash source success/error snapshots, including name collisions,
  scoped imports, private access, binding scope and nested monadic blocks.

## 2. Type inference and pattern checks

- [ ] Infer generators as `Fuzz.fuzzer 'a`, bind patterns to each generated type,
  and check body/result types in the existing direct solver.
- [ ] Preserve per-node solved types, schemes and use-site evidence needed by
  codegen. Tests must not leak declarations into module interfaces.
- [ ] Check generators, bodies, via patterns and sequence bindings through
  existing exhaustiveness/redundancy analysis; via patterns are irrefutable.
- [ ] Render source-aware diagnostics with full Nash inputs in snapshots.

## 3. Power assertions

- [ ] Instrument test-body assertions using solved types and real Show evidence.
  Keep ordinary validator assertions unchanged.
- [ ] Capture the specified strict subexpressions exactly once in evaluation
  order. Do not force lazy branches, short-circuit operands or lambda bodies.
- [ ] Evaluate Show only on failure; missing Show produces `?` without making
  compilation fail. Preserve source regions for rendering.
- [ ] Emit an explicit assertion-site marker even when there are no captures,
  plus indexed capture payloads. Instrumentation survives silent user tracing.
- [ ] Test order, single evaluation, laziness, unavailable Show, zero captures,
  multiple assertion sites, Unicode and multiline source/value rendering.

## 4. Test code generation

- [ ] Compile each unit test to an owned Flat program.
- [ ] Compile properties to deterministic `draw` and `run` programs sharing the
  same generator semantics. Inputs are Data PRNG values; results follow the
  documented option/tuple protocol. Generated values stay inside UPLC.
- [ ] Apply the existing Core passes and target checks for the selected member's
  settings; never silently change a requested ledger target.
- [ ] Preserve source/path/assertion metadata and binder text in owned outputs.
- [ ] Test compilation, decoding and evaluation from real Nash source snapshots.

## 5. Core support, PRNG and evaluation

- [ ] Implement real core Fuzz and Test modules and the required fuzzer trait
  instances. Imports remain explicit. Keep reserved Test traces independent of
  user trace suppression.
- [ ] Seed with Blake2b-256 of u32 big-endian bytes. Thread the 32-byte seed and
  newest-first choices in Seeded; replay next-first choices with a remaining count.
- [ ] Choice values are u64; explicitly validate primitive bounds and replay
  values. Never truncate arbitrary-precision integers. Larger generated values
  can be constructed from multiple primitive choices.
- [ ] Encode/decode losslessly; reset choice history separately between seeded
  iterations. Reject malformed protocol results without panicking.
- [ ] Evaluate using the chosen ledger cost model and machine maximum budget.
  Check requested CPU/memory limits separately from body success/failure.

## 6. Choice-sequence shrinking

- [ ] Implement chunk deletion (including predecessor decrement), chunk zeroing,
  per-choice binary reduction, chunk sorting, neighbour swaps and redistribution;
  repeat until no improvement.
- [ ] Accept only shorter sequences, or equal-length lexicographically smaller
  sequences. Cache exact replay outcomes: public replay-state inspection makes
  general prefix reuse unsound.
- [ ] Draw failure/None is an invalid replay; classify body outcomes according to
  pass/fail/fail-once polarity. Do not accept budget/protocol errors as evidence
  for a body counterexample.
- [ ] Preserve or recompute final shown values, traces and assertion payloads,
  including zero-choice counterexamples and unsuccessful shrink attempts.
- [ ] Cover strict progress, invalid replay, caching and counterexample minima.

## 7. Runner

- [ ] Implement unit pass/fail and property pass/fail/fail-once semantics.
- [ ] Seeded generator error or None always fails as a fuzzer error. Recover PRNG
  state with draw on the original input after expected body errors under `fail`.
- [ ] Enforce per-iteration budgets independently of expected body failures;
  report componentwise maxima from seeded runs, excluding shrinking/recovery.
- [ ] Count labels only from seeded runs; split reserved assertion/label payloads
  from user traces, handling malformed payloads without panics.
- [ ] Run with bounded parallelism and independent PRNG/arena state; jobs 1 and 8
  produce identical ordered results for a fixed seed.

## 8. Reporting

- [ ] Report terminal results, budgets, iterations, label coverage, traces,
  shrunk counterexamples and power-assert source/value layouts.
- [ ] Provide stable structured JSON with seed and replay information.
- [ ] Render Unicode and multiline content safely; normalize nondeterministic
  timing only in snapshots, not semantic results.
- [ ] Test reports with source-backed end-to-end cases and focused renderer tests.

## 9. Driver, CLI and acceptance

- [ ] Add `nash test`/`nash t`, seed, max-success, repeatable match, exact matching,
  trace-level, jobs, coverage and JSON controls, validating option values.
- [ ] Resolve test-only imports/dependencies for check/test; keep them outside
  production dependency graphs. Define and test that check type-checks tests
  without executing them. Test execution does not run dependency-package tests
  accidentally.
- [ ] Respect member-specific target/config and CLI precedence; tests default to
  verbose user tracing when absent in config, with compiler traces enabled.
- [ ] Add runnable `examples/order` and CI coverage proving `--seed 1` exits 1
  and reports a shrunk counterexample.
- [ ] Add end-to-end pass/fail/error/filter/replay/shrink/budget/report/workspace
  tests and preserve Plan 09 production and output-preservation regressions.
- [ ] Update specs and per-crate Sampo changesets; commit coherent chunks with jj.
- [ ] Run `cargo fmt --all`,
  `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
- [ ] Audit every requirement against implementation and test evidence; only then
  mark Plan 10 complete in SPEC.md. Keep Plan 08 unchecked and deferred.
