# nash-driver

## 0.9.0 — 2026-09-22

### Minor changes

- [aee8c3d2](https://github.com/orbistry/nash/commit/aee8c3d240a5a3deabfada906dda9a9e9ebfb67a) Overload bare boolean and unit expressions through FromBool and FromUnit, with
  little defaults and bundled implementations for bool, Bool, unit, and Unit.
  Like integer literals, unannotated values retain their literal constraints;
  annotate concrete entry points where needed. Qualified constructors and patterns
  retain their declared types. Add source snapshots for inference, diagnostics,
  custom conversions, and execution. — Thanks @MicroProofs!
- [9a7fe0ec](https://github.com/orbistry/nash/commit/9a7fe0ec015df3791346358027a472b8f5f37b8b) Move property sampling, rejection, tuple preparation and deferred body/display
  functions into Nash Test helpers. Keep codegen responsible for source callbacks
  and pattern binding. Compose assertion capture traces through Nash helpers while
  preserving failure-only evaluation and compiler trace settings. Base Test uses
  Builtin.trace directly. Allow keyword-named qualified references in source,
  removing the compiler special case for Test-module traces. — Thanks @MicroProofs!
- [3f9188b0](https://github.com/orbistry/nash/commit/3f9188b01b8bdc18363335274c20c150197d1173) Name property generation Prop throughout the compiler, Base, diagnostics and
  examples. Use direct draws returning values and PRNG states. Remove the identity
  run helper, generator map/bind functions and generator trait instances; ordinary
  Option do notation remains available for explicit state-threaded draws. — Thanks @MicroProofs!
- [1d057156](https://github.com/orbistry/nash/commit/1d05715685c5561c798e6bab3df4a7c8788d056a) Use little prng state with native bytes, integers, and lists for property testing.
  Pass native constructor terms between generators and the runner, removing Data
  wrapping from seeded draws and replay. — Thanks @MicroProofs!
- [dccae3df](https://github.com/orbistry/nash/commit/dccae3df5332b8d5628abb785b8b6b361d0974c1) Use a function alias for generators, returning value and next PRNG state per draw.
  Remove the redundant replay count. Prepare properties once and retain their
  state, body function, and deferred display function without rerunning generators.
  Preserve function aliases in typed definitions and captured variables inside
  native constructor and case nodes when returning CEK closures. — Thanks @MicroProofs!
- [402557e5](https://github.com/orbistry/nash/commit/402557e547f9496fb1fc73d4203433bcb25df0d4) Complete Base type helpers with Big or little inputs and little outer results.
  Preserve payload types in Lift conversions, support identity conversion for all
  types, and keep mixed-representation boolean calls lazy. Add source snapshots
  for helper behavior and rejected recursive conversions. — Thanks @MicroProofs!
- [e4abaade](https://github.com/orbistry/nash/commit/e4abaaded35a725d8502d2f23a0b15dc5a08afa1) Add List.isLength for Big or little lists and integer counts, using dropList
  and a singleton pattern rather than traversing the full list to count it. — Thanks @MicroProofs!
- [afaa06a4](https://github.com/orbistry/nash/commit/afaa06a410d67bd9eb592a31cebaaf965bf48b03) Keep computation trait instances on little representations. Add mixed Big/little
  integer arithmetic and ordering helpers, byte append/ordering helpers, Map.union,
  and Option.apply; return little outer representations without converting payloads.
  Remove Result/result, its implicit import, and Option.toResult from bundled Base. — Thanks @MicroProofs!

### Patch changes

- [b3699dba](https://github.com/orbistry/nash/commit/b3699dba5f0564852f52df963f06767e48bea643) Use shared native list and pair cases for field extraction and Base traversal.
  Reuse case-bound tails across adjacent accesses and use dropList for remaining
  gaps of two or more, preserving evaluation order and record update suffixes. — Thanks @MicroProofs!
- [76a7a02e](https://github.com/orbistry/nash/commit/76a7a02ef1bfc0a03082a453fcdafc727c8cb160) Skip payload decoding for ignored Data-pattern fields. Use direct Data
  unwrappers in Base validation implementations, preserving their failure behavior
  and existing element validation without redundant Data variant dispatch. — Thanks @MicroProofs!
- [ba13694e](https://github.com/orbistry/nash/commit/ba13694e2254c7a5d477a68326b1db380e6eeede) Preserve short-circuiting for fully applied Bool.and and Bool.or calls, including assertions. Correct Show Data constructor formatting and add executable snapshots for bundled Base traits and operators. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.10.0, nash-can@0.10.0, nash-codegen@0.4.0, nash-constrain@0.8.0, nash-nitpick@0.3.1, nash-parse@0.7.1, nash-plutus@0.3.1, nash-report@0.5.1, nash-solve@0.8.0, nash-source@0.8.1, nash-test@0.3.0

## 0.8.0 — 2026-09-20

### Minor changes

- [4de7fb4b](https://github.com/orbistry/nash/commit/4de7fb4bf2a87ff059f934a7b9c99b2029cbaee9) Rename `FromData.validateData` to `FromData.validate`. Update method definitions, imports, and calls to use `validate`; validation behavior is unchanged. — Thanks @MicroProofs!
- [352361ba](https://github.com/orbistry/nash/commit/352361baf35943117863225aca6a9d99ab5bdf92) Use one coercion-based `ToData` implementation for every Big type, including user-defined types and nested collections. Remove redundant recursive encoding implementations. Permit blanket impls in the trait's defining module while retaining overlap checks; existing concrete `ToData` impls must be removed because they overlap the blanket. — Thanks @MicroProofs!
- [a95b6910](https://github.com/orbistry/nash/commit/a95b6910d1867463cac0d1030599ea5bc6bfdf71) Provide unchecked `FromData` conversion for every Big type through a blanket coercion impl. Move checked `validate` into the separate `Validate` trait; migrate validation impls and trait imports to `Validate`. Unchecked conversion no longer requires validation instances for a type or its fields. — Thanks @MicroProofs!
- [9fa1d0a7](https://github.com/orbistry/nash/commit/9fa1d0a7de4c416cbaa7e45b5b56702b80046119) Add module-local unit tests and properties, scoped test dependencies, source-aware
  power assertions, deterministic seeded generation and counterexample shrinking.
  Provide `nash test` with budget checks, labels, trace controls, parallel execution,
  and terminal/JSON reports. Type-check tests with `nash check` while keeping them
  out of production builds. Add core Prop and Test support with explicit imports.
  
  Preserve short-circuit evaluation for the core boolean infix operators, including
  inside instrumented assertions. — Thanks @MicroProofs!
- [e69fd3cc](https://github.com/orbistry/nash/commit/e69fd3ccf423c921fbaaecf243f54ce03a83a2cb) Add `Builtin.coerce : 'a -> 'b` as an unchecked, representation-preserving function. Core `FromData.fromData` now defaults to unchecked coercion; use `validate` to reject malformed scalar and nested collection data. — Thanks @MicroProofs!
- [9d2a2b40](https://github.com/orbistry/nash/commit/9d2a2b40090d450c685b3048a31563d61a819cc8) Embed compiler-versioned Base sources in the driver and make Prelude available automatically without a declared dependency, download, or installed source directory. Keep implicit imports out of source syntax and snapshots.
  
  Reserve `Builtin` for actual Plutus functions. Move compiler-owned types, constructors, representation traits, and unchecked `coerce` to `Primitive`, with the foundation package renamed to `nash/base`.
  
  Check bundled Base through in-process compilation tests and remove CLI subprocess tests. — Thanks @MicroProofs!

### Patch changes

- [21914bc6](https://github.com/orbistry/nash/commit/21914bc6fefd951775d9b05f15c90d8b1d36b757) Support `pair(first, second)` patterns for builtin pairs in bindings, function arguments, lambdas, and case expressions. Preserve the distinction from tuples and enforce Storable component types.
  
  Lower pair patterns to native UPLC case even when a field is ignored. Use the same Core pair case for Data constructor payloads and implement Base Pair.fst and Pair.snd with Nash patterns. — Thanks @MicroProofs!
- [bfb38fbc](https://github.com/orbistry/nash/commit/bfb38fbc83c51affea4b4ebf965d9b26b94018dc) Make the bundled map Lift instance explicitly require Big keys and values. Add regression coverage for rejecting native pair components and preserving already encoded map entries through lift and lower. — Thanks @MicroProofs!
- [a219dde2](https://github.com/orbistry/nash/commit/a219dde26f6e582c5305968f21d675da931e5779) Make Data.Constr accept one `pair int (list Data)` payload in expressions and patterns. Require explicit pair destructuring for the tag and fields, lowered with native UPLC case. Update Base helpers and source snapshots. — Thanks @MicroProofs!
- [c985eecb](https://github.com/orbistry/nash/commit/c985eecb822877ccda3b9ae487042f806fd3391b) Define `toData` and `fromData` directly in their blanket impls using inline Big bounds, instead of trait defaults. Conversion behavior is unchanged. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.9.0, nash-can@0.9.0, nash-codegen@0.3.0, nash-config@0.5.0, nash-constrain@0.7.0, nash-nitpick@0.3.0, nash-parse@0.7.0, nash-plutus@0.3.0, nash-report@0.5.0, nash-solve@0.7.0, nash-source@0.8.0, nash-test@0.2.0

## 0.7.0 — 2026-09-18

### Minor changes

- [1650217](https://github.com/orbistry/nash/commit/165021768b6e9a7d0737a3f31c817a73bd2ba60f) Add validator entry-point diagnostics and build solved modules into Plutus V3 scripts with `nash build` output in UPLC, Flat, and single-wrapped CBOR formats. — Thanks @MicroProofs!
- [9440314](https://github.com/orbistry/nash/commit/9440314fdd5ff4433cb374128fb2610f52d77303) Add Core IR with explicit representations, UPLC text printing and checked DeBruijn conversion, executable structural lowering, recursion rewriting, complete builtin mapping, and lazy canonical type conversion for code generation. Add checked Data casts, shared pattern decision trees, evidence normalization, layout-demand analysis, and a driver callback retaining solved build state. — Thanks @MicroProofs!
- [bf78f35](https://github.com/orbistry/nash/commit/bf78f35258de7823f8d147f85b12aba1ed57b783) Complete validator builds with project and CLI target/trace settings, production
  test-block exclusion, protocol-10 target validation, and verified script hashes.
  Write single-wrapped CBOR as hex text instead of binary and track generated
  artifacts for safe stale-output cleanup. Existing output directories without an
  ownership manifest must be cleared of colliding artifacts or replaced with a
  fresh output directory. Optimizer settings remain unavailable. — Thanks @MicroProofs!

### Patch changes

- [3cb7a0f](https://github.com/orbistry/nash/commit/3cb7a0f1ec352bf13522e1eb71f0100ff873d3df) Preserve miette diagnostic codes and error/warning markers in terminal output. Show source paths relative to the project root while keeping file hyperlinks absolute.
  
  Expand colorless rendered diagnostic snapshot coverage across parser, canonicalizer, solver, driver, and CLI tests. Record Nash source instead of Rust assertion expressions in source-driven snapshots, move codegen snapshots to source-compilation tests, and check snapshot metadata for regressions. Keep direct assertions for internal error values and hand-built Core behavior. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.8.0, nash-can@0.8.0, nash-codegen@0.2.0, nash-config@0.4.0, nash-constrain@0.6.0, nash-nitpick@0.2.2, nash-parse@0.6.1, nash-plutus@0.2.0, nash-report@0.4.0, nash-solve@0.6.0

## 0.6.0 — 2026-09-10

### Minor changes

- [7dab841](https://github.com/orbistry/nash/commit/7dab8416345ca7bd57292d82c4d358c9073457b2) Remove unused disk interface-cache APIs, serialization, and cache metadata. Preserve in-memory exports, kind contracts, and fingerprints returned by compilation. — Thanks @MicroProofs!

### Patch changes

- [3bd387a](https://github.com/orbistry/nash/commit/3bd387aabd3755da2f734db212859647c128d3f5) Require UTF-8 text at the parser boundary instead of arbitrary bytes. Remove unchecked string conversions and pass source text directly from the driver. — Thanks @MicroProofs!
- [0ed0c75](https://github.com/orbistry/nash/commit/0ed0c75a0ac421a193a420c116cd2284ba922a25) Infer directly from the canonical AST into the existing union-find and predicate engine. Remove the allocated constraint tree and intermediate inference Type, preserving schemes, evidence, rank ownership, recursive-group sequencing, and complete diagnostics. Pass canonical modules directly to the solver. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.7.1, nash-can@0.7.0, nash-constrain@0.5.0, nash-nitpick@0.2.1, nash-parse@0.6.0, nash-region@0.3.0, nash-report@0.3.0, nash-solve@0.5.0, nash-source@0.7.0

## 0.5.0 — 2026-09-10

### Minor changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Collect independent compiler errors with dependency-aware recovery, retain failed module dependencies, and render owned diagnostics in the terminal, JSON, and language server. Preserve trait-method call names in error context. Add JSON and warning controls to `nash check`, and publish diagnostics for unsaved editor buffers with UTF-16 ranges. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-can@0.6.1, nash-constrain@0.4.1, nash-parse@0.5.1, nash-report@0.2.0, nash-solve@0.4.1

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

