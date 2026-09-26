# nash-codegen

## 0.4.2 — 2026-09-26

### Patch changes

- Updated dependencies: nash-can@0.12.0, nash-ir@0.3.3, nash-plutus@0.3.2, nash-solve@0.9.1, nash-test@0.4.1

## 0.4.1 — 2026-09-25

### Patch changes

- [95d828a2](https://github.com/orbistry/nash/commit/95d828a2602340ace632a79f28f74e49c54d2755) Distinguish implementation heads by compiler-owned representation classes. Allow disjoint Big and Little blankets, preserve representation bounds during selection and evidence resolution, and accept inline Little bounds. — Thanks @MicroProofs!
- [feeb1edb](https://github.com/orbistry/nash/commit/feeb1edbecdb0b402023d99d5867e671eb57b336) Use nested Choice/Group traces for property generation, strict replay, and reduction. Compose generator functions through ordinary Functor, Applicative, and Monad instances in Nash. Preserve reduced replay trees in runner outcomes. Retain concrete little type layouts when specializing generic trait helpers. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.11.0, nash-can@0.11.0, nash-ir@0.3.2, nash-solve@0.9.0, nash-test@0.4.0

## 0.4.0 — 2026-09-22

### Minor changes

- [aee8c3d2](https://github.com/orbistry/nash/commit/aee8c3d240a5a3deabfada906dda9a9e9ebfb67a) Overload bare boolean and unit expressions through FromBool and FromUnit, with
  little defaults and bundled implementations for bool, Bool, unit, and Unit.
  Like integer literals, unannotated values retain their literal constraints;
  annotate concrete entry points where needed. Qualified constructors and patterns
  retain their declared types. Add source snapshots for inference, diagnostics,
  custom conversions, and execution. — Thanks @MicroProofs!
- [dccae3df](https://github.com/orbistry/nash/commit/dccae3df5332b8d5628abb785b8b6b361d0974c1) Use a function alias for generators, returning value and next PRNG state per draw.
  Remove the redundant replay count. Prepare properties once and retain their
  state, body function, and deferred display function without rerunning generators.
  Preserve function aliases in typed definitions and captured variables inside
  native constructor and case nodes when returning CEK closures. — Thanks @MicroProofs!

### Patch changes

- [24a661d6](https://github.com/orbistry/nash/commit/24a661d674d7724d28938f8bf4205507782eb10d) Reuse list tails across Big field projections and preserve unchanged suffixes when updating Big records. — Thanks @MicroProofs!
- [4bdf1519](https://github.com/orbistry/nash/commit/4bdf15195e16c5b45e4ee5e11cc624536da3b347) Use declared Big record field counts to reuse adjacent tails without requiring earlier field reads. — Thanks @MicroProofs!
- [b3699dba](https://github.com/orbistry/nash/commit/b3699dba5f0564852f52df963f06767e48bea643) Use shared native list and pair cases for field extraction and Base traversal.
  Reuse case-bound tails across adjacent accesses and use dropList for remaining
  gaps of two or more, preserving evaluation order and record update suffixes. — Thanks @MicroProofs!
- [9a7fe0ec](https://github.com/orbistry/nash/commit/9a7fe0ec015df3791346358027a472b8f5f37b8b) Move property sampling, rejection, tuple preparation and deferred body/display
  functions into Nash Test helpers. Keep codegen responsible for source callbacks
  and pattern binding. Compose assertion capture traces through Nash helpers while
  preserving failure-only evaluation and compiler trace settings. Base Test uses
  Builtin.trace directly. Allow keyword-named qualified references in source,
  removing the compiler special case for Test-module traces. — Thanks @MicroProofs!
- [737b7237](https://github.com/orbistry/nash/commit/737b7237c957baf048c7e956c5b473e9c7105a08) Trust solved constructor layouts when compiling patterns. Remove Data variant
  checks for typed Big unions, omit dispatch for single-constructor types, and use
  native integer case dispatch for dense constructor tags. Preserve strict
  scrutinee evaluation and explicit matches on unrestricted Data. Add Nash source,
  Core, UPLC, and execution snapshots for Bool, Unit, and nested constructors. — Thanks @MicroProofs!
- [3f9188b0](https://github.com/orbistry/nash/commit/3f9188b01b8bdc18363335274c20c150197d1173) Name property generation Prop throughout the compiler, Base, diagnostics and
  examples. Use direct draws returning values and PRNG states. Remove the identity
  run helper, generator map/bind functions and generator trait instances; ordinary
  Option do notation remains available for explicit state-threaded draws. — Thanks @MicroProofs!
- [402557e5](https://github.com/orbistry/nash/commit/402557e547f9496fb1fc73d4203433bcb25df0d4) Complete Base type helpers with Big or little inputs and little outer results.
  Preserve payload types in Lift conversions, support identity conversion for all
  types, and keep mixed-representation boolean calls lazy. Add source snapshots
  for helper behavior and rejected recursive conversions. — Thanks @MicroProofs!
- [76a7a02e](https://github.com/orbistry/nash/commit/76a7a02ef1bfc0a03082a453fcdafc727c8cb160) Skip payload decoding for ignored Data-pattern fields. Use direct Data
  unwrappers in Base validation implementations, preserving their failure behavior
  and existing element validation without redundant Data variant dispatch. — Thanks @MicroProofs!
- [ba13694e](https://github.com/orbistry/nash/commit/ba13694e2254c7a5d477a68326b1db380e6eeede) Preserve short-circuiting for fully applied Bool.and and Bool.or calls, including assertions. Correct Show Data constructor formatting and add executable snapshots for bundled Base traits and operators. — Thanks @MicroProofs!
- [4243f95b](https://github.com/orbistry/nash/commit/4243f95bf78e201ced7713f076b04f185a17450e) Skip ignored Big constructor and record field projections during pattern
  lowering. Reuse list tails between required fields and avoid reconstructing
  unused Big-list tails while preserving strict scrutinee evaluation. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.10.0, nash-can@0.10.0, nash-ir@0.3.1, nash-plutus@0.3.1, nash-solve@0.8.0, nash-test@0.3.0

## 0.3.0 — 2026-09-20

### Minor changes

- [21914bc6](https://github.com/orbistry/nash/commit/21914bc6fefd951775d9b05f15c90d8b1d36b757) Support `pair(first, second)` patterns for builtin pairs in bindings, function arguments, lambdas, and case expressions. Preserve the distinction from tuples and enforce Storable component types.
  
  Lower pair patterns to native UPLC case even when a field is ignored. Use the same Core pair case for Data constructor payloads and implement Base Pair.fst and Pair.snd with Nash patterns. — Thanks @MicroProofs!
- [4de7fb4b](https://github.com/orbistry/nash/commit/4de7fb4bf2a87ff059f934a7b9c99b2029cbaee9) Rename `FromData.validateData` to `FromData.validate`. Update method definitions, imports, and calls to use `validate`; validation behavior is unchanged. — Thanks @MicroProofs!
- [352361ba](https://github.com/orbistry/nash/commit/352361baf35943117863225aca6a9d99ab5bdf92) Use one coercion-based `ToData` implementation for every Big type, including user-defined types and nested collections. Remove redundant recursive encoding implementations. Permit blanket impls in the trait's defining module while retaining overlap checks; existing concrete `ToData` impls must be removed because they overlap the blanket. — Thanks @MicroProofs!
- [09eb8f56](https://github.com/orbistry/nash/commit/09eb8f5678871a2bb861138d1962fad601e69acd) Restrict Builtin to actual Plutus operations and remove compiler cast nodes,
  cast expansion, generated validation checkers, and privileged core intrinsics.
  Implement core identity, Data encoding, decoding, and representation bridges in
  Nash using concrete builtins and existing Data patterns.
  
  Data builtins now preserve nominal Int, Bytes, List, and Map types. Core fromData
  reconstructs and validates nested values; shallow unchecked reinterpretation is
  no longer available. Existing Data wire formats are preserved. — Thanks @MicroProofs!
- [a95b6910](https://github.com/orbistry/nash/commit/a95b6910d1867463cac0d1030599ea5bc6bfdf71) Provide unchecked `FromData` conversion for every Big type through a blanket coercion impl. Move checked `validate` into the separate `Validate` trait; migrate validation impls and trait imports to `Validate`. Unchecked conversion no longer requires validation instances for a type or its fields. — Thanks @MicroProofs!
- [9fa1d0a7](https://github.com/orbistry/nash/commit/9fa1d0a7de4c416cbaa7e45b5b56702b80046119) Add module-local unit tests and properties, scoped test dependencies, source-aware
  power assertions, deterministic seeded generation and counterexample shrinking.
  Provide `nash test` with budget checks, labels, trace controls, parallel execution,
  and terminal/JSON reports. Type-check tests with `nash check` while keeping them
  out of production builds. Add core Prop and Test support with explicit imports.
  
  Preserve short-circuit evaluation for the core boolean infix operators, including
  inside instrumented assertions. — Thanks @MicroProofs!
- [a219dde2](https://github.com/orbistry/nash/commit/a219dde26f6e582c5305968f21d675da931e5779) Make Data.Constr accept one `pair int (list Data)` payload in expressions and patterns. Require explicit pair destructuring for the tag and fields, lowered with native UPLC case. Update Base helpers and source snapshots. — Thanks @MicroProofs!
- [e69fd3cc](https://github.com/orbistry/nash/commit/e69fd3ccf423c921fbaaecf243f54ce03a83a2cb) Add `Builtin.coerce : 'a -> 'b` as an unchecked, representation-preserving function. Core `FromData.fromData` now defaults to unchecked coercion; use `validate` to reject malformed scalar and nested collection data. — Thanks @MicroProofs!
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

### Patch changes

- [c2d806a0](https://github.com/orbistry/nash/commit/c2d806a00001b172567a9ca3c51c2dc07ec307cc) Lower mutually recursive functions using native constructor packets and case dispatch instead of selector lambdas. Preserve mixed arities, partial applications, lexical captures and lazy branch selection. — Thanks @MicroProofs!
- [3ec1a9d9](https://github.com/orbistry/nash/commit/3ec1a9d9109cee2248fde434cbda4055763013bf) Lower boolean and list matches directly to native case without an extra scrutinee binding. Dispatch Data shapes directly with lazy chooseData branches, removing the manufactured integer tag and second dispatch. Destructure unConstrData pairs with native case. — Thanks @MicroProofs!
- [c985eecb](https://github.com/orbistry/nash/commit/c985eecb822877ccda3b9ae487042f806fd3391b) Define `toData` and `fromData` directly in their blanket impls using inline Big bounds, instead of trait defaults. Conversion behavior is unchanged. — Thanks @MicroProofs!
- [bdde6c9f](https://github.com/orbistry/nash/commit/bdde6c9f1491d1dd07339d8b1fcfc5f62761d46e) Use dropList for Big record and constructor field offsets of two or more, retaining tailList for offset one. Share repeated constant-offset projections within their evaluation scope. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.9.0, nash-can@0.9.0, nash-config@0.5.0, nash-ir@0.3.0, nash-plutus@0.3.0, nash-solve@0.7.0, nash-test@0.2.0

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

