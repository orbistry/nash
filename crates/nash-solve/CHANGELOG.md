# nash-solve

## 0.3.0 — 2026-09-08

### Minor changes

- [c2a42e7](https://github.com/orbistry/nash/commit/c2a42e70f506f2b72807eebccf44050cef582bd0) Support all tuple components in canonicalization, constraints, inference, impl resolution, and error types. Preserve tuple tails when instantiating schemes and reject mismatched arities, component types, and recursive types. — Thanks @MicroProofs!
- [7a294b7](https://github.com/orbistry/nash/commit/7a294b7553823e0e83c4cf9f87ee467fe1931889) Resolve core Lift reflexively for already-equal Big types, retaining solved evidence without narrowing declared types. Preserve ordinary impl selection when the compiler rule does not apply. — Thanks @MicroProofs!
- [ec35b41](https://github.com/orbistry/nash/commit/ec35b41d9251a2363c6da87284e97bb5abbb09a2) Provide structural Eq for all Big types, reject explicit Big Eq overrides,
  and retain compiler-owned equality evidence through inference and resolution. — Thanks @MicroProofs!
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
- [8b5ba81](https://github.com/orbistry/nash/commit/8b5ba81eb92c662c6ac5bf7e5dd8c8a21fcf53dd) Reduce inferred trait contexts and assign retained requirements to their
  definition's evidence slots. Keep recursive argument and result variables in
  the group scope so all members retain the same type relationships and context. — Thanks @MicroProofs!
- [95c059e](https://github.com/orbistry/nash/commit/95c059ef0bb9b2ac7aca751b76efd152d6bd11c9) Infer partially applied aliases while preserving nominal impl identity and closed parameterized bodies across interfaces. Saturate aliases without capturing caller variables, and make evidence keys independent of alias body normalization. — Thanks @MicroProofs!
- [d23211d](https://github.com/orbistry/nash/commit/d23211d6a96205a8ec03f4749705aa30a6061a67) Instantiate foreign and method annotation predicates with the same fresh
  variables as their types. Retain wanted predicates in the solver with their
  originating use, context position, and rank, and attach their IDs to argument
  descriptors. — Thanks @MicroProofs!
- [c494bc8](https://github.com/orbistry/nash/commit/c494bc8bcd99ba92de24125fe015250543df74b2) Match recursive impl patterns consistently during inference, ground resolution
  and superclass checks. Preserve repeated variables and full impl identity,
  reject structural overlaps, and retain nested patterns in diagnostics. — Thanks @MicroProofs!
- [991157d](https://github.com/orbistry/nash/commit/991157d77f8edec3b3dad59a76212a5001c5fb5e) Discharge exact wanted predicates from enclosing annotation contexts in every
  Let path. Preserve use provenance and record the owning binder and context
  index. Match existing types without unification, retaining nominal aliases
  and normalizing equivalent record extension chains. — Thanks @MicroProofs!
- [96b68e0](https://github.com/orbistry/nash/commit/96b68e0cdd5785bd7752724fde3abf1486106edf) Resolve known trait heads through coherent impls and their contexts. Preserve
  impl substitutions and child evidence, report missing impls at the original
  call, and stop expanding contexts with a diagnostic. — Thanks @MicroProofs!
- [482c758](https://github.com/orbistry/nash/commit/482c75856f03432e67cdc8b05e402445b21377bb) Return definition schemes and use-site type arguments and trait evidence with
  inferred annotations. Preserve captured variable names and recursive evidence
  binders, and report unresolved evidence before publishing solver results.
  Adapt the driver to the paired solver result. — Thanks @MicroProofs!
- [ed97474](https://github.com/orbistry/nash/commit/ed974747293ded653a0fe0319c1fe98e31a7fa1a) Resolve ground canonical trait predicates into bounded impl and reflexive Lift
  evidence. Use representation predicates to prove Big without narrowing types, and preserve nominal aliases and ordered impl arguments. — Thanks @MicroProofs!
- [18bd47a](https://github.com/orbistry/nash/commit/18bd47abd82d8018833540ddc615c468f5544adb) Pass canonical trait tables into inference and discharge wanted predicates
  through superclass givens. Preserve original context indexes and projection
  paths, substitute trait parameters, and prefer explicit given evidence. — Thanks @MicroProofs!
- [4af7e4e](https://github.com/orbistry/nash/commit/4af7e4e5a8508e7599e57abb420d43c68124c55e) Convert explicit inference contexts to canonical annotations in evidence
  order. Preserve predicates whose arguments are absent from the result type,
  and reserve variable names across both the type and context. — Thanks @MicroProofs!
- [8c9d933](https://github.com/orbistry/nash/commit/8c9d93331491d9cf7de2a6072f546e2f09746332) Retain trait contexts on unannotated definitions and instantiate their type
  and full context together at each local use. Preserve outer-variable sharing,
  constructed predicate arguments, and shared contexts for recursive groups. — Thanks @MicroProofs!

### Patch changes

- [e75769b](https://github.com/orbistry/nash/commit/e75769b0977cf41dc4341207237b67663099fac6) Copy multiple scheme roots with one shared variable map and restore every
  touched original directly. Preserve sharing within an instantiation, freshen
  generalized variables between uses, and retain outer variables unchanged. — Thanks @MicroProofs!
- [fb3b4c7](https://github.com/orbistry/nash/commit/fb3b4c7fd76d3692d62900e1fd776b4523ce910e) Add predicate IDs to inference descriptors. Preserve and deduplicate pending
  obligations through unification, including descriptor changes during recursive
  unification and type errors. Preserve original descriptor obligations during
  scheme copy bookkeeping and restoration. — Thanks @MicroProofs!
- [343b710](https://github.com/orbistry/nash/commit/343b71008b0e20789276e5f99d5e59abe7b26866) Desugar do statements through the checked core Monad.bind method with sequential pattern and let scope. Reject refutable bind patterns and missing core Monad declarations. Verify inferred constraints and evidence against explicit nested bind calls. — Thanks @MicroProofs!
- [183e0be](https://github.com/orbistry/nash/commit/183e0be466a2a9e035e537d37ff9070b19e32bfd) Report ambiguous trait variables that disappear from a definition's full
  type. Check both inferred and annotated bodies at their generalization
  boundary, preserve outer captures, and identify the innermost definition. — Thanks @MicroProofs!
- [f402a01](https://github.com/orbistry/nash/commit/f402a01df83606682cff2c50c806408e258d0b71) Remove the old numeric, comparable and appendable supertype machinery. Type variable names no longer imply constraints, and negation records core Num evidence at its original expression node. — Thanks @MicroProofs!
- [27c0986](https://github.com/orbistry/nash/commit/27c098688e706fd44feba0cfe5b63ae35e6651ba) Preserve declared quantifiers when instantiating typed recursive calls.
  Report PolymorphicRecursion when a direct or mutual recursive call wraps
  evidence from its own group in impl evidence. — Thanks @MicroProofs!
- [011f4fc](https://github.com/orbistry/nash/commit/011f4fc68477dca94cf4b221f754c40ef8e843d8) Desugar prefix negation to the checked core Num.negate method with evidence on its generated method node. Remove the canonical Negate variant, fabricated method scheme, and dedicated negation constraints. Report a missing core Num method during canonicalization. — Thanks @MicroProofs!
- [ad13ea4](https://github.com/orbistry/nash/commit/ad13ea47dd9606f7c262b11e56f8d617e247f512) Preserve definition identities on untyped recursive headers and fill their
  calls' evidence slots once the group's trait context is known. Keep each
  definition's identity separate from the group's evidence binder. — Thanks @MicroProofs!
- [ca0c2b2](https://github.com/orbistry/nash/commit/ca0c2b20160cea26c13bf43042ca84ee2696c2f4) Default hidden variables constrained by the core literal traits to their
  Builtin little types. Retry impl resolution with enclosing givens and shared
  resolution limits, including defaults exposed by another impl's context. — Thanks @MicroProofs!
- [4e5f758](https://github.com/orbistry/nash/commit/4e5f758ccc2f98ca6728520d1d7062f0cb2da69d) Reject growing trait evidence across nested helper calls. Track individual
  context slots so closed evidence and calls that reset a slot remain valid. — Thanks @MicroProofs!
- [5a49f16](https://github.com/orbistry/nash/commit/5a49f162cc1d0133b954cab11c2bd4f80dd687eb) Snapshot published trait evidence with stable source locations, ordered type arguments, nested impls, recursive context owners and superclass projections. — Thanks @MicroProofs!
- [8a874b2](https://github.com/orbistry/nash/commit/8a874b20d7eeda6c503b1bd8991166f2ba7d1e4a) Report missing trait constraints on rigid annotation variables, with the
  originating method use and the definition that must provide the constraint. — Thanks @MicroProofs!
- [f6045d0](https://github.com/orbistry/nash/commit/f6045d0f9457efaa9ed19fe5a46e2a9dfe81d0f3) Verify that distinct literal traits on one hidden type variable remain ambiguous. — Thanks @MicroProofs!
- [e5e3d2f](https://github.com/orbistry/nash/commit/e5e3d2ff1c580949b3a5cfffd1bafd1a88c030bb) Allow operators backed by imported ordinary functions, retaining their original
  module and checked scheme through interfaces, as required by core Bool operators. — Thanks @MicroProofs!
- [25ee81e](https://github.com/orbistry/nash/commit/25ee81ef0323798e595f3f3fedad6dc9775915c5) Add canonical trait declarations, predicates, method references, and impl evidence. Preserve predicate contexts when copying annotations and compare evidence type arguments independently of source locations. — Thanks @MicroProofs!
- [38e93c3](https://github.com/orbistry/nash/commit/38e93c3260a58b28d71be53fd4d9518715ef0fd7) Resolve matching user-defined little/Big twin constructors by qualification. Bare names select the little twin and module-qualified names select the Big twin, with independent import privacy. Preserve duplicate-constructor errors for unrelated declarations and malformed pairs. — Thanks @MicroProofs!
- [1aa6f28](https://github.com/orbistry/nash/commit/1aa6f28cd299ee1b49faf967d426486c923e5852) Add the first real core Eq and Literal implementations and CLI acceptance.
  Use little list for list literals and patterns, with Storable element predicates.
  Put all compiler-known types in scope and count literal trait imports as used. — Thanks @MicroProofs!
- [c080145](https://github.com/orbistry/nash/commit/c08014558ae702910680321b30ae194f633c8baa) Verify literal generalization and little-type default evidence with Big and
  UTF-8 conversion impls available. — Thanks @MicroProofs!
- [44f917f](https://github.com/orbistry/nash/commit/44f917f0e505d1c6b454d749a7ef746c6d52252c) Constrain integer, string and bytes literals through their core literal traits, retaining ordered literal and Eq evidence for patterns. Canonicalize bytes literals and preserve qualified polymorphic schemes for let-destructuring at the original pattern node. — Thanks @MicroProofs!
- [8807173](https://github.com/orbistry/nash/commit/8807173ac13f1d7db057e3250ae2c9370d7fa1fb) Record definition identities and freeze scheme quantifiers at generalization,
  so local schemes do not quantify captured variables when an outer definition
  later generalizes them. Build exported annotations from the recorded schemes. — Thanks @MicroProofs!
- [84b506d](https://github.com/orbistry/nash/commit/84b506d74e2865176f295bcfdc0c77e190953062) Expose the specified representation cast bindings only inside nash/core, with
  symbolic lowering operations and independent nominal source and target types. — Thanks @MicroProofs!
- [719e577](https://github.com/orbistry/nash/commit/719e5775f250fc7ec2af82c118851d9db66b3d7e) Preserve the full tuple arity in impl keys and evidence lookup. Large tuple heads no longer wrap onto smaller tuple heads and produce false overlap errors. — Thanks @MicroProofs!
- [1a8fb4f](https://github.com/orbistry/nash/commit/1a8fb4fc6e3d98515984660c8df73be76ed7594e) Allow partial named constructors and nominal aliases in higher-kinded type
  annotations. Retain unsupplied alias parameters and diagnose invalid value
  positions through kind checking; keep overapplication errors. — Thanks @MicroProofs!
- [fc9fe15](https://github.com/orbistry/nash/commit/fc9fe156a8cd4d7ec21df9327a4a12fe9efe1a72) Export the specified Builtin value schemes with checked representation predicates, including
  the Storable list and pair APIs. Keep backend identifiers symbolic so the AST
  does not depend on the Plutus runtime. Normalize builtin unit annotations and
  impl heads to the same unit type as (). — Thanks @MicroProofs!
- [77daf19](https://github.com/orbistry/nash/commit/77daf19b159e0d34538330f1ad83f140133d1b4d) Retain instantiated annotation contexts, evidence binders, and original
  definition names with full inference types in constraints. Preserve every
  recursive group member and method independently of lexical value headers. — Thanks @MicroProofs!
- [e05004f](https://github.com/orbistry/nash/commit/e05004f9fe6a6f4414d89e3307004b06f304db83) Expose the real bool and Data constructors through the synthetic Builtin
  interface. Recognize bool patterns by the exact core identity and count their
  imports correctly, removing the old Basics.Bool special case. — Thanks @MicroProofs!
- [5904496](https://github.com/orbistry/nash/commit/5904496926a7691655692b5dd044b27f88726a7a) Allow infix declarations backed by local or imported trait methods. Preserve
  the operator provider, backing method identity, and checked scheme across
  interfaces, with evidence attached to each operator use and section. — Thanks @MicroProofs!
- [57f8033](https://github.com/orbistry/nash/commit/57f803364fb07136ed7feaf59028b86aa84ddaed) Report an annotation variable that is fixed by an outer scope as a type error,
  with its definition name and region, instead of panicking during generalization. — Thanks @MicroProofs!
- [4485efb](https://github.com/orbistry/nash/commit/4485efb14534c84ae2d0f4fb721bb7d837f14486) Carry canonical expression identities through local, foreign, method, and
  operator constraints for per-use type arguments and evidence. Binop constraints
  retain the enclosing operator node identity. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.6.0, nash-can@0.5.0, nash-constrain@0.3.0, nash-parse@0.4.0, nash-source@0.5.0

## 0.2.3 — 2026-09-05

### Patch changes

- [5875804](https://github.com/orbistry/nash/commit/5875804f6e6ea1ce8901ab43f44e1c5e3c4752cf) Infer kind schemes across recursive type declarations and preserve them through canonical interfaces. Check constructor and record fields, retain applied type variables, and report unsupported higher-kinded value unification explicitly until plan 03. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.5.0, nash-can@0.4.0, nash-constrain@0.2.3

## 0.2.2 — 2026-09-05

### Patch changes

- Updated dependencies: nash-ast@0.4.0, nash-can@0.3.2, nash-constrain@0.2.2, nash-parse@0.3.0, nash-source@0.4.0

## 0.2.1 — 2026-08-15

### Patch changes

- Updated dependencies: nash-ast@0.3.1, nash-can@0.3.1, nash-constrain@0.2.1, nash-parse@0.2.2, nash-source@0.3.0

## 0.2.0 — 2026-08-08

### Minor changes

- [f68130a](https://github.com/utxo-company/nash/commit/f68130af319822d4785d7bd165f8af78caf0c6f3) Port Elm's type inference: constraint generation (`Type/Constrain/*`) and the rank-based solver (`Type/{Type,UnionFind,Solve,Unify,Occurs,Instantiate}.hs`).
  
  - `nash-constrain`: the shared inference vocabulary (index-based union-find with Elm's weight balancing, descriptors, inference `Type`, `Constraint`) plus constraint generation for expressions, patterns, and modules, carrying Elm's full `Expected`/`Category` error-context hierarchy. Type error data from `Type/Error.hs` and `Reporting/Error/Type.hs` is ported; rendering stays deferred with the rest of error reporting. Elm cases nash-ast has no expressions for (`Float`/`Chr` literals, shaders, kernel/debug vars, ports, effect managers) are omitted, and built-in type homes are package-less `Basics`/`List`/`String` pending a canonical core package.
  - `nash-solve`: unification with number/comparable/appendable/compappend supertypes, extensible records, and aliases; occurs checks; rank-based generalization with pools; and `to_annotation`/`to_error_type` with Elm's fresh-name scheme. One deliberate fix over Elm: `getVarNames` tracks visits per call instead of with persistent descriptor marks, so top-level values sharing generalized variables (unannotated mutual recursion) get complete `Forall`s — the same shape crashes Elm 0.19.1 with "Map.!: given key is not an element in the map" when used cross-module.
  - `nash-driver`: modules now run the full pipeline — parse, canonicalize, constrain, solve, `Interface::from_module` with the solver's annotations — in dependency order, and each solved module's interface is deep-copied into a build-wide arena for its dependents. Cross-module compilation is back, type-checked end to end.
  - `nash-can`: re-export `Annotations`; `nash-ast`: derive `Copy` for `FieldType`. — Thanks @rvcas!

### Patch changes

- Updated dependencies: nash-ast@0.3.0, nash-can@0.3.0, nash-constrain@0.2.0, nash-parse@0.2.1

