# nash-docs

## 0.2.2 — 2026-10-07

### Patch changes

- Updated dependencies: nash-config@0.6.0, nash-driver@0.13.0

## 0.2.1 — 2026-09-30

### Patch changes

- Updated dependencies: nash-driver@0.12.1

## 0.2.0 — 2026-09-27

### Minor changes

- [d87929ed](https://github.com/orbistry/nash/commit/d87929edf1ab8ef1fa84570b86be0f6f9514033a) Render searchable HTML and Markdown API documentation with `nash docs`, including compiler-bundled Base documentation. — Thanks @MicroProofs!
- [4858f596](https://github.com/orbistry/nash/commit/4858f5966cf26e123f2d30eeeae0a9815dedcff3) Extract public documentation from source comments and solved interfaces, and document the compiler-bundled Base API. — Thanks @MicroProofs!

### Patch changes

- [4a3ec22f](https://github.com/orbistry/nash/commit/4a3ec22ffe5e4576b7d778863455964608a32bc2) Add Primitive.map as a native pair-list alias with storable key/value types.
  Map builtins and library results preserve the alias. Library Eq implementations
  use structural Data equality for Big-element lists and Big/Big maps, and retain
  selected element equality for Little elements and mixed maps. No optimizer
  special case is needed. Generic callers with unknown element representations
  must request container Eq directly. Map equality preserves order and duplicates.
  
  Keep reflexive Lift inference nominal when checking alias identity, so explicit
  alias conversions can infer hidden type parameters without a false competitor. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.12.0, nash-can@0.13.0, nash-driver@0.12.0, nash-parse@0.9.0, nash-report@0.6.0, nash-source@0.10.0

