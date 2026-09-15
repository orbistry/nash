# nash-plutus

## 0.2.0 — 2026-09-14

### Minor changes

- [ca0a5ee](https://github.com/orbistry/nash/commit/ca0a5eef111e56dfd14168bede4a785d6cd3b01f) Compile reachable solved definitions with static trait specialization, scoped field-access sharing, checked program assembly, and bounded comptime evaluation. Add exhaustive Core traversal helpers, single-wrapped CBOR encoding, and executable Vesting budget baselines. — Thanks @MicroProofs!
- [9440314](https://github.com/orbistry/nash/commit/9440314fdd5ff4433cb374128fb2610f52d77303) Add Core IR with explicit representations, UPLC text printing and checked DeBruijn conversion, executable structural lowering, recursion rewriting, complete builtin mapping, and lazy canonical type conversion for code generation. Add checked Data casts, shared pattern decision trees, evidence normalization, layout-demand analysis, and a driver callback retaining solved build state. — Thanks @MicroProofs!

## 0.1.0 — 2026-03-15

### Minor changes

- [001497d](https://github.com/utxo-company/nash/commit/001497df610aa7cd599ba20d699d519862c835ea) Integrate `nash-plutus` (UPLC CEK machine) into the workspace.
  
  Workspace-ify dependencies, rename `uplc_turbo` to `nash_plutus`, upgrade
  thiserror v1 to v2, and replace the proc-macro test generator with a dedicated
  task crate using `quote` and `cargo fmt`. — Thanks @rvcas!

