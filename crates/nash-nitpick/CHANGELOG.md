# nash-nitpick

## 0.3.2 — 2026-09-25

### Patch changes

- Updated dependencies: nash-ast@0.11.0

## 0.3.1 — 2026-09-22

### Patch changes

- [aee8c3d2](https://github.com/orbistry/nash/commit/aee8c3d240a5a3deabfada906dda9a9e9ebfb67a) Overload bare boolean and unit expressions through FromBool and FromUnit, with
  little defaults and bundled implementations for bool, Bool, unit, and Unit.
  Like integer literals, unannotated values retain their literal constraints;
  annotate concrete entry points where needed. Qualified constructors and patterns
  retain their declared types. Add source snapshots for inference, diagnostics,
  custom conversions, and execution. — Thanks @MicroProofs!
- [3f9188b0](https://github.com/orbistry/nash/commit/3f9188b01b8bdc18363335274c20c150197d1173) Name property generation Prop throughout the compiler, Base, diagnostics and
  examples. Use direct draws returning values and PRNG states. Remove the identity
  run helper, generator map/bind functions and generator trait instances; ordinary
  Option do notation remains available for explicit state-threaded draws. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.10.0

## 0.3.0 — 2026-09-20

### Minor changes

- [21914bc6](https://github.com/orbistry/nash/commit/21914bc6fefd951775d9b05f15c90d8b1d36b757) Support `pair(first, second)` patterns for builtin pairs in bindings, function arguments, lambdas, and case expressions. Preserve the distinction from tuples and enforce Storable component types.
  
  Lower pair patterns to native UPLC case even when a field is ignored. Use the same Core pair case for Data constructor payloads and implement Base Pair.fst and Pair.snd with Nash patterns. — Thanks @MicroProofs!
- [0ec9311b](https://github.com/orbistry/nash/commit/0ec9311bf91305cf6253df3525f22c072ef346b5) Canonicalize and type-check module-local tests and property generators. Preserve
  scoped test imports and private declarations, enforce unit sequencing and
  irrefutable generator patterns, and report source-aware test diagnostics. — Thanks @MicroProofs!

### Patch changes

- [9d2a2b40](https://github.com/orbistry/nash/commit/9d2a2b40090d450c685b3048a31563d61a819cc8) Embed compiler-versioned Base sources in the driver and make Prelude available automatically without a declared dependency, download, or installed source directory. Keep implicit imports out of source syntax and snapshots.
  
  Reserve `Builtin` for actual Plutus functions. Move compiler-owned types, constructors, representation traits, and unchecked `coerce` to `Primitive`, with the foundation package renamed to `nash/base`.
  
  Check bundled Base through in-process compilation tests and remove CLI subprocess tests. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.9.0

## 0.2.2 — 2026-09-18

### Patch changes

- [fb37ab1](https://github.com/orbistry/nash/commit/fb37ab1fb480e0b1658258954855d62c2dd1a524) Canonicalize and type-check assertions, aborts, traces, and compile-time expressions while preserving source locations, dependency tracking, solved node types, and pattern coverage checks. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.8.0

## 0.2.1 — 2026-09-10

### Patch changes

- Updated dependencies: nash-ast@0.7.1, nash-region@0.3.0

## 0.2.0 — 2026-09-10

### Minor changes

- [c4b63fd](https://github.com/orbistry/nash/commit/c4b63fd02e77d5549f182ababf59fd5f1d792aa4) Add Maranget exhaustiveness and redundancy checking across declarations,
  trait defaults, impl methods and nested expressions. Render missing-pattern
  examples and handle trait-overloaded literals conservatively.
  Reject modules with incomplete or redundant patterns after type solving,
  before publishing interfaces or retaining solved modules for dependents. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-ast@0.7.0

