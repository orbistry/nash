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
| Instantiation | An explicit quantified-variable replacement map can make ownership and scoping clearer. | Nash's memoized graph copy already preserves sharing and retains nongeneralized variables. A replacement must preserve both; substituting every variable would be unsound. |
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
