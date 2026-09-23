# Representation-classed impl heads

Status: complete (2026-09-23). Independent of Plans 11–13.

## Goal

Allow compiler-owned representation predicates to distinguish implementation
heads. For a trait owned by the current module, these heads are disjoint:

```nash
impl Convert ('a : Big) where ...
impl Convert ('a : Little) where ...
```

A Big blanket can also coexist with a concrete `int` implementation. A Little
blanket and an `int` implementation still overlap. Ordinary trait prerequisites
never establish disjointness.

The original motivating example also put a variable only in an impl context.
That is a separate restriction: impl context variables must still occur in a
head. This feature does not introduce existential implementation parameters.

## Implemented design

- `Head::Var { index, repr }` records the intersection of representation
  predicates on that variable. Every repeated occurrence carries the same class.
  Unrestricted variables admit all representations.
- Canonicalization derives classes after kind and representation checking.
  Predicates on constructed applications remain ordinary context obligations.
- Overlap unifies independent head variables while intersecting their classes.
  Concrete constructor representations can rule out overlap. Transparent aliases
  without a fixed representation remain conservative during overlap checking.
- Inference selection checks known representations or the intersection of givens.
  Flexible subjects defer; rigid subjects require sufficient givens. Alias
  subjects and givens are normalized and compared structurally.
- Declaration-time entailment and ground evidence use the same classed head
  matcher. Their class callbacks inspect complete types, including alias arguments.
- Structural Big equality remains compiler-owned. Identity `Lift` remains
  universal across all representations, so every classed Lift implementation is
  checked against reflexive identity.
- Classes travel inside implementation keys, including exported interfaces,
  fingerprints, selected evidence, and specialization keys.
- Diagnostics display classed variables. Inline `('a : Little)` is accepted
  alongside Big, Const, Term, and Storable.
- Literal-defaulting retries use the enclosing live inference scope; alias
  substitutions must not reopen an already generalized young scope.

## Completed chunks

- [x] 1. Representation-bearing head variables across AST consumers.
- [x] 2. Derive classes from checked implementation contexts.
- [x] 3. Classed overlap and compiler-owned instance guards.
- [x] 4. Inference, declaration entailment, and ground evidence selection.
- [x] 5. Interface fingerprint coverage, diagnostics, and in-process bundled Base tests.
- [x] 6. Trait/kind/syntax documentation and changeset.

Snapshot coverage includes disjoint and intersecting classes, nested heads,
concrete types, conservative aliases, rigid givens, intersected givens,
higher-kinded applications, missing constraints, and execution through bundled
Base. Existing superclass, identity Lift, and evidence tests run with these rules.

Validation passed: formatting, strict Clippy, and the full workspace suite.

## Completed follow-up

The solver now infers equalities between existing arguments of multi-parameter
constraints from unique compatible implementations or annotation dictionaries,
including reflexive Lift. Map.keys/values accept Big or little maps and return
little lists. Representation classes constrain the candidate probes. Connected
hidden variables can remain in generic signatures; concrete calls must resolve
them unambiguously. See docs/traits.md for the rules and limits.
