# nash-parse

## 0.5.0 — 2026-09-10

### Minor changes

- [adf37e3](https://github.com/orbistry/nash/commit/adf37e3ead0ba83dbe2690252687294603f487ba) Replace row polymorphism with nominal record aliases. Resolve literals by
  visible field sets, preserve declaration-order metadata, and resolve record
  operations before generalization. Support qualified lowercase type names and
  alias constructor functions while preserving trait-based literals and
  representation predicates.
  Use the complete primitive type inventory under the Builtin qualifier and
  represent unit uniformly as a named builtin type throughout inference and
  instance selection.
  Preserve labeled constructor metadata, support construction and pattern sugar,
  and permit field projection only through visible single-constructor unions.
  Keep parenthesized record literals as positional constructor arguments. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-source@0.6.0

## 0.4.0 — 2026-09-08

### Minor changes

- [3495cc5](https://github.com/orbistry/nash/commit/3495cc5c755ca5b81c315a1e5358df1888db31c3) Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
  requirements through separate representation predicates and inline
  representation annotations. Infer datatype contexts with a terminating SCC
  worklist and enforce them at declarations and local or imported uses.
  
  Preserve higher-kinded and partial alias applications, captured variables,
  head-only impl coherence, superclass evidence and literal defaulting. Export
  closed kinds and ordered predicate contexts through interfaces. Use
  elementwise builtin-list Eq, structural Big Eq and reflexive Lift. — Thanks @MicroProofs!
- [600364d](https://github.com/orbistry/nash/commit/600364da0123550f74f40fad20dd016b4acd2bfd) Accept dollar signs in operators so the specified Functor map operator `<$>`
  can be declared and used by core Prelude. — Thanks @MicroProofs!

### Patch changes

- [84e8b7e](https://github.com/orbistry/nash/commit/84e8b7e4d9db858e5ad3af07abbc69038b45d03c) Preserve module prefixes when parsing qualified constructor expressions,
  including builtin constructors used alongside Big twin declarations in core. — Thanks @MicroProofs!
- Updated dependencies: nash-source@0.5.0

## 0.3.0 — 2026-09-05

### Minor changes

- [1726a38](https://github.com/orbistry/nash/commit/1726a386268c1eb44e507bec63e80717356c5d2d) Add do blocks with bind, let, and expression statements. — Thanks @MicroProofs!
- [a480977](https://github.com/orbistry/nash/commit/a480977c380e867d3b8ded60af620b544dcaf4d3) Remove Elm-only syntax and reserve Nash keywords and operators. — Thanks @MicroProofs!
- [6b61624](https://github.com/orbistry/nash/commit/6b61624ebb3596a0263da18a39034f70376875c9) Add attributes to value, union, and alias declarations. — Thanks @MicroProofs!
- [765a64a](https://github.com/orbistry/nash/commit/765a64a4bc8025ee8d2fcb215ec91a1e2d34ffc3) Add implementation declaration syntax. — Thanks @MicroProofs!
- [9883ea8](https://github.com/orbistry/nash/commit/9883ea8e1c79bd9428df05e9b795a6b39f23d461) Add module-level unit and property test blocks. Resolve project roots before
  discovering source files so relative `nash check` paths work. — Thanks @MicroProofs!
- [35b5185](https://github.com/orbistry/nash/commit/35b518543621ce4b05b2a59a027913a4fe117ced) Add hexadecimal bytes literals to expressions and patterns. — Thanks @MicroProofs!
- [606d6b1](https://github.com/orbistry/nash/commit/606d6b16a8e1c56d7cca52598ed9ccd68264a1c5) Add constrained type annotations to the surface syntax. — Thanks @MicroProofs!
- [4a3b5cb](https://github.com/orbistry/nash/commit/4a3b5cbe988bbc3c3e136c589ca687d6eee4461a) Add qualified and unqualified macro call syntax. — Thanks @MicroProofs!
- [3e0c6af](https://github.com/orbistry/nash/commit/3e0c6af19cefbc12293a568466d77af4fa49e617) Add validator module headers. — Thanks @MicroProofs!
- [cbb9962](https://github.com/orbistry/nash/commit/cbb996224926b843751fdb0d42064cc552d4e31f) Add trait declaration syntax. — Thanks @MicroProofs!
- [d22bd2a](https://github.com/orbistry/nash/commit/d22bd2a628e8d845746a1ac532edf2d749fd207f) Add quoted type variables, little type names, kind annotations, and labeled constructor fields. Remove record extension types. — Thanks @MicroProofs!
- [1f4d0fb](https://github.com/orbistry/nash/commit/1f4d0fb9102eeff7759b15999b5f4bd4308ed156) Add assert, fail, todo, trace, and comptime expressions. — Thanks @MicroProofs!
- [c9d4638](https://github.com/orbistry/nash/commit/c9d4638dad7da0e7926870c6333e81a9d83ac777) Add left and right partial operator sections. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-source@0.4.0

## 0.2.2 — 2026-08-15

### Patch changes

- Updated dependencies: nash-source@0.3.0

## 0.2.1 — 2026-08-08

### Patch changes

- [ce5c411](https://github.com/utxo-company/nash/commit/ce5c4110ece5c7dea33bad51bafeafa9143ab38d) Fix `check_indent` to match Elm's `Space.checkIndent`: the check now runs on the parser's current column (`col > indent && col > 1`) instead of the previous token's end position. Previously a declaration starting at column 1 could be swallowed by the preceding construct — most visibly, a union's variant list would absorb a following value definition as extra constructor arguments (`type Msg = Increment | Decrement` followed by `main = 0` parsed `main` as a constructor argument and dropped the definition entirely). Multiline snapshot tests now use `assert_indented_*_snapshot!` variants that lay fragments out as they appear inside a definition, since column-1 continuation lines are not valid layout. — Thanks @rvcas!

