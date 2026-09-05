# nash-ast

## 0.5.0 — 2026-09-05

### Minor changes

- [5875804](https://github.com/orbistry/nash/commit/5875804f6e6ea1ce8901ab43f44e1c5e3c4752cf) Infer kind schemes across recursive type declarations and preserve them through canonical interfaces. Check constructor and record fields, retain applied type variables, and report unsupported higher-kinded value unification explicitly until plan 03. — Thanks @MicroProofs!
- [d69e330](https://github.com/orbistry/nash/commit/d69e330c1fc593ef4d2eadf97cac3b167e541f4f) Define kind schemes and representation bounds for builtin types and seed the canonical kind environment. — Thanks @MicroProofs!
- [6bc70fe](https://github.com/orbistry/nash/commit/6bc70fe1ce74e01db54a38e58e1416f5083e0ec7) Relax the `pair` kind to `Storable -> Storable -> Const` so `unConstrData : Data -> pair int (list Data)` kind-checks; construction stays restricted to `mkPairData`. — Thanks @MicroProofs!
- [a15a3b7](https://github.com/orbistry/nash/commit/a15a3b72809888153a2ab531a24470890ff0c387) Add Big, Const, Term, arrow kinds, bounded kind sets, and quantified kind schemes. — Thanks @MicroProofs!
- [00ebcaa](https://github.com/orbistry/nash/commit/00ebcaa31d4fe929c6bfd55f82cbaa6cd4d0fa6e) Check value annotations, including nested lets, while preserving original alias kind contracts. Include kind schemes and bounds in interface fingerprints and serialized interfaces. — Thanks @MicroProofs!

## 0.4.0 — 2026-09-05

### Minor changes

- [3e0c6af](https://github.com/orbistry/nash/commit/3e0c6af19cefbc12293a568466d77af4fa49e617) Add validator module headers. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-source@0.4.0

## 0.3.1 — 2026-08-15

### Patch changes

- Updated dependencies: nash-source@0.3.0

## 0.3.0 — 2026-08-08

### Minor changes

- [ce5c411](https://github.com/utxo-company/nash/commit/ce5c4110ece5c7dea33bad51bafeafa9143ab38d) Restore Elm's pipeline invariant: interfaces only exist for type-solved modules, and foreign annotations are required, not optional.
  
  - `nash-ast`: `VarForeign`, `VarOperator`, and `Binop` now carry a required `&Annotation`, matching `AST.Canonical` field-for-field.
  - `nash-can`: `Interface::from_module(bump, module, annotations)` takes the solver's annotations map like Elm's `I.fromModule`; `InterfaceValue`/`InterfaceBinop`, `Env`'s `Var::Foreign`, `q_vars`, and `Binop` all carry required annotations. Local `infix` declarations no longer enter the env (matching Elm — the defining module calls the operator's function directly); they are validated instead: duplicate operators and operators whose function is not a top-level value are now real errors (`DuplicateBinop`, `BinopFunctionNotFound`), which Elm never needed because `infix` is kernel-only there. Nash keeps user-defined `infix` by simply not porting Elm's kernel-package parse gate.
  - `nash-driver`: cross-module compilation is removed until `nash-constrain`/`nash-solve` exist — compiling dependents against unsolved dependencies was never sound. Modules now parse and canonicalize independently (in parallel), and the `Interface<'static>` transmute is gone. — Thanks @rvcas!

### Patch changes

- [f68130a](https://github.com/utxo-company/nash/commit/f68130af319822d4785d7bd165f8af78caf0c6f3) Port Elm's type inference: constraint generation (`Type/Constrain/*`) and the rank-based solver (`Type/{Type,UnionFind,Solve,Unify,Occurs,Instantiate}.hs`).
  
  - `nash-constrain`: the shared inference vocabulary (index-based union-find with Elm's weight balancing, descriptors, inference `Type`, `Constraint`) plus constraint generation for expressions, patterns, and modules, carrying Elm's full `Expected`/`Category` error-context hierarchy. Type error data from `Type/Error.hs` and `Reporting/Error/Type.hs` is ported; rendering stays deferred with the rest of error reporting. Elm cases nash-ast has no expressions for (`Float`/`Chr` literals, shaders, kernel/debug vars, ports, effect managers) are omitted, and built-in type homes are package-less `Basics`/`List`/`String` pending a canonical core package.
  - `nash-solve`: unification with number/comparable/appendable/compappend supertypes, extensible records, and aliases; occurs checks; rank-based generalization with pools; and `to_annotation`/`to_error_type` with Elm's fresh-name scheme. One deliberate fix over Elm: `getVarNames` tracks visits per call instead of with persistent descriptor marks, so top-level values sharing generalized variables (unannotated mutual recursion) get complete `Forall`s — the same shape crashes Elm 0.19.1 with "Map.!: given key is not an element in the map" when used cross-module.
  - `nash-driver`: modules now run the full pipeline — parse, canonicalize, constrain, solve, `Interface::from_module` with the solver's annotations — in dependency order, and each solved module's interface is deep-copied into a build-wide arena for its dependents. Cross-module compilation is back, type-checked end to end.
  - `nash-can`: re-export `Annotations`; `nash-ast`: derive `Copy` for `FieldType`. — Thanks @rvcas!

## 0.2.0 — 2026-03-15

### Minor changes

- [72464df](https://github.com/utxo-company/nash/commit/72464df4b41aac20c3c9699e62068e151a888933) Add the canonical Nash AST definitions for modules, declarations, expressions, patterns, types, and exports.
  
  Wire the crate to `nash-region` and `nash-source` for source locations and shared source-level metadata. — Thanks @MicroProofs!

