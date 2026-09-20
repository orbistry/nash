# nash-config

## 0.5.0 — 2026-09-20

### Minor changes

- [9fa1d0a7](https://github.com/orbistry/nash/commit/9fa1d0a7de4c416cbaa7e45b5b56702b80046119) Add module-local unit tests and properties, scoped test dependencies, source-aware
  power assertions, deterministic seeded fuzzing and counterexample shrinking.
  Provide `nash test` with budget checks, labels, trace controls, parallel execution,
  and terminal/JSON reports. Type-check tests with `nash check` while keeping them
  out of production builds. Add core Fuzz and Test support with explicit imports.
  
  Preserve short-circuit evaluation for the core boolean infix operators, including
  inside instrumented assertions. — Thanks @MicroProofs!

## 0.4.0 — 2026-09-18

### Minor changes

- [bf78f35](https://github.com/orbistry/nash/commit/bf78f35258de7823f8d147f85b12aba1ed57b783) Complete validator builds with project and CLI target/trace settings, production
  test-block exclusion, protocol-10 target validation, and verified script hashes.
  Write single-wrapped CBOR as hex text instead of binary and track generated
  artifacts for safe stale-output cleanup. Existing output directories without an
  ownership manifest must be cleared of colliding artifacts or replaced with a
  fresh output directory. Optimizer settings remain unavailable. — Thanks @MicroProofs!

## 0.3.0 — 2026-02-20

### Minor changes

- [c46f722](https://github.com/nash-script/compiler/commit/c46f72228fb027163b0590fa9075edc8eeff39c7) Unify into a single `nash` binary with clap for argument parsing, octocrab for downloading missing compiler versions from GitHub Releases, and automatic version proxying before clap runs. — Thanks @rvcas!

