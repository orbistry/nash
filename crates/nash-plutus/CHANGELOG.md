# nash-plutus

## 0.3.1 — 2026-09-22

### Patch changes

- [dccae3df](https://github.com/orbistry/nash/commit/dccae3df5332b8d5628abb785b8b6b361d0974c1) Use a function alias for generators, returning value and next PRNG state per draw.
  Remove the redundant replay count. Prepare properties once and retain their
  state, body function, and deferred display function without rerunning generators.
  Preserve function aliases in typed definitions and captured variables inside
  native constructor and case nodes when returning CEK closures. — Thanks @MicroProofs!

## 0.3.0 — 2026-09-20

### Minor changes

- [7d8979fa](https://github.com/orbistry/nash/commit/7d8979fa62fcf3fa2b4d40994873b2f6a50d267e) Target protocol 11 and UPLC 1.1.0 for all supported ledger languages. Lower
  conditionals and boolean/list matches to native case terms, and dispatch Data
  branches through a chooseData tag and native case. Preserve branch laziness and
  single evaluation of scrutinees.
  
  Enable protocol-11 builtin and constant validation, correct constant-case branch
  limits and UTF-8 string costing, and encode nested Data constants without panics.
  Bundled evaluator costs remain estimates rather than live ledger parameters. — Thanks @MicroProofs!

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

## 0.1.0 — 2026-03-15

### Minor changes

- [001497d](https://github.com/utxo-company/nash/commit/001497df610aa7cd599ba20d699d519862c835ea) Integrate `nash-plutus` (UPLC CEK machine) into the workspace.
  
  Workspace-ify dependencies, rename `uplc_turbo` to `nash_plutus`, upgrade
  thiserror v1 to v2, and replace the proc-macro test generator with a dedicated
  task crate using `quote` and `cargo fmt`. — Thanks @rvcas!

