# nash-constrain

## 0.4.1 — 2026-09-10

### Patch changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Collect independent compiler errors with dependency-aware recovery, retain failed module dependencies, and render owned diagnostics in the terminal, JSON, and language server. Preserve trait-method call names in error context. Add JSON and warning controls to `nash check`, and publish diagnostics for unsaved editor buffers with UTF-16 ranges. — Thanks @MicroProofs!

## 0.4.0 — 2026-09-10

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

- Updated dependencies: nash-ast@0.7.0

## 0.3.0 — 2026-09-08

### Minor changes

- [c2a42e7](https://github.com/orbistry/nash/commit/c2a42e70f506f2b72807eebccf44050cef582bd0) Support all tuple components in canonicalization, constraints, inference, impl resolution, and error types. Preserve tuple tails when instantiating schemes and reject mismatched arities, component types, and recursive types. — Thanks @MicroProofs!
- [fb3b4c7](https://github.com/orbistry/nash/commit/fb3b4c7fd76d3692d62900e1fd776b4523ce910e) Add predicate IDs to inference descriptors. Preserve and deduplicate pending
  obligations through unification, including descriptor changes during recursive
  unification and type errors. Preserve original descriptor obligations during
  scheme copy bookkeeping and restoration. — Thanks @MicroProofs!
- [183e0be](https://github.com/orbistry/nash/commit/183e0be466a2a9e035e537d37ff9070b19e32bfd) Report ambiguous trait variables that disappear from a definition's full
  type. Check both inferred and annotated bodies at their generalization
  boundary, preserve outer captures, and identify the innermost definition. — Thanks @MicroProofs!
- [f402a01](https://github.com/orbistry/nash/commit/f402a01df83606682cff2c50c806408e258d0b71) Remove the old numeric, comparable and appendable supertype machinery. Type variable names no longer imply constraints, and negation records core Num evidence at its original expression node. — Thanks @MicroProofs!
- [27c0986](https://github.com/orbistry/nash/commit/27c098688e706fd44feba0cfe5b63ae35e6651ba) Preserve declared quantifiers when instantiating typed recursive calls.
  Report PolymorphicRecursion when a direct or mutual recursive call wraps
  evidence from its own group in impl evidence. — Thanks @MicroProofs!
- [011f4fc](https://github.com/orbistry/nash/commit/011f4fc68477dca94cf4b221f754c40ef8e843d8) Desugar prefix negation to the checked core Num.negate method with evidence on its generated method node. Remove the canonical Negate variant, fabricated method scheme, and dedicated negation constraints. Report a missing core Num method during canonicalization. — Thanks @MicroProofs!
- [ca0c2b2](https://github.com/orbistry/nash/commit/ca0c2b20160cea26c13bf43042ca84ee2696c2f4) Default hidden variables constrained by the core literal traits to their
  Builtin little types. Retry impl resolution with enclosing givens and shared
  resolution limits, including defaults exposed by another impl's context. — Thanks @MicroProofs!
- [8a874b2](https://github.com/orbistry/nash/commit/8a874b20d7eeda6c503b1bd8991166f2ba7d1e4a) Report missing trait constraints on rigid annotation variables, with the
  originating method use and the definition that must provide the constraint. — Thanks @MicroProofs!
- [2dd00f1](https://github.com/orbistry/nash/commit/2dd00f1a7877aeecb9dc3bacaa48dbdd4cba428d) Retain declared contexts on local bindings, including monomorphic annotations
  and annotated recursive declarations published before their bodies. Copy
  constructed context arguments with the function type at each use, and keep
  annotation provenance distinct from call-site wanteds. — Thanks @MicroProofs!
- [ac8e124](https://github.com/orbistry/nash/commit/ac8e12473dd8d4f6f1e078d522223c54d0f44829) Infer higher-kinded applications over nominal constructors, preserving qualified schemes and use-site evidence. Remove unused alias placeholders from constraint types. — Thanks @MicroProofs!
- [3495cc5](https://github.com/orbistry/nash/commit/3495cc5c755ca5b81c315a1e5358df1888db31c3) Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
  requirements through separate representation predicates and inline
  representation annotations. Infer datatype contexts with a terminating SCC
  worklist and enforce them at declarations and local or imported uses.
  
  Preserve higher-kinded and partial alias applications, captured variables,
  head-only impl coherence, superclass evidence and literal defaulting. Export
  closed kinds and ordered predicate contexts through interfaces. Use
  elementwise builtin-list Eq, structural Big Eq and reflexive Lift. — Thanks @MicroProofs!
- [95c059e](https://github.com/orbistry/nash/commit/95c059ef0bb9b2ac7aca751b76efd152d6bd11c9) Infer partially applied aliases while preserving nominal impl identity and closed parameterized bodies across interfaces. Saturate aliases without capturing caller variables, and make evidence keys independent of alias body normalization. — Thanks @MicroProofs!
- [44f917f](https://github.com/orbistry/nash/commit/44f917f0e505d1c6b454d749a7ef746c6d52252c) Constrain integer, string and bytes literals through their core literal traits, retaining ordered literal and Eq evidence for patterns. Canonicalize bytes literals and preserve qualified polymorphic schemes for let-destructuring at the original pattern node. — Thanks @MicroProofs!
- [96b68e0](https://github.com/orbistry/nash/commit/96b68e0cdd5785bd7752724fde3abf1486106edf) Resolve known trait heads through coherent impls and their contexts. Preserve
  impl substitutions and child evidence, report missing impls at the original
  call, and stop expanding contexts with a diagnostic. — Thanks @MicroProofs!
- [482c758](https://github.com/orbistry/nash/commit/482c75856f03432e67cdc8b05e402445b21377bb) Return definition schemes and use-site type arguments and trait evidence with
  inferred annotations. Preserve captured variable names and recursive evidence
  binders, and report unresolved evidence before publishing solver results.
  Adapt the driver to the paired solver result. — Thanks @MicroProofs!
- [77daf19](https://github.com/orbistry/nash/commit/77daf19b159e0d34538330f1ad83f140133d1b4d) Retain instantiated annotation contexts, evidence binders, and original
  definition names with full inference types in constraints. Preserve every
  recursive group member and method independently of lexical value headers. — Thanks @MicroProofs!
- [8c9d933](https://github.com/orbistry/nash/commit/8c9d93331491d9cf7de2a6072f546e2f09746332) Retain trait contexts on unannotated definitions and instantiate their type
  and full context together at each local use. Preserve outer-variable sharing,
  constructed predicate arguments, and shared contexts for recursive groups. — Thanks @MicroProofs!
- [57f8033](https://github.com/orbistry/nash/commit/57f803364fb07136ed7feaf59028b86aa84ddaed) Report an annotation variable that is fixed by an outer scope as a type error,
  with its definition name and region, instead of panicking during generalization. — Thanks @MicroProofs!
- [4485efb](https://github.com/orbistry/nash/commit/4485efb14534c84ae2d0f4fb721bb7d837f14486) Carry canonical expression identities through local, foreign, method, and
  operator constraints for per-use type arguments and evidence. Binop constraints
  retain the enclosing operator node identity. — Thanks @MicroProofs!

### Patch changes

- [ad13ea4](https://github.com/orbistry/nash/commit/ad13ea47dd9606f7c262b11e56f8d617e247f512) Preserve definition identities on untyped recursive headers and fill their
  calls' evidence slots once the group's trait context is known. Keep each
  definition's identity separate from the group's evidence binder. — Thanks @MicroProofs!
- [25ee81e](https://github.com/orbistry/nash/commit/25ee81ef0323798e595f3f3fedad6dc9775915c5) Add canonical trait declarations, predicates, method references, and impl evidence. Preserve predicate contexts when copying annotations and compare evidence type arguments independently of source locations. — Thanks @MicroProofs!
- [8b5ba81](https://github.com/orbistry/nash/commit/8b5ba81eb92c662c6ac5bf7e5dd8c8a21fcf53dd) Reduce inferred trait contexts and assign retained requirements to their
  definition's evidence slots. Keep recursive argument and result variables in
  the group scope so all members retain the same type relationships and context. — Thanks @MicroProofs!
- [c494bc8](https://github.com/orbistry/nash/commit/c494bc8bcd99ba92de24125fe015250543df74b2) Match recursive impl patterns consistently during inference, ground resolution
  and superclass checks. Preserve repeated variables and full impl identity,
  reject structural overlaps, and retain nested patterns in diagnostics. — Thanks @MicroProofs!
- [c398411](https://github.com/orbistry/nash/commit/c3984110597a0a406e2ff7f36f6790f75b5f214a) Check trait default bodies and specialized impl bodies during inference, in
  the module environment, without introducing methods as top-level values. — Thanks @MicroProofs!
- [1aa6f28](https://github.com/orbistry/nash/commit/1aa6f28cd299ee1b49faf967d426486c923e5852) Add the first real core Eq and Literal implementations and CLI acceptance.
  Use little list for list literals and patterns, with Storable element predicates.
  Put all compiler-known types in scope and count literal trait imports as used. — Thanks @MicroProofs!
- [5904496](https://github.com/orbistry/nash/commit/5904496926a7691655692b5dd044b27f88726a7a) Allow infix declarations backed by local or imported trait methods. Preserve
  the operator provider, backing method identity, and checked scheme across
  interfaces, with evidence attached to each operator use and section. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.6.0

## 0.2.3 — 2026-09-05

### Patch changes

- [5875804](https://github.com/orbistry/nash/commit/5875804f6e6ea1ce8901ab43f44e1c5e3c4752cf) Infer kind schemes across recursive type declarations and preserve them through canonical interfaces. Check constructor and record fields, retain applied type variables, and report unsupported higher-kinded value unification explicitly until plan 03. — Thanks @MicroProofs!
- [9c0692f](https://github.com/orbistry/nash/commit/9c0692f423aecce8e657185f9513f603a750689a) Serialize interface caches directly without a version marker or older-format handling. Use the nash/core Builtin.List identity consistently for annotations, literals, and patterns, removing the alternate List.List kind scheme and import replacement. — Thanks @MicroProofs!
- [00ebcaa](https://github.com/orbistry/nash/commit/00ebcaa31d4fe929c6bfd55f82cbaa6cd4d0fa6e) Check value annotations, including nested lets, while preserving original alias kind contracts. Include kind schemes and bounds in interface fingerprints and serialized interfaces. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.5.0

## 0.2.2 — 2026-09-05

### Patch changes

- Updated dependencies: nash-ast@0.4.0

## 0.2.1 — 2026-08-15

### Patch changes

- Updated dependencies: nash-ast@0.3.1

## 0.2.0 — 2026-08-08

### Minor changes

- [f68130a](https://github.com/utxo-company/nash/commit/f68130af319822d4785d7bd165f8af78caf0c6f3) Port Elm's type inference: constraint generation (`Type/Constrain/*`) and the rank-based solver (`Type/{Type,UnionFind,Solve,Unify,Occurs,Instantiate}.hs`).
  
  - `nash-constrain`: the shared inference vocabulary (index-based union-find with Elm's weight balancing, descriptors, inference `Type`, `Constraint`) plus constraint generation for expressions, patterns, and modules, carrying Elm's full `Expected`/`Category` error-context hierarchy. Type error data from `Type/Error.hs` and `Reporting/Error/Type.hs` is ported; rendering stays deferred with the rest of error reporting. Elm cases nash-ast has no expressions for (`Float`/`Chr` literals, shaders, kernel/debug vars, ports, effect managers) are omitted, and built-in type homes are package-less `Basics`/`List`/`String` pending a canonical core package.
  - `nash-solve`: unification with number/comparable/appendable/compappend supertypes, extensible records, and aliases; occurs checks; rank-based generalization with pools; and `to_annotation`/`to_error_type` with Elm's fresh-name scheme. One deliberate fix over Elm: `getVarNames` tracks visits per call instead of with persistent descriptor marks, so top-level values sharing generalized variables (unannotated mutual recursion) get complete `Forall`s — the same shape crashes Elm 0.19.1 with "Map.!: given key is not an element in the map" when used cross-module.
  - `nash-driver`: modules now run the full pipeline — parse, canonicalize, constrain, solve, `Interface::from_module` with the solver's annotations — in dependency order, and each solved module's interface is deep-copied into a build-wide arena for its dependents. Cross-module compilation is back, type-checked end to end.
  - `nash-can`: re-export `Annotations`; `nash-ast`: derive `Copy` for `FieldType`. — Thanks @rvcas!

### Patch changes

- Updated dependencies: nash-ast@0.3.0

