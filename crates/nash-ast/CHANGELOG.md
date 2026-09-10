# nash-ast

## 0.7.0 — 2026-09-10

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

## 0.6.0 — 2026-09-08

### Minor changes

- [343b710](https://github.com/orbistry/nash/commit/343b71008b0e20789276e5f99d5e59abe7b26866) Desugar do statements through the checked core Monad.bind method with sequential pattern and let scope. Reject refutable bind patterns and missing core Monad declarations. Verify inferred constraints and evidence against explicit nested bind calls. — Thanks @MicroProofs!
- [ec35b41](https://github.com/orbistry/nash/commit/ec35b41d9251a2363c6da87284e97bb5abbb09a2) Provide structural Eq for all Big types, reject explicit Big Eq overrides,
  and retain compiler-owned equality evidence through inference and resolution. — Thanks @MicroProofs!
- [011f4fc](https://github.com/orbistry/nash/commit/011f4fc68477dca94cf4b221f754c40ef8e843d8) Desugar prefix negation to the checked core Num.negate method with evidence on its generated method node. Remove the canonical Negate variant, fabricated method scheme, and dedicated negation constraints. Report a missing core Num method during canonicalization. — Thanks @MicroProofs!
- [25ee81e](https://github.com/orbistry/nash/commit/25ee81ef0323798e595f3f3fedad6dc9775915c5) Add canonical trait declarations, predicates, method references, and impl evidence. Preserve predicate contexts when copying annotations and compare evidence type arguments independently of source locations. — Thanks @MicroProofs!
- [3495cc5](https://github.com/orbistry/nash/commit/3495cc5c755ca5b81c315a1e5358df1888db31c3) Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
  requirements through separate representation predicates and inline
  representation annotations. Infer datatype contexts with a terminating SCC
  worklist and enforce them at declarations and local or imported uses.
  
  Preserve higher-kinded and partial alias applications, captured variables,
  head-only impl coherence, superclass evidence and literal defaulting. Export
  closed kinds and ordered predicate contexts through interfaces. Use
  elementwise builtin-list Eq, structural Big Eq and reflexive Lift. — Thanks @MicroProofs!
- [95c059e](https://github.com/orbistry/nash/commit/95c059ef0bb9b2ac7aca751b76efd152d6bd11c9) Infer partially applied aliases while preserving nominal impl identity and closed parameterized bodies across interfaces. Saturate aliases without capturing caller variables, and make evidence keys independent of alias body normalization. — Thanks @MicroProofs!
- [c494bc8](https://github.com/orbistry/nash/commit/c494bc8bcd99ba92de24125fe015250543df74b2) Match recursive impl patterns consistently during inference, ground resolution
  and superclass checks. Preserve repeated variables and full impl identity,
  reject structural overlaps, and retain nested patterns in diagnostics. — Thanks @MicroProofs!
- [6b3cf28](https://github.com/orbistry/nash/commit/6b3cf2808f14c5ca468a2eb32ed43856df9cb13f) Represent the compiler-owned reflexive Lift rule explicitly in evidence. Recognize only the exact core trait during superclass checking, prove Big without narrowing rigid types, and reject overlapping constructor impls. — Thanks @MicroProofs!
- [1aa6f28](https://github.com/orbistry/nash/commit/1aa6f28cd299ee1b49faf967d426486c923e5852) Add the first real core Eq and Literal implementations and CLI acceptance.
  Use little list for list literals and patterns, with Storable element predicates.
  Put all compiler-known types in scope and count literal trait imports as used. — Thanks @MicroProofs!
- [44f917f](https://github.com/orbistry/nash/commit/44f917f0e505d1c6b454d749a7ef746c6d52252c) Constrain integer, string and bytes literals through their core literal traits, retaining ordered literal and Eq evidence for patterns. Canonicalize bytes literals and preserve qualified polymorphic schemes for let-destructuring at the original pattern node. — Thanks @MicroProofs!
- [84b506d](https://github.com/orbistry/nash/commit/84b506d74e2865176f295bcfdc0c77e190953062) Expose the specified representation cast bindings only inside nash/core, with
  symbolic lowering operations and independent nominal source and target types. — Thanks @MicroProofs!
- [719e577](https://github.com/orbistry/nash/commit/719e5775f250fc7ec2af82c118851d9db66b3d7e) Preserve the full tuple arity in impl keys and evidence lookup. Large tuple heads no longer wrap onto smaller tuple heads and produce false overlap errors. — Thanks @MicroProofs!
- [fc9fe15](https://github.com/orbistry/nash/commit/fc9fe156a8cd4d7ec21df9327a4a12fe9efe1a72) Export the specified Builtin value schemes with checked representation predicates, including
  the Storable list and pair APIs. Keep backend identifiers symbolic so the AST
  does not depend on the Plutus runtime. Normalize builtin unit annotations and
  impl heads to the same unit type as (). — Thanks @MicroProofs!
- [e05004f](https://github.com/orbistry/nash/commit/e05004f9fe6a6f4414d89e3307004b06f304db83) Expose the real bool and Data constructors through the synthetic Builtin
  interface. Recognize bool patterns by the exact core identity and count their
  imports correctly, removing the old Basics.Bool special case. — Thanks @MicroProofs!
- [5904496](https://github.com/orbistry/nash/commit/5904496926a7691655692b5dd044b27f88726a7a) Allow infix declarations backed by local or imported trait methods. Preserve
  the operator provider, backing method identity, and checked scheme across
  interfaces, with evidence attached to each operator use and section. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-source@0.5.0

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

