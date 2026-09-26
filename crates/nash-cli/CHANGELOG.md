# nash-cli

## 0.6.3 — 2026-09-26

### Patch changes

- Updated dependencies: nash-codegen@0.4.2, nash-driver@0.11.0, nash-language-server@0.4.5, nash-report@0.5.3, nash-test@0.4.1

## 0.6.2 — 2026-09-25

### Patch changes

- Updated dependencies: nash-codegen@0.4.1, nash-driver@0.10.0, nash-language-server@0.4.4, nash-report@0.5.2, nash-test@0.4.0

## 0.6.1 — 2026-09-22

### Patch changes

- Updated dependencies: nash-codegen@0.4.0, nash-driver@0.9.0, nash-language-server@0.4.3, nash-report@0.5.1, nash-test@0.3.0

## 0.6.0 — 2026-09-20

### Minor changes

- [9fa1d0a7](https://github.com/orbistry/nash/commit/9fa1d0a7de4c416cbaa7e45b5b56702b80046119) Add module-local unit tests and properties, scoped test dependencies, source-aware
  power assertions, deterministic seeded generation and counterexample shrinking.
  Provide `nash test` with budget checks, labels, trace controls, parallel execution,
  and terminal/JSON reports. Type-check tests with `nash check` while keeping them
  out of production builds. Add core Prop and Test support with explicit imports.
  
  Preserve short-circuit evaluation for the core boolean infix operators, including
  inside instrumented assertions. — Thanks @MicroProofs!

### Patch changes

- [7d8979fa](https://github.com/orbistry/nash/commit/7d8979fa62fcf3fa2b4d40994873b2f6a50d267e) Target protocol 11 and UPLC 1.1.0 for all supported ledger languages. Lower
  conditionals and boolean/list matches to native case terms, and dispatch Data
  branches through a chooseData tag and native case. Preserve branch laziness and
  single evaluation of scrutinees.
  
  Enable protocol-11 builtin and constant validation, correct constant-case branch
  limits and UTF-8 string costing, and encode nested Data constants without panics.
  Bundled evaluator costs remain estimates rather than live ledger parameters. — Thanks @MicroProofs!
- [9d2a2b40](https://github.com/orbistry/nash/commit/9d2a2b40090d450c685b3048a31563d61a819cc8) Embed compiler-versioned Base sources in the driver and make Prelude available automatically without a declared dependency, download, or installed source directory. Keep implicit imports out of source syntax and snapshots.
  
  Reserve `Builtin` for actual Plutus functions. Move compiler-owned types, constructors, representation traits, and unchecked `coerce` to `Primitive`, with the foundation package renamed to `nash/base`.
  
  Check bundled Base through in-process compilation tests and remove CLI subprocess tests. — Thanks @MicroProofs!
- Updated dependencies: nash-codegen@0.3.0, nash-config@0.5.0, nash-driver@0.8.0, nash-language-server@0.4.2, nash-report@0.5.0, nash-test@0.2.0

## 0.5.0 — 2026-09-18

### Minor changes

- [1650217](https://github.com/orbistry/nash/commit/165021768b6e9a7d0737a3f31c817a73bd2ba60f) Add validator entry-point diagnostics and build solved modules into Plutus V3 scripts with `nash build` output in UPLC, Flat, and single-wrapped CBOR formats. — Thanks @MicroProofs!
- [bf78f35](https://github.com/orbistry/nash/commit/bf78f35258de7823f8d147f85b12aba1ed57b783) Complete validator builds with project and CLI target/trace settings, production
  test-block exclusion, protocol-10 target validation, and verified script hashes.
  Write single-wrapped CBOR as hex text instead of binary and track generated
  artifacts for safe stale-output cleanup. Existing output directories without an
  ownership manifest must be cleared of colliding artifacts or replaced with a
  fresh output directory. Optimizer settings remain unavailable. — Thanks @MicroProofs!

### Patch changes

- [6a9afd5](https://github.com/orbistry/nash/commit/6a9afd52121a3ab7a511d0277c27fdfdaea9c82a) Display diagnostic source paths relative to the loaded workspace or package root, with absolute file hyperlinks on supported terminals, including related diagnostics. Preserve miette diagnostic-code headers and severity markers, and retain absolute source identities for JSON and editor clients. — Thanks @MicroProofs!
- [3cb7a0f](https://github.com/orbistry/nash/commit/3cb7a0f1ec352bf13522e1eb71f0100ff873d3df) Preserve miette diagnostic codes and error/warning markers in terminal output. Show source paths relative to the project root while keeping file hyperlinks absolute.
  
  Expand colorless rendered diagnostic snapshot coverage across parser, canonicalizer, solver, driver, and CLI tests. Record Nash source instead of Rust assertion expressions in source-driven snapshots, move codegen snapshots to source-compilation tests, and check snapshot metadata for regressions. Keep direct assertions for internal error values and hand-built Core behavior. — Thanks @MicroProofs!
- Updated dependencies: nash-codegen@0.2.0, nash-config@0.4.0, nash-driver@0.7.0, nash-language-server@0.4.1, nash-plutus@0.2.0, nash-report@0.4.0

## 0.4.0 — 2026-09-10

### Minor changes

- [e3e72a7](https://github.com/orbistry/nash/commit/e3e72a7432f96f2de01777edd6c5a935e88d48fb) Use concise diagnostics with full expected/actual type comparisons, expectation-origin labels, and stable codes independent of display titles. Retain parser opening positions for closing-delimiter reports. Support arbitrary secondary labels and related reports across source files.
  
  Extend diagnostic JSON with code, severity, labels, suggestions, and related reports. JSON messages now contain styled prose without embedded source drawings; consumers should render the structured labels. LSP diagnostic codes now use stable identifiers instead of titles and include secondary and related source locations. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-driver@0.6.0, nash-language-server@0.4.0, nash-report@0.3.0

## 0.3.0 — 2026-09-10

### Minor changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Collect independent compiler errors with dependency-aware recovery, retain failed module dependencies, and render owned diagnostics in the terminal, JSON, and language server. Preserve trait-method call names in error context. Add JSON and warning controls to `nash check`, and publish diagnostics for unsaved editor buffers with UTF-16 ranges. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-driver@0.5.0, nash-language-server@0.3.0, nash-report@0.2.0

## 0.2.6 — 2026-09-10

### Patch changes

- [c4b63fd](https://github.com/orbistry/nash/commit/c4b63fd02e77d5549f182ababf59fd5f1d792aa4) Add Maranget exhaustiveness and redundancy checking across declarations,
  trait defaults, impl methods and nested expressions. Render missing-pattern
  examples and handle trait-overloaded literals conservatively.
  Reject modules with incomplete or redundant patterns after type solving,
  before publishing interfaces or retaining solved modules for dependents. — Thanks @MicroProofs!
- [adf37e3](https://github.com/orbistry/nash/commit/adf37e3ead0ba83dbe2690252687294603f487ba) Replace row polymorphism with nominal record aliases. Resolve literals by
  visible field sets, preserve declaration-order metadata, and resolve record
  operations before generalization. Support qualified lowercase type names and
  alias constructor functions while preserving trait-based literals and
  representation predicates.
  Use the complete primitive type inventory under the Builtin qualifier and
  represent unit uniformly as a named builtin type throughout inference and
  instance selection.
  Preserve labeled constructor metadata, support construction and pattern sugar,
  and permit field projection only through visible single-constructor unions.
  Keep parenthesized record literals as positional constructor arguments. — Thanks @MicroProofs!
- Updated dependencies: nash-driver@0.4.0

## 0.2.5 — 2026-09-08

### Patch changes

- [3495cc5](https://github.com/orbistry/nash/commit/3495cc5c755ca5b81c315a1e5358df1888db31c3) Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
  requirements through separate representation predicates and inline
  representation annotations. Infer datatype contexts with a terminating SCC
  worklist and enforce them at declarations and local or imported uses.
  
  Preserve higher-kinded and partial alias applications, captured variables,
  head-only impl coherence, superclass evidence and literal defaulting. Export
  closed kinds and ordered predicate contexts through interfaces. Use
  elementwise builtin-list Eq, structural Big Eq and reflexive Lift. — Thanks @MicroProofs!
- [dc3d24f](https://github.com/orbistry/nash/commit/dc3d24fe03d7909f0af693e8a6624f64992ceac2) Carry discovered package ownership into canonicalization and imported
  interfaces so core literal defaulting works through the CLI. Preserve
  application identities and reject conflicting ownership of a source URI. — Thanks @MicroProofs!
- Updated dependencies: nash-driver@0.3.0

## 0.2.4 — 2026-09-05

### Patch changes

- Updated dependencies: nash-driver@0.2.3

## 0.2.3 — 2026-09-05

### Patch changes

- Updated dependencies: nash-driver@0.2.2

## 0.2.2 — 2026-08-15

### Patch changes

- Updated dependencies: nash-driver@0.2.1

## 0.2.1 — 2026-08-08

### Patch changes

- [1c5f4a8](https://github.com/utxo-company/nash/commit/1c5f4a803a5f6f7f8178fc02d8edbd89bf234c32) Fix the compiler version proxy: guard against a cached binary whose version doesn't match its folder name exec'ing itself in a silent infinite loop (now a clear error via the `NASH_PROXY_VERSION` env var handshake), and fix the release asset name to match what cargo-dist actually publishes (`nash-cli-{target}` instead of `nash-{target}`), which made every proxy download fail. — Thanks @rvcas!
- Updated dependencies: nash-driver@0.2.0

## 0.2.0 — 2026-03-10

### Minor changes

- [45b79ae](https://github.com/utxo-company/nash/commit/45b79ae1c94701b4f16a612e381a57c986b099e0) Add the minimal Nash language server skeleton and CLI entry point for running it over stdio.
  
  Includes commit `f2b7657c1fa70a22dc2ac450ec54cc0623bd2f3f` and the follow-up cleanup to the initial LSP scaffolding. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-language-server@0.2.0

## 0.1.3 — 2026-02-20

### Patch changes

- [8a40dbe](https://github.com/nash-script/compiler/commit/8a40dbe43869de6453b6a7bb5dce04d3b900efcf) Add module-level doc comments to lib.rs. — Thanks @rvcas!

## 0.1.2 — 2026-02-20

### Patch changes

- [5cc62cc](https://github.com/nash-script/compiler/commit/5cc62cc9ce5568bb779f542b872909dc71942578) Add explicit about text to CLI help output. — Thanks @rvcas!

## 0.1.1 — 2026-02-20

### Patch changes

- [c46f722](https://github.com/nash-script/compiler/commit/c46f72228fb027163b0590fa9075edc8eeff39c7) Unify into a single `nash` binary with clap for argument parsing, octocrab for downloading missing compiler versions from GitHub Releases, and automatic version proxying before clap runs. — Thanks @rvcas!
- Updated dependencies: nash-config@0.3.0, nash-driver@0.1.1

