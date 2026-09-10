# nash-cli

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

