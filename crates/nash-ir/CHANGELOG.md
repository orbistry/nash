# nash-ir

## 0.3.2 — 2026-09-25

### Patch changes

- Updated dependencies: nash-ast@0.11.0

## 0.3.1 — 2026-09-22

### Patch changes

- Updated dependencies: nash-ast@0.10.0, nash-plutus@0.3.1

## 0.3.0 — 2026-09-20

### Minor changes

- [21914bc6](https://github.com/orbistry/nash/commit/21914bc6fefd951775d9b05f15c90d8b1d36b757) Support `pair(first, second)` patterns for builtin pairs in bindings, function arguments, lambdas, and case expressions. Preserve the distinction from tuples and enforce Storable component types.
  
  Lower pair patterns to native UPLC case even when a field is ignored. Use the same Core pair case for Data constructor payloads and implement Base Pair.fst and Pair.snd with Nash patterns. — Thanks @MicroProofs!
- [09eb8f56](https://github.com/orbistry/nash/commit/09eb8f5678871a2bb861138d1962fad601e69acd) Restrict Builtin to actual Plutus operations and remove compiler cast nodes,
  cast expansion, generated validation checkers, and privileged core intrinsics.
  Implement core identity, Data encoding, decoding, and representation bridges in
  Nash using concrete builtins and existing Data patterns.
  
  Data builtins now preserve nominal Int, Bytes, List, and Map types. Core fromData
  reconstructs and validates nested values; shallow unchecked reinterpretation is
  no longer available. Existing Data wire formats are preserved. — Thanks @MicroProofs!

### Patch changes

- [9d2a2b40](https://github.com/orbistry/nash/commit/9d2a2b40090d450c685b3048a31563d61a819cc8) Embed compiler-versioned Base sources in the driver and make Prelude available automatically without a declared dependency, download, or installed source directory. Keep implicit imports out of source syntax and snapshots.
  
  Reserve `Builtin` for actual Plutus functions. Move compiler-owned types, constructors, representation traits, and unchecked `coerce` to `Primitive`, with the foundation package renamed to `nash/base`.
  
  Check bundled Base through in-process compilation tests and remove CLI subprocess tests. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.9.0, nash-plutus@0.3.0

## 0.2.0 — 2026-09-18

### Minor changes

- [ca0a5ee](https://github.com/orbistry/nash/commit/ca0a5eef111e56dfd14168bede4a785d6cd3b01f) Compile reachable solved definitions with static trait specialization, scoped field-access sharing, checked program assembly, and bounded comptime evaluation. Add exhaustive Core traversal helpers, single-wrapped CBOR encoding, and executable Vesting budget baselines. — Thanks @MicroProofs!
- [9440314](https://github.com/orbistry/nash/commit/9440314fdd5ff4433cb374128fb2610f52d77303) Add Core IR with explicit representations, UPLC text printing and checked DeBruijn conversion, executable structural lowering, recursion rewriting, complete builtin mapping, and lazy canonical type conversion for code generation. Add checked Data casts, shared pattern decision trees, evidence normalization, layout-demand analysis, and a driver callback retaining solved build state. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-ast@0.8.0, nash-plutus@0.2.0

