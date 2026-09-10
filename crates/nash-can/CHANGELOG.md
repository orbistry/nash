# nash-can

## 0.7.0 — 2026-09-10

### Minor changes

- [d06f2c5](https://github.com/orbistry/nash/commit/d06f2c5b69164d527d1e0bc4eefe6dcc7117867c) Borrow module data and local binding maps during canonicalization instead of cloning the full environment at each scope. Preserve shadowing, diagnostics, and error recovery. — Thanks @MicroProofs!

### Patch changes

- [e22bfb5](https://github.com/orbistry/nash/commit/e22bfb5783668dc1256eba8a2294e28d679a4816) Use ordered map ranges for trait candidate lookup, evidence construction, entailment, and missing-implementation suggestions. Preserve candidate order without a second index. — Thanks @MicroProofs!
- [8bb97f6](https://github.com/orbistry/nash/commit/8bb97f680047cc8818b890af407247cdd585000e) Use source-sized coordinates and diagnostic widths throughout parsing and reporting. Check LSP coordinate conversion instead of truncating. Reject oversized Unicode escapes without integer overflow, and make arbitrary lookahead offsets safe. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.7.1, nash-parse@0.6.0, nash-region@0.3.0, nash-source@0.7.0

## 0.6.1 — 2026-09-10

### Patch changes

- [b7ff823](https://github.com/orbistry/nash/commit/b7ff823b144beb39d5cc355052710b7032f0509e) Collect independent compiler errors with dependency-aware recovery, retain failed module dependencies, and render owned diagnostics in the terminal, JSON, and language server. Preserve trait-method call names in error context. Add JSON and warning controls to `nash check`, and publish diagnostics for unsaved editor buffers with UTF-16 ranges. — Thanks @MicroProofs!
- Updated dependencies: nash-parse@0.5.1

## 0.6.0 — 2026-09-10

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

- Updated dependencies: nash-ast@0.7.0, nash-parse@0.5.0, nash-source@0.6.0

## 0.5.0 — 2026-09-08

### Minor changes

- [c2a42e7](https://github.com/orbistry/nash/commit/c2a42e70f506f2b72807eebccf44050cef582bd0) Support all tuple components in canonicalization, constraints, inference, impl resolution, and error types. Preserve tuple tails when instantiating schemes and reject mismatched arities, component types, and recursive types. — Thanks @MicroProofs!
- [e469b3e](https://github.com/orbistry/nash/commit/e469b3e4bf6a28355059dbd34a856cd52c27b0f0) Retain impl metadata in module interfaces and expose global trait and impl tables to subsequent passes. Reject duplicate impl keys across build interfaces, with both defining modules in the diagnostic. — Thanks @MicroProofs!
- [343b710](https://github.com/orbistry/nash/commit/343b71008b0e20789276e5f99d5e59abe7b26866) Desugar do statements through the checked core Monad.bind method with sequential pattern and let scope. Reject refutable bind patterns and missing core Monad declarations. Verify inferred constraints and evidence against explicit nested bind calls. — Thanks @MicroProofs!
- [ec35b41](https://github.com/orbistry/nash/commit/ec35b41d9251a2363c6da87284e97bb5abbb09a2) Provide structural Eq for all Big types, reject explicit Big Eq overrides,
  and retain compiler-owned equality evidence through inference and resolution. — Thanks @MicroProofs!
- [a39b831](https://github.com/orbistry/nash/commit/a39b831febd6487986bb418293058bb776b09947) Check impl superclass requirements using global instances and the impl context's superclass closure. Report missing proofs, context cycles, and bounded-search exhaustion without depending on declaration order. — Thanks @MicroProofs!
- [011f4fc](https://github.com/orbistry/nash/commit/011f4fc68477dca94cf4b221f754c40ef8e843d8) Desugar prefix negation to the checked core Num.negate method with evidence on its generated method node. Remove the canonical Negate variant, fabricated method scheme, and dedicated negation constraints. Report a missing core Num method during canonicalization. — Thanks @MicroProofs!
- [0919811](https://github.com/orbistry/nash/commit/09198113af9d21b889e983ac683b5b5016595391) Resolve trait predicates in annotations, check their arity and free variables, and retain contexts on top-level and local typed definitions. — Thanks @MicroProofs!
- [3495cc5](https://github.com/orbistry/nash/commit/3495cc5c755ca5b81c315a1e5358df1888db31c3) Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
  requirements through separate representation predicates and inline
  representation annotations. Infer datatype contexts with a terminating SCC
  worklist and enforce them at declarations and local or imported uses.
  
  Preserve higher-kinded and partial alias applications, captured variables,
  head-only impl coherence, superclass evidence and literal defaulting. Export
  closed kinds and ordered predicate contexts through interfaces. Use
  elementwise builtin-list Eq, structural Big Eq and reflexive Lift. — Thanks @MicroProofs!
- [390409f](https://github.com/orbistry/nash/commit/390409fdcd70855deb39f1dea0ef963cf18d37ca) Remove the superseded interface deep-copy API. Compiled interfaces now borrow the retained build arena, preserving original type and evidence identities. — Thanks @MicroProofs!
- [c494bc8](https://github.com/orbistry/nash/commit/c494bc8bcd99ba92de24125fe015250543df74b2) Match recursive impl patterns consistently during inference, ground resolution
  and superclass checks. Preserve repeated variables and full impl identity,
  reject structural overlaps, and retain nested patterns in diagnostics. — Thanks @MicroProofs!
- [38e93c3](https://github.com/orbistry/nash/commit/38e93c3260a58b28d71be53fd4d9518715ef0fd7) Resolve matching user-defined little/Big twin constructors by qualification. Bare names select the little twin and module-qualified names select the Big twin, with independent import privacy. Preserve duplicate-constructor errors for unrelated declarations and malformed pairs. — Thanks @MicroProofs!
- [6b3cf28](https://github.com/orbistry/nash/commit/6b3cf2808f14c5ca468a2eb32ed43856df9cb13f) Represent the compiler-owned reflexive Lift rule explicitly in evidence. Recognize only the exact core trait during superclass checking, prove Big without narrowing rigid types, and reject overlapping constructor impls. — Thanks @MicroProofs!
- [d43d7c3](https://github.com/orbistry/nash/commit/d43d7c3d7c522e8b18ceb9494b61af6f3b0c408f) Canonicalize impl heads and method bodies, check head and context kinds, and substitute method types without capturing head variables. — Thanks @MicroProofs!
- [44f917f](https://github.com/orbistry/nash/commit/44f917f0e505d1c6b454d749a7ef746c6d52252c) Constrain integer, string and bytes literals through their core literal traits, retaining ordered literal and Eq evidence for patterns. Canonicalize bytes literals and preserve qualified polymorphic schemes for let-destructuring at the original pattern node. — Thanks @MicroProofs!
- [84b506d](https://github.com/orbistry/nash/commit/84b506d74e2865176f295bcfdc0c77e190953062) Expose the specified representation cast bindings only inside nash/core, with
  symbolic lowering operations and independent nominal source and target types. — Thanks @MicroProofs!
- [1a8fb4f](https://github.com/orbistry/nash/commit/1a8fb4fc6e3d98515984660c8df73be76ed7594e) Allow partial named constructors and nominal aliases in higher-kinded type
  annotations. Retain unsupplied alias parameters and diagnose invalid value
  positions through kind checking; keep overapplication errors. — Thanks @MicroProofs!
- [6ab29e5](https://github.com/orbistry/nash/commit/6ab29e5c01fce919e99dcb47e275f50d48a2d7e6) Export and import trait schemes and method metadata. Preserve their identities in retained build arenas, retain private metadata for checking public schemes, and report ambiguous trait and method imports. — Thanks @MicroProofs!
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
- [9ad0e77](https://github.com/orbistry/nash/commit/9ad0e77164bb9f84d7e68cfa579874fc6d7cf443) Retain canonical module nodes and solved trait evidence together for the duration of a build. Separate the canonicalizer's interface lookup lifetime from arena data so dependent modules borrow interfaces without copying nodes. — Thanks @MicroProofs!
- [ed97474](https://github.com/orbistry/nash/commit/ed974747293ded653a0fe0319c1fe98e31a7fa1a) Resolve ground canonical trait predicates into bounded impl and reflexive Lift
  evidence. Use representation predicates to prove Big without narrowing types, and preserve nominal aliases and ordered impl arguments. — Thanks @MicroProofs!
- [c232ffc](https://github.com/orbistry/nash/commit/c232ffc7977abf880320339554891878656c02f2) Canonicalize local traits and default methods, resolve method references, and infer trait kind schemes. Check superclass cycles and predicate argument kinds while keeping method quantifiers independent. — Thanks @MicroProofs!

### Patch changes

- [e5e3d2f](https://github.com/orbistry/nash/commit/e5e3d2ff1c580949b3a5cfffd1bafd1a88c030bb) Allow operators backed by imported ordinary functions, retaining their original
  module and checked scheme through interfaces, as required by core Bool operators. — Thanks @MicroProofs!
- [25ee81e](https://github.com/orbistry/nash/commit/25ee81ef0323798e595f3f3fedad6dc9775915c5) Add canonical trait declarations, predicates, method references, and impl evidence. Preserve predicate contexts when copying annotations and compare evidence type arguments independently of source locations. — Thanks @MicroProofs!
- [95c059e](https://github.com/orbistry/nash/commit/95c059ef0bb9b2ac7aca751b76efd152d6bd11c9) Infer partially applied aliases while preserving nominal impl identity and closed parameterized bodies across interfaces. Saturate aliases without capturing caller variables, and make evidence keys independent of alias body normalization. — Thanks @MicroProofs!
- [38696ed](https://github.com/orbistry/nash/commit/38696edad4c532d191808f893d21f043d1a331a7) Verify duplicate impl method locations and overapplied named and alias head
  diagnostics, completing the impl-declaration acceptance audit. — Thanks @MicroProofs!
- [1aa6f28](https://github.com/orbistry/nash/commit/1aa6f28cd299ee1b49faf967d426486c923e5852) Add the first real core Eq and Literal implementations and CLI acceptance.
  Use little list for list literals and patterns, with Storable element predicates.
  Put all compiler-known types in scope and count literal trait imports as used. — Thanks @MicroProofs!
- [719e577](https://github.com/orbistry/nash/commit/719e5775f250fc7ec2af82c118851d9db66b3d7e) Preserve the full tuple arity in impl keys and evidence lookup. Large tuple heads no longer wrap onto smaller tuple heads and produce false overlap errors. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.6.0, nash-parse@0.4.0, nash-source@0.5.0

## 0.4.0 — 2026-09-05

### Minor changes

- [5875804](https://github.com/orbistry/nash/commit/5875804f6e6ea1ce8901ab43f44e1c5e3c4752cf) Infer kind schemes across recursive type declarations and preserve them through canonical interfaces. Check constructor and record fields, retain applied type variables, and report unsupported higher-kinded value unification explicitly until plan 03. — Thanks @MicroProofs!
- [d69e330](https://github.com/orbistry/nash/commit/d69e330c1fc593ef4d2eadf97cac3b167e541f4f) Define kind schemes and representation bounds for builtin types and seed the canonical kind environment. — Thanks @MicroProofs!
- [7d82f03](https://github.com/orbistry/nash/commit/7d82f034478f8e799aee9d15ee8c45f46189e2b4) Add bounded kind unification with occurs checks, fresh instantiation, and generalization. — Thanks @MicroProofs!
- [4cf4ffb](https://github.com/orbistry/nash/commit/4cf4ffb84c582e1b6bdd63d3a6052823cc37a209) Enforce explicit Big, Const, Term, Storable, and arrow annotations on type parameters after recursive-group inference. Report mismatches at the parameter annotation. — Thanks @MicroProofs!
- [00ebcaa](https://github.com/orbistry/nash/commit/00ebcaa31d4fe929c6bfd55f82cbaa6cd4d0fa6e) Check value annotations, including nested lets, while preserving original alias kind contracts. Include kind schemes and bounds in interface fingerprints and serialized interfaces. — Thanks @MicroProofs!

### Patch changes

- [9c0692f](https://github.com/orbistry/nash/commit/9c0692f423aecce8e657185f9513f603a750689a) Serialize interface caches directly without a version marker or older-format handling. Use the nash/core Builtin.List identity consistently for annotations, literals, and patterns, removing the alternate List.List kind scheme and import replacement. — Thanks @MicroProofs!
- [6bc70fe](https://github.com/orbistry/nash/commit/6bc70fe1ce74e01db54a38e58e1416f5083e0ec7) Relax the `pair` kind to `Storable -> Storable -> Const` so `unConstrData : Data -> pair int (list Data)` kind-checks; construction stays restricted to `mkPairData`. — Thanks @MicroProofs!
- Updated dependencies: nash-ast@0.5.0

## 0.3.2 — 2026-09-05

### Patch changes

- [b08e4a1](https://github.com/orbistry/nash/commit/b08e4a19101ea0db52ad68f08d900c8768af50c1) Use distinct generated bindings for nested operator sections. — Thanks @MicroProofs!
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
- Updated dependencies: nash-ast@0.4.0, nash-parse@0.3.0, nash-source@0.4.0

## 0.3.1 — 2026-08-15

### Patch changes

- Updated dependencies: nash-ast@0.3.1, nash-parse@0.2.2, nash-source@0.3.0

## 0.3.0 — 2026-08-08

### Minor changes

- [41b8820](https://github.com/utxo-company/nash/commit/41b8820670da125b7974568e0977942ebea5c91a) Align `nash-can` module canonicalization more closely with Elm by tightening export validation, improving imported type ambiguity handling, and updating canonicalization errors and coverage. — Thanks @MicroProofs!
- [5b371ff](https://github.com/utxo-company/nash/commit/5b371ff3a3736619cc9d9a056b2f9dec21ba6b97) # Expand Interface model
  
  Add visibility, constructor metadata, and binop support for canonicalization. — Thanks @MicroProofs!
- [5b371ff](https://github.com/utxo-company/nash/commit/5b371ff3a3736619cc9d9a056b2f9dec21ba6b97) # Replace flat TypeContext with Env
  
  BTreeMap-based Env built from imports and local definitions, add
  InterfaceValue with annotation slot, auto-create RecordCtor for record
  aliases. — Thanks @MicroProofs!
- [ce5c411](https://github.com/utxo-company/nash/commit/ce5c4110ece5c7dea33bad51bafeafa9143ab38d) Restore Elm's pipeline invariant: interfaces only exist for type-solved modules, and foreign annotations are required, not optional.
  
  - `nash-ast`: `VarForeign`, `VarOperator`, and `Binop` now carry a required `&Annotation`, matching `AST.Canonical` field-for-field.
  - `nash-can`: `Interface::from_module(bump, module, annotations)` takes the solver's annotations map like Elm's `I.fromModule`; `InterfaceValue`/`InterfaceBinop`, `Env`'s `Var::Foreign`, `q_vars`, and `Binop` all carry required annotations. Local `infix` declarations no longer enter the env (matching Elm — the defining module calls the operator's function directly); they are validated instead: duplicate operators and operators whose function is not a top-level value are now real errors (`DuplicateBinop`, `BinopFunctionNotFound`), which Elm never needed because `infix` is kernel-only there. Nash keeps user-defined `infix` by simply not porting Elm's kernel-package parse gate.
  - `nash-driver`: cross-module compilation is removed until `nash-constrain`/`nash-solve` exist — compiling dependents against unsolved dependencies was never sound. Modules now parse and canonicalize independently (in parallel), and the `Interface<'static>` transmute is gone. — Thanks @rvcas!
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
- [b5dce51](https://github.com/utxo-company/nash/commit/b5dce5118211878726683e9869b88f4baf69ee4e) # Add pattern and type annotation canonicalization
  
  Extract type canonicalization into `types.rs` with `to_annotation` and free
  variable collection. Add `pattern.rs` with constructor resolution, arity
  checks, and duplicate binder detection. Add `TupleLargerThanThree` validation
  to both type and pattern paths. — Thanks @MicroProofs!
- [4cb71ef](https://github.com/utxo-company/nash/commit/4cb71ef6032bc2abe0759679d45487f7ebe2697f) # Add local validation pass
  
  Duplicate detection, alias cycle detection via SCC, type variable binding
  checks, record field and export dup checks, and Elm-style error accumulation. — Thanks @MicroProofs!
- [57ea057](https://github.com/utxo-company/nash/commit/57ea05753fbd7876b600972f837bfeb05a958b50) # Add Ctor::Bool for PBool pattern synthesis, pre-seed List, replace unwrap — Thanks @MicroProofs!

### Patch changes

- [f68130a](https://github.com/utxo-company/nash/commit/f68130af319822d4785d7bd165f8af78caf0c6f3) Port Elm's type inference: constraint generation (`Type/Constrain/*`) and the rank-based solver (`Type/{Type,UnionFind,Solve,Unify,Occurs,Instantiate}.hs`).
  
  - `nash-constrain`: the shared inference vocabulary (index-based union-find with Elm's weight balancing, descriptors, inference `Type`, `Constraint`) plus constraint generation for expressions, patterns, and modules, carrying Elm's full `Expected`/`Category` error-context hierarchy. Type error data from `Type/Error.hs` and `Reporting/Error/Type.hs` is ported; rendering stays deferred with the rest of error reporting. Elm cases nash-ast has no expressions for (`Float`/`Chr` literals, shaders, kernel/debug vars, ports, effect managers) are omitted, and built-in type homes are package-less `Basics`/`List`/`String` pending a canonical core package.
  - `nash-solve`: unification with number/comparable/appendable/compappend supertypes, extensible records, and aliases; occurs checks; rank-based generalization with pools; and `to_annotation`/`to_error_type` with Elm's fresh-name scheme. One deliberate fix over Elm: `getVarNames` tracks visits per call instead of with persistent descriptor marks, so top-level values sharing generalized variables (unannotated mutual recursion) get complete `Forall`s — the same shape crashes Elm 0.19.1 with "Map.!: given key is not an element in the map" when used cross-module.
  - `nash-driver`: modules now run the full pipeline — parse, canonicalize, constrain, solve, `Interface::from_module` with the solver's annotations — in dependency order, and each solved module's interface is deep-copied into a build-wide arena for its dependents. Cross-module compilation is back, type-checked end to end.
  - `nash-can`: re-export `Annotations`; `nash-ast`: derive `Copy` for `FieldType`. — Thanks @rvcas!
- Updated dependencies: nash-ast@0.3.0, nash-parse@0.2.1

## 0.2.0 — 2026-03-15

### Minor changes

- [4276751](https://github.com/utxo-company/nash/commit/427675123bfd01ff43fa423cbafc108ba2e88994) Add module header canonicalization to `nash-can`.
  
  The new API builds canonical module names and exports from parsed module headers,
  preserves package context, and reports missing explicit headers. — Thanks @MicroProofs!
- [546fd0d](https://github.com/utxo-company/nash/commit/546fd0dfcb132c51530e15c23a9e2308cc9b0cda) Add imported interface support for canonical type resolution in `nash-can`.
  
  This slice threads imported module interfaces through canonicalization so alias and union types can resolve from exposed imports and qualified import prefixes while leaving value canonicalization and broader import/export resolution for later slices. — Thanks @MicroProofs!
- [4dfbdda](https://github.com/utxo-company/nash/commit/4dfbdda549ea5b18d85736962ca64df731c1bea4) Extend `nash-can` module canonicalization with real lowering for local unions, aliases, and local named types.
  
  This slice removes the temporary unsupported-content pseudo-errors, canonicalizes alias and union exports against local declarations, and supports unqualified and self-qualified references to local named types while leaving imports and value canonicalization for later slices. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-ast@0.2.0

