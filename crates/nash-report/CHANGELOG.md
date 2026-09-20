# nash-report

## 0.5.0 — 2026-09-20

### Minor changes

- [0ec9311b](https://github.com/orbistry/nash/commit/0ec9311bf91305cf6253df3525f22c072ef346b5) Canonicalize and type-check module-local tests and property generators. Preserve
  scoped test imports and private declarations, enforce unit sequencing and
  irrefutable generator patterns, and report source-aware test diagnostics. — Thanks @MicroProofs!

### Patch changes

- [21914bc6](https://github.com/orbistry/nash/commit/21914bc6fefd951775d9b05f15c90d8b1d36b757) Support `pair(first, second)` patterns for builtin pairs in bindings, function arguments, lambdas, and case expressions. Preserve the distinction from tuples and enforce Storable component types.
  
  Lower pair patterns to native UPLC case even when a field is ignored. Use the same Core pair case for Data constructor payloads and implement Base Pair.fst and Pair.snd with Nash patterns. — Thanks @MicroProofs!
- [352361ba](https://github.com/orbistry/nash/commit/352361baf35943117863225aca6a9d99ab5bdf92) Use one coercion-based `ToData` implementation for every Big type, including user-defined types and nested collections. Remove redundant recursive encoding implementations. Permit blanket impls in the trait's defining module while retaining overlap checks; existing concrete `ToData` impls must be removed because they overlap the blanket. — Thanks @MicroProofs!
- [9d2a2b40](https://github.com/orbistry/nash/commit/9d2a2b40090d450c685b3048a31563d61a819cc8) Embed compiler-versioned Base sources in the driver and make Prelude available automatically without a declared dependency, download, or installed source directory. Keep implicit imports out of source syntax and snapshots.
  
  Reserve `Builtin` for actual Plutus functions. Move compiler-owned types, constructors, representation traits, and unchecked `coerce` to `Primitive`, with the foundation package renamed to `nash/base`.
  
  Check bundled Base through in-process compilation tests and remove CLI subprocess tests. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.9.0, nash-can@0.9.0, nash-constrain@0.7.0, nash-nitpick@0.3.0, nash-parse@0.7.0, nash-source@0.8.0

## 0.4.0 — 2026-09-18

### Minor changes

- [1650217](https://github.com/orbistry/nash/commit/165021768b6e9a7d0737a3f31c817a73bd2ba60f) Add validator entry-point diagnostics and build solved modules into Plutus V3 scripts with `nash build` output in UPLC, Flat, and single-wrapped CBOR formats. — Thanks @MicroProofs!

### Patch changes

- [6a9afd5](https://github.com/orbistry/nash/commit/6a9afd52121a3ab7a511d0277c27fdfdaea9c82a) Display diagnostic source paths relative to the loaded workspace or package root, with absolute file hyperlinks on supported terminals, including related diagnostics. Preserve miette diagnostic-code headers and severity markers, and retain absolute source identities for JSON and editor clients. — Thanks @MicroProofs!
- [3cb7a0f](https://github.com/orbistry/nash/commit/3cb7a0f1ec352bf13522e1eb71f0100ff873d3df) Preserve miette diagnostic codes and error/warning markers in terminal output. Show source paths relative to the project root while keeping file hyperlinks absolute.
  
  Expand colorless rendered diagnostic snapshot coverage across parser, canonicalizer, solver, driver, and CLI tests. Record Nash source instead of Rust assertion expressions in source-driven snapshots, move codegen snapshots to source-compilation tests, and check snapshot metadata for regressions. Keep direct assertions for internal error values and hand-built Core behavior. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.8.0, nash-can@0.8.0, nash-constrain@0.6.0, nash-nitpick@0.2.2, nash-parse@0.6.1, nash-solve@0.6.0

## 0.3.0 — 2026-09-10

### Minor changes

- [e3e72a7](https://github.com/orbistry/nash/commit/e3e72a7432f96f2de01777edd6c5a935e88d48fb) Use concise diagnostics with full expected/actual type comparisons, expectation-origin labels, and stable codes independent of display titles. Retain parser opening positions for closing-delimiter reports. Support arbitrary secondary labels and related reports across source files.
  
  Extend diagnostic JSON with code, severity, labels, suggestions, and related reports. JSON messages now contain styled prose without embedded source drawings; consumers should render the structured labels. LSP diagnostic codes now use stable identifiers instead of titles and include secondary and related source locations. — Thanks @MicroProofs!
- [8bb97f6](https://github.com/orbistry/nash/commit/8bb97f680047cc8818b890af407247cdd585000e) Use source-sized coordinates and diagnostic widths throughout parsing and reporting. Check LSP coordinate conversion instead of truncating. Reject oversized Unicode escapes without integer overflow, and make arbitrary lookahead offsets safe. — Thanks @MicroProofs!

### Patch changes

- [9977e60](https://github.com/orbistry/nash/commit/9977e607c8b870f448f5251a19cc9bf6d1c043f6) Report excessive expression, pattern, and type nesting before stack exhaustion. Parse flat sequences and nested comments with loops. — Thanks @MicroProofs!
- [0ed0c75](https://github.com/orbistry/nash/commit/0ed0c75a0ac421a193a420c116cd2284ba922a25) Infer directly from the canonical AST into the existing union-find and predicate engine. Remove the allocated constraint tree and intermediate inference Type, preserving schemes, evidence, rank ownership, recursive-group sequencing, and complete diagnostics. Pass canonical modules directly to the solver. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.7.1, nash-can@0.7.0, nash-constrain@0.5.0, nash-nitpick@0.2.1, nash-parse@0.6.0, nash-region@0.3.0, nash-solve@0.5.0, nash-source@0.7.0

## 0.2.0 — 2026-09-10

### Minor changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Add compiler report documents, source spans, terminal rendering and name suggestions. Expose parser token classifiers and preserve nested parse errors for diagnostics. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-can@0.6.1, nash-constrain@0.4.1, nash-parse@0.5.1, nash-solve@0.4.1

