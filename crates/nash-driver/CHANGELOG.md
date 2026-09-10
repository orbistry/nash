# nash-driver

## 0.4.0 — 2026-09-10

### Minor changes

- [c4b63fd](https://github.com/orbistry/nash/commit/c4b63fd02e77d5549f182ababf59fd5f1d792aa4) Add Maranget exhaustiveness and redundancy checking across declarations,
  trait defaults, impl methods and nested expressions. Render missing-pattern
  examples and handle trait-overloaded literals conservatively.
  Reject modules with incomplete or redundant patterns after type solving,
  before publishing interfaces or retaining solved modules for dependents. — Thanks @MicroProofs!

### Patch changes

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
- Updated dependencies: nash-ast@0.7.0, nash-can@0.6.0, nash-constrain@0.4.0, nash-nitpick@0.2.0, nash-parse@0.5.0, nash-solve@0.4.0, nash-source@0.6.0

## 0.3.0 — 2026-09-08

### Minor changes

- [3495cc5](https://github.com/orbistry/nash/commit/3495cc5c755ca5b81c315a1e5358df1888db31c3) Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
  requirements through separate representation predicates and inline
  representation annotations. Infer datatype contexts with a terminating SCC
  worklist and enforce them at declarations and local or imported uses.
  
  Preserve higher-kinded and partial alias applications, captured variables,
  head-only impl coherence, superclass evidence and literal defaulting. Export
  closed kinds and ordered predicate contexts through interfaces. Use
  elementwise builtin-list Eq, structural Big Eq and reflexive Lift. — Thanks @MicroProofs!
- [dc3d24f](https://github.com/orbistry/nash/commit/dc3d24fe03d7909f0af693e8a6624f64992ceac2) Carry discovered package ownership into canonicalization and imported
  interfaces so core literal defaulting works through the CLI. Preserve
  application identities and reject conflicting ownership of a source URI. — Thanks @MicroProofs!

### Patch changes

- [e32dd1b](https://github.com/orbistry/nash/commit/e32dd1b619690c728389dc29b856e79d112da84b) Preserve module dependency order while loading sources so imports compile after their dependencies. Retain failed reads in their original positions. — Thanks @MicroProofs!
- [3d3e735](https://github.com/orbistry/nash/commit/3d3e735d3209c30858af8908fde20de1a4b3dc9b) Verify direct and transitive trait impl resolution through the driver, including
  impls owned by a type's module and orphan and overlap diagnostics. Correct the
  driver documentation to describe sequential compilation after source fetching. — Thanks @MicroProofs!
- [482c758](https://github.com/orbistry/nash/commit/482c75856f03432e67cdc8b05e402445b21377bb) Return definition schemes and use-site type arguments and trait evidence with
  inferred annotations. Preserve captured variable names and recursive evidence
  binders, and report unresolved evidence before publishing solver results.
  Adapt the driver to the paired solver result. — Thanks @MicroProofs!
- [9ad0e77](https://github.com/orbistry/nash/commit/9ad0e77164bb9f84d7e68cfa579874fc6d7cf443) Retain canonical module nodes and solved trait evidence together for the duration of a build. Separate the canonicalizer's interface lookup lifetime from arena data so dependent modules borrow interfaces without copying nodes. — Thanks @MicroProofs!
- [18bd47a](https://github.com/orbistry/nash/commit/18bd47abd82d8018833540ddc615c468f5544adb) Pass canonical trait tables into inference and discharge wanted predicates
  through superclass givens. Preserve original context indexes and projection
  paths, substitute trait parameters, and prefer explicit given evidence. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.6.0, nash-can@0.5.0, nash-constrain@0.3.0, nash-parse@0.4.0, nash-solve@0.3.0, nash-source@0.5.0

## 0.2.3 — 2026-09-05

### Patch changes

- [9c0692f](https://github.com/orbistry/nash/commit/9c0692f423aecce8e657185f9513f603a750689a) Serialize interface caches directly without a version marker or older-format handling. Use the nash/core Builtin.List identity consistently for annotations, literals, and patterns, removing the alternate List.List kind scheme and import replacement. — Thanks @MicroProofs!
- [00ebcaa](https://github.com/orbistry/nash/commit/00ebcaa31d4fe929c6bfd55f82cbaa6cd4d0fa6e) Check value annotations, including nested lets, while preserving original alias kind contracts. Include kind schemes and bounds in interface fingerprints and serialized interfaces. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.5.0, nash-can@0.4.0, nash-constrain@0.2.3, nash-solve@0.2.3

## 0.2.2 — 2026-09-05

### Patch changes

- [9883ea8](https://github.com/orbistry/nash/commit/9883ea8e1c79bd9428df05e9b795a6b39f23d461) Add module-level unit and property test blocks. Resolve project roots before
  discovering source files so relative `nash check` paths work. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.4.0, nash-can@0.3.2, nash-constrain@0.2.2, nash-parse@0.3.0, nash-solve@0.2.2, nash-source@0.4.0

## 0.2.1 — 2026-08-15

### Patch changes

- Updated dependencies: nash-ast@0.3.1, nash-can@0.3.1, nash-constrain@0.2.1, nash-parse@0.2.2, nash-solve@0.2.1, nash-source@0.3.0

## 0.2.0 — 2026-08-08

### Minor changes

- [f68130a](https://github.com/utxo-company/nash/commit/f68130af319822d4785d7bd165f8af78caf0c6f3) Port Elm's type inference: constraint generation (`Type/Constrain/*`) and the rank-based solver (`Type/{Type,UnionFind,Solve,Unify,Occurs,Instantiate}.hs`).
  
  - `nash-constrain`: the shared inference vocabulary (index-based union-find with Elm's weight balancing, descriptors, inference `Type`, `Constraint`) plus constraint generation for expressions, patterns, and modules, carrying Elm's full `Expected`/`Category` error-context hierarchy. Type error data from `Type/Error.hs` and `Reporting/Error/Type.hs` is ported; rendering stays deferred with the rest of error reporting. Elm cases nash-ast has no expressions for (`Float`/`Chr` literals, shaders, kernel/debug vars, ports, effect managers) are omitted, and built-in type homes are package-less `Basics`/`List`/`String` pending a canonical core package.
  - `nash-solve`: unification with number/comparable/appendable/compappend supertypes, extensible records, and aliases; occurs checks; rank-based generalization with pools; and `to_annotation`/`to_error_type` with Elm's fresh-name scheme. One deliberate fix over Elm: `getVarNames` tracks visits per call instead of with persistent descriptor marks, so top-level values sharing generalized variables (unannotated mutual recursion) get complete `Forall`s — the same shape crashes Elm 0.19.1 with "Map.!: given key is not an element in the map" when used cross-module.
  - `nash-driver`: modules now run the full pipeline — parse, canonicalize, constrain, solve, `Interface::from_module` with the solver's annotations — in dependency order, and each solved module's interface is deep-copied into a build-wide arena for its dependents. Cross-module compilation is back, type-checked end to end.
  - `nash-can`: re-export `Annotations`; `nash-ast`: derive `Copy` for `FieldType`. — Thanks @rvcas!
- [ce5c411](https://github.com/utxo-company/nash/commit/ce5c4110ece5c7dea33bad51bafeafa9143ab38d) Restore Elm's pipeline invariant: interfaces only exist for type-solved modules, and foreign annotations are required, not optional.
  
  - `nash-ast`: `VarForeign`, `VarOperator`, and `Binop` now carry a required `&Annotation`, matching `AST.Canonical` field-for-field.
  - `nash-can`: `Interface::from_module(bump, module, annotations)` takes the solver's annotations map like Elm's `I.fromModule`; `InterfaceValue`/`InterfaceBinop`, `Env`'s `Var::Foreign`, `q_vars`, and `Binop` all carry required annotations. Local `infix` declarations no longer enter the env (matching Elm — the defining module calls the operator's function directly); they are validated instead: duplicate operators and operators whose function is not a top-level value are now real errors (`DuplicateBinop`, `BinopFunctionNotFound`), which Elm never needed because `infix` is kernel-only there. Nash keeps user-defined `infix` by simply not porting Elm's kernel-package parse gate.
  - `nash-driver`: cross-module compilation is removed until `nash-constrain`/`nash-solve` exist — compiling dependents against unsolved dependencies was never sound. Modules now parse and canonicalize independently (in parallel), and the `Interface<'static>` transmute is gone. — Thanks @rvcas!

### Patch changes

- [ce5c411](https://github.com/utxo-company/nash/commit/ce5c4110ece5c7dea33bad51bafeafa9143ab38d) Fix semantic deviations from Elm's canonicalizer found in a full parity audit, and stop the driver from blocking the tokio executor during builds.
  
  nash-can:
  
  - Duplicate binders are now detected across sibling argument patterns (`f x x = x` and `\x x -> x` are rejected), with one duplicate-detection scope spanning all arguments like Elm's `Pattern.verify`.
  - Import privacy is enforced: closed unions no longer leak constructors and private unions/aliases are no longer importable (`toPublicUnion`/`toPublicAlias` are now applied when building the import environment).
  - `import Foo as F exposing (Bar(..))` exposes constructors again — the exposed-ctor tables are built directly from the interface instead of reading back through the alias-keyed qualified table.
  - Let-destructure cycle detection uses Elm's `_M$`-mangled node keys with an edge per bound name, so `let (a, b) = f a` reports `RecursiveLet` instead of silently dropping the destructure.
  - `let` destructures with constructor and list patterns now bind their names.
  - Record extension variables participate in union/alias free-variable checks: extensible-record aliases are accepted, unbound extension variables are rejected.
  - Bool constructor patterns are arity-checked (`True x` is a `BadArity` error).
  - `iterated_dealias` substitutes alias arguments, so typed definitions through parameterized aliases get the instantiated argument/result types.
  - Error fidelity now matches Elm: one duplicate error per name in name order, constructor/type lookup errors point at the name itself, `ExportNotFound` carries suggestions, `AnnotationTooShort` carries argument counts, `RecursiveAlias` carries the source type, export resolution runs before duplicate detection, cyclic aliases run the type-variable check first, and all bad top-level cycles in a group are reported together.
  - Determinism parity with Elm: SCC computation is an exact port of `Data.Graph.stronglyConnComp` (key-sorted Kosaraju), record fields and explicit exports are stored in Elm's canonical name order, explicit type/binop exposing overwrites instead of merging, and ambiguity tracking compares full canonical module names.
  
  nash-driver:
  
  - `build` now fetches sources asynchronously and runs all CPU-bound compilation inside `spawn_blocking`, compiling modules on a bounded pool of scoped worker threads instead of one OS thread per module and no longer stalling tokio executor workers. — Thanks @rvcas!
- Updated dependencies: nash-ast@0.3.0, nash-can@0.3.0, nash-constrain@0.2.0, nash-parse@0.2.1, nash-solve@0.2.0

## 0.1.1 — 2026-02-20

### Patch changes

- Updated dependencies: nash-config@0.3.0

