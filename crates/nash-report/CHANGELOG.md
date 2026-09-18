# nash-report

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

