# nash-config

## 0.7.0 — 2026-10-10

### Minor changes

- [6c40e488](https://github.com/orbistry/nash/commit/6c40e48881220fbdd71f63040c19efa782bb6e21) Add explicit O2 compilation for build and test. O2 requires silent settings, removes user and compiler traces, and discards trace message computations even when they fail or diverge before applying the O1 optimizer. Reject compact, verbose and compiler tracing with O2. Keep O1 as the default. — Thanks @MicroProofs!
- [0e64dce4](https://github.com/orbistry/nash/commit/0e64dce4aafb56a963ded210d0c6589854ee981b) Remove the Plutus V1 and V2 targets. Nash compiles validators against the V3 script context only, so a V1 or V2 language tag produced an invalid validator. The `plutusVersion` project setting, the `--plutus-version` option, `nash_config::PlutusVersion`, `assemble_core_for_version`, `TestProgram::plutus_version` and the version parameters of `assemble_core_with_options` and `compile_tests*` are gone. Script hashes always use the V3 language tag. — Thanks @MicroProofs!

## 0.6.0 — 2026-10-07

### Minor changes

- [1ef1f880](https://github.com/orbistry/nash/commit/1ef1f880fb69765219beb822b27b59c5fecde3f2) Make trace-preserving O1 optimization the default for builds and tests, with explicit O0/O1 CLI and project settings. Share the accepted optimizer with snapshots and measurements, and include direct integer representation cancellation before ANF.
  
  Print deep UPLC term trees with an explicit work stack. — Thanks @MicroProofs!

## 0.5.0 — 2026-09-20

### Minor changes

- [9fa1d0a7](https://github.com/orbistry/nash/commit/9fa1d0a7de4c416cbaa7e45b5b56702b80046119) Add module-local unit tests and properties, scoped test dependencies, source-aware
  power assertions, deterministic seeded generation and counterexample shrinking.
  Provide `nash test` with budget checks, labels, trace controls, parallel execution,
  and terminal/JSON reports. Type-check tests with `nash check` while keeping them
  out of production builds. Add core Prop and Test support with explicit imports.
  
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

