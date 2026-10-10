# nash-plutus

## 0.3.5 — 2026-10-10

### Patch changes

- [5ee70cc4](https://github.com/orbistry/nash/commit/5ee70cc4da311a8f237db47350f8b9418c7b3729) Restore the recursive Flat term encoder and decoder. `nash_test::Config` gains `worker_stack`, and `nash test` runs its workers on the compiler stack. — Thanks @MicroProofs!
- [f4c2c37e](https://github.com/orbistry/nash/commit/f4c2c37e46430ad7c830a884bb8b0179a18c7bb1) Restore the recursive UPLC term printer, De Bruijn conversion, and target validation. — Thanks @MicroProofs!
- [7687f06d](https://github.com/orbistry/nash/commit/7687f06d3a80dee9491899bf96a2346eb3833d3a) Encode and decode negative Data integers below `-(2^64 - 1)` as the Haskell node does: CBOR tag 3 holds `-1 - n`, and `-2^64` is a plain negative integer. — Thanks @MicroProofs!
- [ee7fd341](https://github.com/orbistry/nash/commit/ee7fd341614d8f6dadda7616c47c421a84c9d128) Run constant folding, unused-parameter removal and cleanup until unchanged without optimizer resource or output-growth limits. Evaluate all pure representable builtin calls directly and return errors instead of panicking for out-of-range constant indices and constructor tags. Correct whole-byte left shifts and handle arbitrary-size shift, rotate and list-drop inputs. — Thanks @MicroProofs!

## 0.3.4 — 2026-10-07

### Patch changes

- [1ef1f880](https://github.com/orbistry/nash/commit/1ef1f880fb69765219beb822b27b59c5fecde3f2) Make trace-preserving O1 optimization the default for builds and tests, with explicit O0/O1 CLI and project settings. Share the accepted optimizer with snapshots and measurements, and include direct integer representation cancellation before ANF.
  
  Print deep UPLC term trees with an explicit work stack. — Thanks @MicroProofs!
- [07c328f2](https://github.com/orbistry/nash/commit/07c328f276a4e36f227fae4068430dfae5d8fb49) Replace recursive Core and UPLC assembly traversals with heap work lists. Remove the O1 stack enlargement while preserving traversal order, generated code, traces, and lowering diagnostics. — Thanks @MicroProofs!

## 0.3.3 — 2026-09-27

### Patch changes

- [44e22412](https://github.com/orbistry/nash/commit/44e22412bfb8d60baeb600dbccfc41a71c134e0c) Remove completed performance experiments, benchmark fixtures and unused benchmark dependencies while retaining functional coverage. — Thanks @MicroProofs!
- [70a160a9](https://github.com/orbistry/nash/commit/70a160a916b2803b6d12d5376e44ead684632986) Decode deeply nested Flat terms with an explicit stack, avoiding host stack overflow for large generated programs. — Thanks @MicroProofs!

## 0.3.2 — 2026-09-26

### Patch changes

- [d7301b1f](https://github.com/orbistry/nash/commit/d7301b1f5d833f84301bfdc7e58345701f2c2fd9) Consolidate Data conversion and checking in traits. Allow explicit little-type
  ToData and FromData implementations alongside Big-only identity blankets. Add
  independent optional Decode instances, including Cardano V3 types, and remove
  the separate encoder and decoder combinator modules.
  
  Encode Flat terms iteratively so large context decoders do not exhaust the host stack. — Thanks @MicroProofs!

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

