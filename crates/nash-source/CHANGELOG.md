# nash-source

## 0.4.0 — 2026-09-05

### Minor changes

- [1726a38](https://github.com/orbistry/nash/commit/1726a386268c1eb44e507bec63e80717356c5d2d) Add do blocks with bind, let, and expression statements. — Thanks @MicroProofs!
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

## 0.3.0 — 2026-08-15

### Minor changes

- [c56025b](https://github.com/utxo-company/nash/commit/c56025bbfa8da9b042add37ceab2de17e0ffaf35) Derive `Clone` and `Copy` for `Associativity` and `Precedence`. These derives have existed in the workspace since March but were never released, so the published crate could not support dependents that embed these types in `Copy` structs (publishing `nash-can` failed with E0204 against the registry copy). — Thanks @rvcas!

