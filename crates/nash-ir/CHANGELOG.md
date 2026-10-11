# nash-ir

## 0.5.1 — 2026-10-10

### Patch changes

- [fe14e41d](https://github.com/orbistry/nash/commit/fe14e41d33685937c9cd354a9983342f5861ca4b) Enable bounded constant builtin evaluation in O1. Fold saturated literal calls through ANF bindings and repeat cleanup, preserving traces and runtime failures. Keep O0 and explicit comptime policies unchanged. — Thanks @MicroProofs!
- [a357f4e4](https://github.com/orbistry/nash/commit/a357f4e4d5605b78ec94de78bca33a32b15ed899) Fuse adjacent application stages in O1, clean up generated UPLC bindings, and
  pack value application spines with native Case/Constr while preserving traces,
  failure and termination. — Thanks @MicroProofs!
- [7f0e49b1](https://github.com/orbistry/nash/commit/7f0e49b183143167642597ce3a6557e94fc43397) Support read-only `.fst` and `.snd` field access and accessor functions for known builtin pair types, including aliases. Lower access directly to the pair builtins. Enable the restricted O1 pair-case rewrite only when constructor inverse cleanup removes the introduced projection, preserving strict producer evaluation. — Thanks @MicroProofs!
- [f4ad45a0](https://github.com/orbistry/nash/commit/f4ad45a000e1b021942985347384f41beaa631e9) Revisit static recursive parameter lifting after cleanup and recursive pruning so O1 reaches the same closed program in one invocation. Preserve atomic operands when lifting all-static oversaturated recursive calls and check optimizer idempotence across executable fixtures. — Thanks @MicroProofs!
- [2cc5754f](https://github.com/orbistry/nash/commit/2cc5754f5ccb9b71148d49b8c329b55769d0e3b2) Remove unused self-recursive and mutual parameters in O1 while preserving strict argument order, forwarding dependencies, type views, and delayed worker execution. — Thanks @MicroProofs!
- [056ee255](https://github.com/orbistry/nash/commit/056ee2551fc4e5518b1c2a0e8bda0944db7cc50b) Restore the recursive Core traversals: `walk`, `map`, occurrence and free-variable analysis, discard checks, hygiene, ANF, beta splicing, and Core printing. — Thanks @MicroProofs!
- Updated dependencies: nash-plutus@0.3.5

## 0.5.0 — 2026-10-07

### Minor changes

- [31f81190](https://github.com/orbistry/nash/commit/31f811909464bb3abe0b6ffca835031803c20c2f) Fold field accesses on known native constructors while preserving strict field evaluation, escaping values, and malformed runtime shapes. — Thanks @MicroProofs!
- [d362d7f6](https://github.com/orbistry/nash/commit/d362d7f64602d149d40d3545fa675b2dff6dfccc) Add an isolated integer representation cancellation pass with structural runtime-shape evidence and preserved operand evaluation. — Thanks @MicroProofs!
- [365ec7e3](https://github.com/orbistry/nash/commit/365ec7e304e52c7e8c1c8bfb66cea6da5e72dce2) Fold known native integer and byte case subjects while preserving invalid tables, runtime failures, and strict evaluation. — Thanks @MicroProofs!

### Patch changes

- [a192b185](https://github.com/orbistry/nash/commit/a192b1855a37ec9c8bd65008a348086891e57074) Render deep Core term trees with a heap work list while preserving diagnostic and snapshot formatting. — Thanks @MicroProofs!
- [1ef1f880](https://github.com/orbistry/nash/commit/1ef1f880fb69765219beb822b27b59c5fecde3f2) Make trace-preserving O1 optimization the default for builds and tests, with explicit O0/O1 CLI and project settings. Share the accepted optimizer with snapshots and measurements, and include direct integer representation cancellation before ANF.
  
  Print deep UPLC term trees with an explicit work stack. — Thanks @MicroProofs!
- [07c328f2](https://github.com/orbistry/nash/commit/07c328f276a4e36f227fae4068430dfae5d8fb49) Replace recursive Core and UPLC assembly traversals with heap work lists. Remove the O1 stack enlargement while preserving traversal order, generated code, traces, and lowering diagnostics. — Thanks @MicroProofs!
- [a14c3c6b](https://github.com/orbistry/nash/commit/a14c3c6bfc5627beb76238816c3260abdaa14440) Cancel proven integer, byte, list, map and UTF-8 representation round trips, including let-bound operands and constructor Data projections/reconstruction, while preserving validation, traces and evaluation order. Run cancellation in the O1 cleanup loop as well as before normalization. — Thanks @MicroProofs!
- Updated dependencies: nash-plutus@0.3.4

## 0.4.0 — 2026-09-30

### Minor changes

- [8874c1cf](https://github.com/orbistry/nash/commit/8874c1cf4ea9c6aa7ac8e9944df1df0d2943dc7b) Add direct lambda application reduction with strict ANF bindings and iterate it with alias propagation to a fixed point.
  
  Permit repeated-use propagation of integer and BLS constants and byte strings up to 64 bytes. — Thanks @MicroProofs!
- [11888aaa](https://github.com/orbistry/nash/commit/11888aaa4698a78f870b11206a10c9f7e61a304d) Remove unused, safe-to-discard Core bindings in the accepted optimizer cleanup loop. — Thanks @MicroProofs!
- [e2c267c4](https://github.com/orbistry/nash/commit/e2c267c4361be563c2bff1819d5d770d7209c462) Remove recursive members unreachable from their group's continuation in the accepted optimizer cleanup loop. — Thanks @MicroProofs!
- [609b1445](https://github.com/orbistry/nash/commit/609b14454e4c9bc0c3f061b5edbfb0afd9f5b212) Add single-use ANF value substitution and immediate computed-return elimination.
  
  Preserve forced builtin bindings for top-level sharing regardless of use count. — Thanks @MicroProofs!
- [2ca2fc5f](https://github.com/orbistry/nash/commit/2ca2fc5f0bcd01653aa1026d9fa617819477c7de) Add typed Core inlining for shared identity and single-builtin wrappers at fully applied direct calls, composed with the accepted binding cleanup passes. — Thanks @MicroProofs!
- [cfecc38e](https://github.com/orbistry/nash/commit/cfecc38eae60ae0b92c1fa9b849888a491e50a54) Require result types on every Core node and retain source and compiler-generated
  runtime metadata through codegen and recursion rewriting. Core operation variants
  move to CoreKind; builder APIs require result types where they cannot be derived. — Thanks @MicroProofs!
- [8e03e756](https://github.com/orbistry/nash/commit/8e03e756620f738d08850439a76ace8b9a8d9c67) Add shared Core occurrence, scope, discard-safety and structural-size analyses,
  plus capture-free substitution and binder freshening for future optimizer passes. — Thanks @MicroProofs!
- [b26ce865](https://github.com/orbistry/nash/commit/b26ce86553341f082c6abb022ab534c92aa3f0b9) Add known Boolean case folding to the Core optimizer cleanup fixed point. — Thanks @MicroProofs!
- [821a419f](https://github.com/orbistry/nash/commit/821a419f6668c301409acc96f80f171d4d312f44) Add a standalone Core pass for folding direct native-constructor cases while preserving strict field evaluation.
  
  Group Boolean and constructor folding in `known_case`, and unused-binding and recursive reachability cleanup in `dead_code`. — Thanks @MicroProofs!
- [6843bde7](https://github.com/orbistry/nash/commit/6843bde7a2dc1f34e6a5a15bcc7a837e01446f67) Add a standalone unused-parameter pass for nonrecursive helpers with exact direct calls, preserving strict argument evaluation before or after ANF. — Thanks @MicroProofs!
- [f3a7cacb](https://github.com/orbistry/nash/commit/f3a7cacb4019eb8fc33ed0cd4843279891d76034) Add a standalone Core pass for direct force/delay cancellation. — Thanks @MicroProofs!
- [98881e48](https://github.com/orbistry/nash/commit/98881e48209fcf58c2383eb055e65d6c5289a259) Add typed ANF variable-alias and nonduplicating literal propagation. — Thanks @MicroProofs!
- [1741eb00](https://github.com/orbistry/nash/commit/1741eb0038091c949f7756e61812afb90bc29c58) Add static-parameter lifting, typed A-normalization and structural invariant
  checks for Core. Preserve application staging and branch, lambda, delay and trace
  execution boundaries. Support explicitly delayed recursive workers after lifting
  all static parameters. — Thanks @MicroProofs!

## 0.3.4 — 2026-09-27

### Patch changes

- Updated dependencies: nash-ast@0.12.0, nash-plutus@0.3.3

## 0.3.3 — 2026-09-26

### Patch changes

- Updated dependencies: nash-plutus@0.3.2

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

