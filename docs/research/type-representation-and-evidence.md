# Type representation and trait evidence: switching assessment

Exploratory review, 2026-09-10. This report does not change Nash's language or
backend decisions. Nash source baseline: `e3e72a74`; Alder baseline:
[`b8d2ee03f468dcd719feee22b40915ac4654c837`](https://github.com/orbistry/alder/tree/b8d2ee03f468dcd719feee22b40915ac4654c837).
The Alder revision was checked against its remote HEAD during this review.

## 1. Replacing union-find with substitutions

**Current judgment: investigate a smaller binding-table design, but there is no
evidence yet that replacing Nash's union-find would improve the compiler.** The
strongest potential benefit is simpler internal APIs and state management. The
strongest risks are loss of type sharing, repeated traversal, and rebuilding the
correctness machinery around generalization, predicates, and error recovery.

### What the two implementations actually do

Nash already infers directly from the canonical AST. Its former intermediate
constraint-tree architecture is gone. Switching representations would therefore
not remove that layer a second time. See
[`solve/infer.rs`](../../crates/nash-solve/src/solve/infer.rs) and
[`nash-solve/lib.rs`](../../crates/nash-solve/src/lib.rs).

Nash stores a graph: a small `Variable` indexes a descriptor, whose children are
other variable handles. Weighted union joins equivalence classes, and lookup
compresses paths. Descriptors also carry ranks, predicate IDs, traversal marks,
and temporary copying state. Relevant code:
[`union_find.rs`](../../crates/nash-constrain/src/union_find.rs),
[`type_.rs`](../../crates/nash-constrain/src/type_.rs).

Alder uses owned recursive `Ty` values containing `Box`, `Vec`, and `BTreeMap`.
A variable indexes `Vec<Option<Ty>>`. Binding writes that slot; `prune` follows
bindings and writes back a cloned normalized value. It also compresses variable
lookup paths. Thus Alder uses **mutable substitution bindings**, not a persistent
substitution map returned by a pure Algorithm W implementation. Its historical
`unify.rs` is not compiled. See active
[`inference.rs:23`](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/inference.rs#L23),
[`Infer:1358`](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/inference.rs#L1358),
and [`bind/prune:7962`](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/inference.rs#L7962).

A simplified equation illustrates the common ground:

```text
unify(a -> a, int -> b)

Both must establish: a = int, b = int
Nash: join/refine graph nodes addressed by handles.
Alder: bind variable slots to types and follow those substitutions later.
```

The difference is how these facts are stored and propagated, not whether the
language can unify types or support polymorphism.

### Benefits and costs worth measuring

| Concern | Potential benefit from switching | Cost or qualification for Nash |
| --- | --- | --- |
| Reading inference code | Direct matches over ordinary type values can be easier to follow than descriptor/representative operations. | Alder's types still need binding lookup, normalization, kind information, predicates, and deferred checks. An enum alone does not replace the solver. |
| Generalization | Free-variable subtraction expresses the rule directly: quantify variables free in the inferred type but not the environment. | Alder clones relevant schemes and scans their free variables. Nash uses rank pools to avoid treating the entire environment as a fresh set calculation at each binding. Levels/ranks can also be retained with substitutions. |
| Instantiation | An explicit quantified-variable replacement map can make ownership and scoping clearer. | Nash's memoized graph copy preserves sharing and retains nongeneralized variables. A replacement must preserve variable identity, including repeated quantified occurrences and nongeneralized variables. Losing structural sharing can increase copying; freshening nongeneralized variables would be unsound. |
| Allocation and sharing | Shared immutable type nodes could simplify reading and caching. Owned values release memory through normal Rust ownership. | Alder's derived `Clone` recursively copies owned structure. `prune` clones a stored binding and a cached result. Nash clones descriptor containers too, but their child handles do not recursively clone the whole type. |
| Error-state isolation | A transaction or persistent substitution state can make failed attempts easier to discard. | Alder's actual table is mutable, not automatically transactional. Nash's diagnostics require preserving independent information after errors. Recovery remains a separate design problem. |
| Diagnostics and interfaces | Immutable normalized output can provide stable, easy-to-inspect types. | Nash already converts solver types into owned report data and canonical annotations. Better output APIs do not require replacing the solver. |
| Future type features | An explicit algebraic type representation can be convenient for extensions. | Higher kinds, nominal identity, alias application, and representation rules are semantic requirements, not consequences of union-find. They must survive either implementation. |

Concrete generalization/instantiation sites:
[Nash rank generalization](../../crates/nash-solve/src/solve.rs) (`generalize`,
`make_copy_help`),
[Alder environment scanning](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/inference.rs#L5348),
[Alder instantiation](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/inference.rs#L5403).

**A significant cost already exists in Nash:** each call to public `unify`
collects reachable variables and clones their contents before it knows whether
the comparison will fail. This supports recovery, but also runs on successful
comparisons. That work may dominate the cheap representative operations. Profile
it before attributing checking time to union-find itself. See
[`unify.rs:26`](../../crates/nash-solve/src/unify.rs) and
[`recovery.rs`](../../crates/nash-solve/src/recovery.rs).

Conversely, a substitution representation can repeat work when many variables
refer to the same large type. A shared graph can retain a common subtree once;
an owned-tree implementation may copy it at each use. This is a workload to
measure, not a demonstrated slowdown for Alder. Near-constant amortized
union/find operations likewise do not imply near-constant type inference:
structural comparisons, occurs checks, normalization, and recovery still cost
work.

The classic [Typing Haskell in Haskell](https://web.cecs.pdx.edu/~mpj/thih/thih.pdf)
is useful as an executable substitution-based specification. Its introduction
explicitly prioritizes clarity over efficient implementation and notes repeated
work. It does not establish that its representation should replace a production
compiler's graph.

### Recovery must be assessed independently

Nash retains historical dependency edges and poisons information invalidated by
failed comparisons, while continuing through independent siblings. Alder
creates a fresh inference state after failure, excludes affected declarations
and dependents, and retries; it also has body-level recovery work. Neither can
publish a failed module as successful. See
[Alder recovery](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/inference.rs#L1460).

Neither policy is forced by its type representation. A fair experiment must
hold recovery behavior constant, then evaluate changes to recovery separately.
A write journal around Nash's graph is a candidate for reducing snapshot work;
it would need to account for descriptor writes, path compression, new nodes,
predicates, and failure propagation. It is not a free rollback primitive.

### What a Nash replacement must retain

- Rigid annotation variables, local capture, let generalization, recursive-group
  behavior, and predicate variables not visible in the result type.
- Higher-kinded applications, partial aliases, nominal record identity, and
  deferred field resolution. Alder's structural row records are not a substitute
  for Nash's record semantics.
- Compiler-owned representation predicates and the existing trait evidence
  contract. A different type store does not remove these checks.
- Independent-error recovery, source provenance, stable diagnostics, and failed
  module/interface boundaries.

Zero external users removes migration and compatibility concerns. It does not
remove these requirements: they define the language being built. If a new
implementation wins, replace the old one rather than ship both indefinitely.

### A useful experiment, rather than a wholesale port

Compare these choices using the same Nash inputs and semantic rules:

1. Current union-find and recovery, as the baseline.
2. Current union-find with a measured reduction in recovery snapshot work.
3. A variable-binding table pointing to shared immutable type nodes, retaining
   levels if they are useful. This tests simpler type access without copying
   Alder's owned-tree allocation behavior.
4. An owned recursive substitution implementation only if it offers a concrete
   simplicity benefit over option 3.

Start with unification and let polymorphism; this is a bounded experiment, not a
second production solver. Use generated constraints for long variable chains,
repeated references to shared large types, repeated polymorphic instantiation,
nested applications, and late mismatches. Then require parity on Nash's higher
kinds, aliases, records, traits, representation predicates, and recovery tests.

Record elapsed inference time, allocations/bytes, peak memory, node visits,
normalization visits, and recovery work. Separate parsing and reporting. Run
release builds, warm up, repeat, and report distributions and scaling. Count
implementation complexity only across equal semantic coverage, not entire
crate line counts. Prototype syntax and runtime backends must not affect this
comparison.

**Adoption gate:** demonstrably simpler production code or a repeatable material
performance gain, with all semantic/recovery tests preserved and no important
workload regression. The current source review establishes no speed winner.

## 2. Runtime dictionaries instead of mandatory specialization

**Current judgment: this is the more promising architectural experiment.**
It could make trait-evidence specialization optional where the remaining
representation operations admit generic lowering, reduce duplicated bodies,
and support some evidence-growing recursion. Whether it is a better default for Nash depends on UPLC script size
and execution budgets, not JavaScript output size alone.

### What is implemented, and what is only planned

Nash's solver already records `Impl`, `Given`, and `Super` evidence, plus
compiler-owned `Repr`, `ReflexiveLift`, and `StructuralEq` evidence. Definition
schemes and use-site instances are implemented. See
[`Evidence`](../../crates/nash-ast/src/lib.rs) and
[`SolvedTypes`](../../crates/nash-solve/src/solved.rs).

There is no `nash-codegen` crate in this checkout. The backend worklist that
consumes evidence is specified in [Plan 07](../../plans/07-codegen.md), not an
implemented specialization engine. Consequently, this review cannot benchmark
Nash's two emitted-code strategies against each other yet.

There is also a material inconsistency to resolve before implementing either:

- [traits.md](../traits.md), “Recursion” and the evidence discussion, says an
  evidence-free polymorphic definition has one specialization and describes
  specialization as evidence-only.
- [codegen.md](../codegen.md), “Monomorphization”, and Plan 07's `MonoKey` include
  ordinary ground `type_args` in addition to evidence.

Those contracts can produce different copy counts, and potentially different
termination behavior for polymorphic recursion. A generic identity function
should not need different code merely because its argument has a different
source type if its lowered body is identical. Conversely, any type-dependent
lowering or representation operation must remain accounted for. Define the key
in terms of actual code-generation dependencies; do not assume all types erase
without checking their consumers. This review records the disagreement rather
than silently choosing a new language/backend policy.

Alder implements dictionaries in its active JavaScript backend. Its solver
publishes binding dictionary parameters and per-use actions; codegen turns them
into hidden arguments, method selections, imports, and factory calls. See
[`SolveOutput`, `BindingAbi`, `UseAction`](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-solve/src/lib.rs#L18)
and [evidence lowering](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-codegen/src/oxc_backend.rs#L3008).

### What changes at a trait call

Schematic lowering, with concrete names shortened for clarity:

```text
Source:
    describe : Show a => a -> string
    describe x = show x

Specialization:
    describe_int x = show_int x
    describe_other x = show_other x

Dictionary passing:
    describe show_dictionary x = show_dictionary.show x
    describe int_dictionary 42

After successful specialization of the dictionary version:
    describe_int x = show_int x
```

Dictionary passing does not require a runtime search for an impl. The compiler
still resolves coherent instances; it passes the selected operations as values.
The runtime operation can be an indirect function call or dictionary projection,
not a type-name lookup. Alder's
[actual emitted snapshot](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-codegen/src/snapshots/alder_codegen__tests__trait_dictionary_passing.snap)
shows a single `describe($dict0, value)` body selecting `show` from `$dict0`.

### Benefits, costs, and claims that do not follow

| Concern | Dictionary strategy | Specialization strategy |
| --- | --- | --- |
| Generated body count | One generic body can serve many evidence combinations. Dictionary values/factories still add code. | Can create a body for each relevant combination. Sharing and deduplication determine the actual count. |
| Runtime work | Hidden arguments, projections, captured evidence, and factory construction can cost execution and memory. | Closed calls can dispatch directly and expose methods to inlining and simplification. |
| Compile time | Can avoid enumerating every specialized generic body. Still requires resolution and dictionary lowering. | Worklist processing and repeated optimization can grow with the number of instances. Neither backend cost is measured for Nash yet. |
| Higher-order functions | A constrained function value can capture dictionaries in a closure and reuse generic code. | A use can select a specialized function value. Ordinary higher-order functions are not a dictionary-only feature. |
| Defaults and superclasses | Shared method wrappers and superclass fields give a uniform runtime model. | Existing evidence can select defaults and superclass impls statically. Those features do not require switching. |
| Separate compilation | A generic calling convention can allow compilation without all downstream call types. | Cross-module specialization generally needs body information or a generic fallback. Nash's final validator still requires closed executable code. |
| Polymorphic recursion | Growing evidence can be constructed at runtime instead of demanding infinitely many static bodies. | Mandatory recursive specialization needs a finite family or a rejection rule. Nash currently has such a rule. |
| Predictability | Generic fallback bounds body duplication, but runtime construction can grow with input. | More predictable direct calls, but evidence combinations can increase compile work and script size. |

A simple size model is useful as a hypothesis, not a benchmark. For a generic
body of size B used with k distinct evidence vectors, specialization can approach
k copies of B, whereas dictionaries can share B and add dictionary/call overhead.
Inlining, dead-method removal, body merging, and the number of methods actually
used can reverse the result for a small function. Count reachable copies from
real entry points, not every possible impl combination.

Alder's factory output constructs an object and method closures and freezes it;
the use-site lowering emits a factory call. There is no memoization at that
lowering site. This makes allocation a concrete concern, although later
optimization or hoisting may remove particular constructions. See
[factory construction](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-codegen/src/oxc_backend.rs#L790)
and the
[factory snapshot](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-codegen/src/snapshots/alder_codegen__tests__prerequisite_dictionary_factory.snap).

Alder also binds evidence into higher-order values using JavaScript `.bind`, and
forwards the selected self dictionary through default-method wrappers. See
[`bind_evidence`](https://github.com/orbistry/alder/blob/b8d2ee03f468dcd719feee22b40915ac4654c837/crates/alder-codegen/src/oxc_backend.rs#L3253).
These are useful implementation examples, not a ready-made UPLC calling convention.

### Keep language decisions separate

Nash already supports let polymorphism, higher-kinded types, and annotated
polymorphic recursion (subject to the evidence-growth restriction below). These
are distinct from higher-rank polymorphism: accepting a function argument whose
own type variables remain universally quantified inside the receiving function.
For example, `use : (forall a. a -> a) -> (unit, Color)` uses mathematical
`forall` notation that Nash does not currently support. The current canonical
AST keeps quantification on `Annotation.free_vars`; nested `Type` nodes do not
contain quantified schemes, and function parameters get monomorphic variables.
See [`Annotation` and `Type`](../../crates/nash-ast/src/lib.rs) and
[lambda inference](../../crates/nash-solve/src/solve/expressions.rs). The solver's
internal variable ranks track generalization levels; they do not mean rank-N
source types.

A dictionary switch does not itself add higher-rank polymorphism, existential
packages, first-class constraints, user-selected instances, or overlapping impls. Nash's
orphan/overlap rules and instance resolution can remain unchanged. Both compilers
already check coherence. A dictionary is an implementation of evidence, not a
new source-language escape from those rules.

The clearest possible extension is evidence-growing polymorphic recursion:
conceptually a recursive call may require `Show (Wrapper a)` given `Show a`.
A factory can construct the next dictionary as execution recurses. Nash currently
rejects growing evidence in [`solve.rs`](../../crates/nash-solve/src/solve.rs)
(`growing_evidence`, `final_errors`). Merely adding dictionary lowering would
leave that rejection intact. Supporting this case requires an explicit decision,
a change to that restriction, and tests using representation-valid wrappers.
It does not guarantee runtime termination or a bounded execution budget. It also
does not justify accepting a cyclic instance-resolution obligation that has no
finite proof.

### UPLC changes the cost question

Nash targets strict UPLC, not a JavaScript VM with object allocation and possible
JIT devirtualization. Candidate dictionaries need a compiler-internal encoding:
function arguments, closures/selectors, or suitable constructor/tuple values
where the chosen target supports them. Method closures are not serializable
Plutus `Data`; do not reuse a datum encoding as a dictionary representation.

Runtime applications, machine steps, and builtins contribute to evaluation cost.
The official [Plutus cost-model description](https://github.com/IntersectMBO/plutus/blob/master/plutus-core/cost-model/CostModelGeneration.md)
explains charging for machine steps and builtins. Compare serialized script
bytes, CPU budget, and memory budget under a pinned Plutus version and cost
model. Source size or JavaScript timing cannot substitute for these measurements.

Strict evaluation also makes construction and capture important. Unused methods
must not execute just because their dictionary was built. Recursive defaults and
superclasses need a valid strategy for self references. Hoisting, closure sharing,
and dictionary elimination must preserve errors, traces, and evaluation order.
These correctness obligations belong in the prototype, not only in later tuning.

Keep proof-only predicates erased: `Evidence::Repr` explicitly has no runtime
dictionary. Preserve compiler-owned structural equality and reflexive lifting
as special operations where appropriate. Passing a dictionary for every solver
predicate would introduce needless runtime data and could obscure representation
checks.

### The most useful prototype is a hybrid

Retain Nash's current solver evidence. Translate method evidence to explicit
parameters in a small Core layer; erase proof-only evidence; specialize closed
calls and eliminate dictionary projections where profitable. Unknown generic
calls retain their shared implementation. This can make trait-evidence specialization optional, provided every remaining
type-dependent representation operation also has a valid generic lowering. It
does not require a second type solver.

This is an established combination, not a claim of novelty: the
[GHC optimization guide](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/using-optimisation.html)
describes specialization of overloaded functions, including specialized calls
within them. GHC's existence establishes that dictionary passing and
specialization can coexist; it does not establish that its optimization policy
is suitable for Nash.

For the prototype, compare three lowerings of the same solved Nash evidence:

1. Mandatory specialization, with its key contract first made explicit.
2. Generic dictionaries, with no specialization, as a clear cost baseline.
3. Generic dictionaries plus a bounded specialization and elimination pass.

Use small and large constrained functions, multiple evidence combinations,
first-class constrained function values, default/superclass methods, repeated
factory use, and recursive evidence. Include compiler-owned equality/lifting and
ordinary trait-free polymorphism as controls. Measure reachable body count,
script bytes before/after optimization, compilation allocations/time, execution
CPU/memory budgets, and result/error/trace equivalence. Fix inputs and target cost
parameters; measure shallow and growing inputs separately.

**Adoption gate:** a material code-size, compile-time, or expressiveness benefit
on representative programs with acceptable target execution costs and complete
semantic coverage. If dictionaries improve generic code but hurt hot calls, a
hybrid may win. If specialization consistently wins on the intended validators,
keep it. A hybrid adds an optimizer and another callable form; it is not
inherently the simplest implementation.

## 3. Recommendation and evidence limits

These decisions are independent:

| Type inference store | Mandatory specialization | Dictionaries, optionally specialized |
| --- | --- | --- |
| Nash-style union-find | Current planned direction | Most useful next backend experiment |
| Substitution bindings | Possible, but no demonstrated advantage yet | Possible; two changes at once would obscure which caused a result |

Investigate dictionary lowering first, using the existing evidence output, and
settle the specialization-key disagreement before building the comparison.
Profile type-inference work independently, especially recovery snapshots and
repeated normalization. Prototype a new type store only against a specific
maintainability or measured-performance goal. Do not couple the two migrations.

There is no backward-compatibility requirement in this assessment. Migration cost
means engineering work and correctness risk, not supporting external users or
retaining an old implementation. Prototype alternatives can be disposable; a
selected production replacement should remove the superseded path.

Verification performed:

- Inspected the actual active module declarations, type stores, inference,
  generalization, recovery, evidence output, codegen consumer, and snapshots.
- Verified Alder's source revision against remote HEAD.
- Ran `cargo test -p alder-codegen dictionary --locked --offline`: **13 passed**.
  These are codegen tests, not UPLC execution benchmarks.
- Ran `cargo test -p alder-solve --locked --offline`: **533 passed**. This
  verifies the inspected Alder baseline; it is not a comparison with Nash.
- No Nash compiler implementation was changed. Its specialization backend does
  not yet exist, so this report contains no measured comparison of emitted Nash
  programs and makes no speedup claim.
