# nash-codegen

## 0.2.0 — 2026-09-18

### Minor changes

- [ca0a5ee](https://github.com/orbistry/nash/commit/ca0a5eef111e56dfd14168bede4a785d6cd3b01f) Compile reachable solved definitions with static trait specialization, scoped field-access sharing, checked program assembly, and bounded comptime evaluation. Add exhaustive Core traversal helpers, single-wrapped CBOR encoding, and executable Vesting budget baselines. — Thanks @MicroProofs!
- [9440314](https://github.com/orbistry/nash/commit/9440314fdd5ff4433cb374128fb2610f52d77303) Add Core IR with explicit representations, UPLC text printing and checked DeBruijn conversion, executable structural lowering, recursion rewriting, complete builtin mapping, and lazy canonical type conversion for code generation. Add checked Data casts, shared pattern decision trees, evidence normalization, layout-demand analysis, and a driver callback retaining solved build state. — Thanks @MicroProofs!
- [bf78f35](https://github.com/orbistry/nash/commit/bf78f35258de7823f8d147f85b12aba1ed57b783) Complete validator builds with project and CLI target/trace settings, production
  test-block exclusion, protocol-10 target validation, and verified script hashes.
  Write single-wrapped CBOR as hex text instead of binary and track generated
  artifacts for safe stale-output cleanup. Existing output directories without an
  ownership manifest must be cleared of colliding artifacts or replaced with a
  fresh output directory. Optimizer settings remain unavailable. — Thanks @MicroProofs!

### Patch changes

- [4d27cd2](https://github.com/orbistry/nash/commit/4d27cd200f4473f503eaa2212325fa479c1a268c) Add Nash source-to-UPLC snapshot tests for collections, pattern matching,
  data conversions, traces, and validators. — Thanks @MicroProofs!
- [3cb7a0f](https://github.com/orbistry/nash/commit/3cb7a0f1ec352bf13522e1eb71f0100ff873d3df) Preserve miette diagnostic codes and error/warning markers in terminal output. Show source paths relative to the project root while keeping file hyperlinks absolute.
  
  Expand colorless rendered diagnostic snapshot coverage across parser, canonicalizer, solver, driver, and CLI tests. Record Nash source instead of Rust assertion expressions in source-driven snapshots, move codegen snapshots to source-compilation tests, and check snapshot metadata for regressions. Keep direct assertions for internal error values and hand-built Core behavior. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.8.0, nash-can@0.8.0, nash-constrain@0.6.0, nash-ir@0.2.0, nash-nitpick@0.2.2, nash-parse@0.6.1, nash-plutus@0.2.0, nash-solve@0.6.0

