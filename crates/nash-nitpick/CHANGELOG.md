# nash-nitpick

## 0.2.2 — 2026-09-14

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

