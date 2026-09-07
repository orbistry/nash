# Recursive kind obligations

Status: implemented, 2026-09-07. The checker uses **inductive** validity:
`self self` is rejected. This supersedes the former coinductive behavior.
Plan 03 type inference, trait resolution, and evidence construction retain
their existing contracts. Validation is recorded in the implementation plan.

## Decisions and limits

1. Infer declaration SCCs eagerly, in their own shared scope, as today.
2. At uses of generalized constructors, specialize qualified residual schemes.
   Do not instantiate parameters that have not been supplied.
3. Store a whole application spine as `Apply(head, [arguments], result)`.
4. Check outer shapes and ground bounds before nested obligation expansion.
5. Require finite inductive proofs. An active dependency is not a proof.
6. Use the finite expansion fragment below for a semantic termination guarantee.
   It is a conservative language restriction, not an established
   completeness result for unrestricted Nash schemes.

The residual representation and scheduling rules explain the supplied failures.
They do not alone prove termination. A settlement work allowance is an interim
operational safeguard, with a separate diagnostic, not a kinding rule.

## Judgments and scopes

Write a constructor declaration as

```
C = forall p1 ... pn. exists w1 ... wm. Q => (p1,...,pn) => R
```

This notation separates supplied type-parameter kinds `p`, internal result
slots `w`, the finite constraint graph `Q`, and the result `R`. It is not new
surface syntax. Actual schemes can share kind variables across positions;
the `p` entries need not be distinct independent variables. Bounds and all
such equalities are part of `Q`. ADT results are fixed by casing; alias results
can depend on parameters and must still satisfy alias casing constraints.

`Gamma` contains stable external roots, their bounds, and assumed application
predicates. `Delta` contains flexible inference roots or local result slots.
External annotation roots are rigid during proof. Declaration roots are
flexible during inference and generalized only after settlement.

The judgments are:

| Judgment | Meaning |
|---|---|
| `Gamma; Delta |- e => k ; Q` | Infer a kind and required constraints |
| `Gamma |- k in B` | Prove membership in a shape bound |
| `Gamma |- Apply(h, [a1,...,aj], r)` | Prove one whole application |
| `Gamma |- Valid(C, captures)` | Prove the demanded instantiated constraints |
| `Gamma |- Q` | Discharge all required constraints by a finite derivation |

Bounds are the existing subsets of `{Big, Const, Term, Arrow}`. A flexible
root can narrow by intersection; an empty intersection fails. To prove a
requirement for a rigid root, its promised bound must imply the required
bound. Narrowing that root is not proof. Structural kind equality keeps the
ordinary finite-tree occurs check. Nominal closure identity is not structural
expansion of its declaration.

An unsolved `Apply(f, args, r)` headed by an external parameter can be retained
in the enclosing qualified scheme. In a proof query it must follow from the
givens, preserving their sharing and existential scope. Otherwise it is not
proved. It is never made true by treating `f` as an arbitrary constructor.

## Declaration inference remains eager

Keep the current SCC algorithm: monomorphic placeholders within the SCC,
eager field/body constraints, casing checks, then generalization. A use of
`list a` narrows `a` to Storable in that declaration; incompatible fields are
reported there. This applies whether the declaration is an ADT or alias.
The result still follows existing casing rules (a little user ADT is Term;
a lowercase alias of a builtin list can be Const).

Unresolved parameter-headed applications are generalized as qualified
constraints, as they are today. Eager does not mean invent a monomorphic arrow
for polymorphic self-application. An earlier generalized declaration used
inside the SCC is instantiated by the residual rules below.

## Application and qualification

Represent `f a b` as one obligation, not `Apply(f,a,w)` followed by
`Apply(w,b,r)`. Flatten only the application spine: `f (g a) b` retains a
separate argument subexpression `g a`. Its result slot may be an argument,
but is not an artificial queued head. A kind annotation between spine nodes
remains a constraint at that prefix; flattening cannot discard it.

For known `C[c1,...,ci]` and a new argument vector `a`, combine the captures
and arguments once, substitute them simultaneously in C's template, and
check the constraints determined in this scope. This operation does not
allocate witnesses for missing declaration parameters.

Determine residual dependencies transitively through result-slot producers.
Decompose a constraint into its independently decidable projections: a known
head's arity can fail even when an argument is still symbolic. Check all
projections independent of missing parameters, and discharge every already
closed validity subgoal, at partial use. For `p f a = P (f f) a`, partial
`p self` already demands `self self` and fails; missing a does not defer it.
Conversely, do not discharge a dependent premise by inventing a value for a
missing parameter. Preserve the original full constraint as the residual
unless all its projections have been proved.

* **Underapplication:** return a closure with the remaining binder and all
  residual constraints. Its outer shape is Arrow. Apply bounds and equalities
  on already supplied arguments now, including explicit alias bounds.
* **Saturation:** return the instantiated result shape, with a required
  `Valid` premise for every instantiated constraint. The shape can be read
  provisionally for scheduling; it is not proof that the application is valid.
* **Overapplication:** reject when the result cannot accept the remaining
  arguments. Preserve named-constructor `BadArity` and variable-application
  diagnostics at their existing layers.

For example, a partial `s F` retains

```
forall G A. exists u v.
  Apply(F, [F,A], u), Apply(G, [u], v), v in Any
  => (G,A) => Term
```

No fresh actual `G` or `A` enters the global queue. At `s F G A` the first
obligation specializes in one step. Its local result `u` is a slot with a
producer, not a new independently quantified type parameter.

Residual qualification is a condition on future uses. It does not promise
that every argument with a compatible outer shape will work. A phantom `tag`
can accept such a closure under All without demanding its future applications.
An operation that applies it must retain and discharge those applications.
An unqualified arrow capability requires proof, with rigid symbolic arguments,
that its promised domain implies the residual conditions. Arrow shape alone
cannot establish that capability.

Constraints on supplied arguments are never dropped because a constructor is
phantom. Thus `tag (pair (option int))` still fails the supplied pair bound.
Only obligations depending on missing parameters remain qualified. Results
already supplied to a field or saturated constructor still require validity,
even if the consumer is phantom.

Freshen local slots for each independent use. Preserve repeated external
variables and captured arguments. Generalize/instantiate a `ValueKinds` group
together; do not instantiate each predicate separately. In particular, two
applications of option may accept Const and Term independently, while two
occurrences of the same external variable retain one kind.

## Inductive recursion

`Valid` is the **least** fixed point of the above finite-premise rules. A valid
ground application has a finite proof tree. There is no coinductive rule.

A completed proof may be reused. A dependency on an active identical goal
cannot close a proof and reports `KindInfinite` at the source use. The checker
keeps the dependency path internally for cycle detection.
This requires identity after substitution, including constructor identity,
captured arguments, argument vector, bounds, external-root identities,
sharing, and proof assumptions. Different captures or equal bounds alone do
not identify goals. A node pending its first expansion is not completed.

For deterministic known-constructor expansion all premises are required, so
an active back-edge proves that this required derivation is cyclic. If future
rules add alternatives, use least-fixed-point answer propagation; failure of
one cyclic alternative must not reject an available finite proof.

Local result slots may be renamed within their own binder, bijectively and
with sharing preserved. External roots must not be alpha-renamed to make a
cycle. The restricted algorithm below eliminates local slots before recursive
keys are made, and therefore needs no general alpha-equivalent goal solver.
Reusing a proof never reuses another instantiation's mutable result variables.

This policy concerns kind-obligation proofs. Ordinary recursive ADTs remain
checked through monomorphic SCC placeholders. Trait evidence, superclass
resolution, and structural kind equality acquire no new cyclic rules.

## Why cycle detection is insufficient

N-ary specialization removes the supplied unsupplied-parameter replay loop.
But a finite set of constructor names alone does not bound nested captures:
the unrestricted representation admits `C[x]`, `C[C[x]]`, and so on. Exact
cycle detection terminates only if reachable distinct goal states are bounded
or every non-repeating path has a well-founded decrease.

Whether the full Nash rules, including supplied-prefix arity checks, already
impose such a bound is **unresolved**. This document supplies no valid Nash
counterexample to termination of the new n-ary residual rules, and makes no
undecidability claim. The 22 inputs demonstrate the old replay bug, not the
necessity of an additional restriction. The fragment below is sufficient for
a guarantee; its necessity and a less restrictive alternative remain open.

## A precise finite expansion fragment

The implementation uses the following conservative
restriction at each discharge boundary. A boundary is one complete source
application/annotation predicate group, with all its syntactic arguments
collected before expansion; it is not an arbitrary queue batch.

1. Freeze a finite atom set `A`: base kinds, stable roots of that query's
   enclosing binder and source result witnesses, and all closure/arrow subterms in the normalized input
   and closed constants of the reachable finite declaration templates.
   Include empty named constructor closures and all source application
   prefixes. Preserve root identity. Do not unfold templates to build A.
2. Let `D` contain A and every partial closure `C[a1,...,aj]` with captures
   in A and `j < arity(C)`. Also allow arrows whose two children are in A.
   A generated member of D may be an application head or result. It may
   itself be captured or become an arrow child only if it already belongs
   to the frozen A. The set is not enlarged during expansion.
3. A partial or saturated expansion key consists of C and an ordered capture/argument
   vector from A. A request with a generated capture outside A is outside
   this fragment. Direct local shape errors take precedence over this check.
4. Local existential witnesses are result slots, not atoms and not choices
   from a finite domain of kinds. Resolve a slot through its producer and
   ordinary finite unification before using it in a recursive expansion key.
   If it cannot resolve to an admitted term, retain the qualified predicate
   where permitted; otherwise report an unsupported dependency.
5. Keep bound refinements/equivalences on the finite query roots explicit.
   If a solved local slot aliases an original root, use that root identity.
   No allocation during recursive expansion creates an additional external
   root. Unification cannot construct arrow terms outside the same restriction.
6. Normalize givens as a set over the finite admitted predicate/template
   universe. Keys cannot accumulate duplicate assumptions or fresh scoped
   binders. Any rigid arguments needed for a capability proof must be allocated
   from the finite source capability syntax before freezing A. A demand for
   another nested capability binder during expansion is outside this first
   fragment. Local existential givens keep their original finite binder and
   may be matched bijectively without replacing their witnesses by constants.

These rules constrain proof expansion, not the semantic range of a kind
variable. A rigid parameter still ranges over all kinds admitted by its bound.
Do not prove a universal capability by enumerating A. Use a rigid proof with
givens; if that cannot prove the capability, return not-proved.

The restriction can reject a finite proof which passes a newly generated
closure as another closure's argument. That is an explicit limitation, not
`KindInfinite`. Moving an expression into a separately named source definition
can change admission; the rule is intentionally syntax-sensitive. Broader
closure under construction or a size-change analysis needs a separate proof.
Do not silently extend A when such a case appears.

## Scheduler and algorithm

First collect the source spine and argument shapes without recursively
discharging known schemes. Run finite outer shape checks and ground bound
checks (including missing/extra arguments) before expanding nested `Valid`
premises. Repeat this priority after each specialization. Symbolic constraints
remain in their owning binder, not a global queue of freshly opened schemes.

The implementation keeps a proof forest with an ancestor path per pending
application or expansion. It compares each admitted goal's scheme and ordered
arguments against its ancestors using current union-find representatives.
An active back-edge fails inductively. There is no completed-proof cache:
sibling goals may repeat, and no stale success can survive root refinement.
Proof queries use isolated state and verify protected external roots are
unchanged. Capability domains are checked as whole promises, not narrowed
intersections of the compared constructors' domains.

Prioritization is not a termination argument: each expansion must still pass
admission. A secondary work allowance can stop expensive finite inputs and
reports a limit, never a semantic success or proof of disjointness.

## Proof arguments and their scope

**Residual substitution.** For a fixed declaration template, partition its
parameters into supplied and remaining. Capture-avoiding substitution yields
the same Q as full simultaneous substitution once the remaining arguments
arrive. Retaining all other conjuncts and their sharing loses no premise.
Induction over the finite application spine proves that incremental residual
specialization and one n-ary specialization agree. This relies on preserving
prefix annotations and checking supplied bounds; dropping either is unsound.

**Proof soundness.** Induct on the height of a completed proof. Leaves are
verified finite equalities/bounds or valid givens. Each known-constructor step
instantiates its declared Q and has completed proofs of every demanded
premise. The residual substitution argument identifies these premises with
the declaration's requirements. Hence every Proven node has an inductive
derivation. Active nodes are never premises of completed proofs. Capability
proofs use rigid roots, so cannot manufacture a stronger assumption by
narrowing the caller's promise. This proves soundness of the abstract rules;
it is not a proof that today's mutable implementation realizes them.

**Termination in the fragment.** A is finite and frozen; finitely many
constructors have finite arities, so D and all admitted vectors are finite.
Each template has finitely many constraints and local slots. Slots cannot
escape into new roots/keys: a temporary local root can enter a recursive key
only after resolving to an admitted term or an original query-root class.
There are finitely many partitions, bounds, and acyclic bindings of the
original root classes to the allowed constructor/arrow forms over the original
atom symbols. Links to fresh witnesses inherit the original-root admission
check. Bounds narrow monotonically and root bindings never expand A.
Capability checking cannot introduce additional binder identities mid-search.
Thus the normalized goal universe is finite. Every template has finite
branching; ancestor rejection prevents a repeated admitted goal on a path.
The resulting proof forest has finite depth and finite size, although sibling
goals can repeat. Finite unification and local template traversal terminate;
local bound projection narrows a finite lattice or resolves a finite slot.
Non-admitted keys are rejected before any premise matching or child specialization. Therefore queries
terminate without relying on fuel. Retaining a symbolic predicate is a finite
output, not discharge. The operational allowance may stop a large finite proof
before completion and reports that distinction.

**Completeness boundary.** The graph algorithm decides finite derivability
for its admitted deterministic known-goal graph, relative to its checked
givens. This document claims neither unrestricted inference completeness nor
completeness of higher-rank capability implication. Unknown rigid implications
are not-proved; unsupported witness dependencies are diagnosed explicitly.
A less restrictive fragment requires a separate proof and review.

## Derivations for the supplied cases

Numbers refer to the supplied fenced source blocks, in
order, including the first three. Every declaration named s has arity three.
`C[x,...]` below denotes a partial closure, not an unfolded arrow.

| Case | Source use in w | Required outcome and derivation |
|---|---|---|
| 1 | `s s s` | KindMismatch: one argument missing; Arrow cannot be a value field |
| 2 | `s s s tag` | KindMismatch: field is `s (s s tag)`, a closure with two arguments missing |
| 3 | `s s tag tag` | Accept: field is `tag (s s tag)`; tag accepts the residual closure under All and returns Term |
| 4 | `s s s` | KindMismatch: one argument missing |
| 5 | `s (s s) tag` | KindMismatch: one argument missing |
| 6 | `s tag (s tag)` | KindMismatch: one argument missing |
| 7 | `s s (s s)` | KindMismatch: one argument missing |
| 8 | `s s (s s)` | KindMismatch: one argument missing |
| 9 | `s s s` | KindMismatch: one argument missing |
| 10 | `s s (s s s)` | KindMismatch: one argument missing |
| 11 | `s (s s) (s s)` | KindMismatch: one argument missing |
| 12 | `s s tag` | KindMismatch: one argument missing |
| 13 | `s (s s) (s s)` | KindMismatch: one argument missing |
| 14 | `s (s tag tag) s` | KindMismatch: one argument missing |
| 15 | `s (s tag tag) s` | KindMismatch: one argument missing |
| 16 | `s s (s s s)` | KindMismatch: one argument missing |
| 17 | `s s (s s s)` | KindMismatch: one argument missing |
| 18 | `s (s s) tag` | KindMismatch: one argument missing |
| 19 | `s (s s tag) (s s)` | KindMismatch: one argument missing |
| 20 | `s s (s s s)` | KindMismatch: one argument missing |
| 21 | `s (s s s) tag` | KindMismatch: one argument missing |
| 22 | `s (s s s) tag` | KindMismatch: one argument missing |

Some also have internal arity errors; they do not override the directly
available outer field mismatch. These are rule-derived expectations; the
implementation reproduces every row.

For case 3 the remaining condition of `s[s,tag]` is, for an eventual X,
`tag (s s X)` must be a value. It stays qualified. Passing the partial to
phantom tag does not apply X or demand an infinite totality proof. All closures
needed for the actual saturated use are source prefixes or permitted partials.

| Existing control | Inductive outcome |
|---|---|
| Minimal s at `s tag s tag` | Reject: `tag tag tag` tries to apply a Term |
| Arity-two cousin `s f a = S (f f a)` at `s s` | Reject: partial constructor in value field |
| Same cousin at `s s tag` | KindInfinite: demanded body application is exactly `s s tag` again |
| `self tag` | Accept: the single child is `tag tag`, with no retained premises |
| `tag tag` | Accept: tag's argument is phantom and admits Arrow |
| `self self` | KindInfinite: its required child is the same saturated application |
| `self (self tag)` | Reject: its supplied argument is Term, not applicable |
| `tag (self self)` | KindInfinite: phantom consumption cannot erase a saturated argument's validity premise |
| `app (app tag) tag` | Accept: child is `app tag tag`, then `tag tag`; distinct captured arguments, finite proof |
| `g f = G (f (f f))` at `g g` | Reject: the inner saturated result is Term, but the outer g requires an applicable argument; do shape checks first |
| Same-SCC `bad f = Bad (bad bad)` | KindInfinite: existing structural occurs check |
| Independent Big/Const/Term uses | Accept each compatible scheme instantiation without merging separate arguments |
| Functor option: `option unit` to `option (unit,unit)` | Accept: Const and Term independently satisfy Any |
| Corresponding Functor list use | Reject: Term does not satisfy Storable |
| `app list (option int)` / `pair (option int)` | Reject Storable bound, including when pair is partial |

## Current implementation and migration points

`crates/nash-ast/src/lib.rs` already represents `Kind::Constructor` with a
scheme and captures. Preserve that distinction. `KindScheme`, `ValueKinds`,
and `KindApplication` need explicit binder ownership/result-slot treatment
and vector arguments; interface serialization/fingerprints must follow suit.
Do not turn a quantified constructor into one mutable monomorphic arrow.

In `crates/nash-can/src/kinds.rs`, change `Application`, `apply_args`,
`instantiate_applications`, `open_constructor`, `settle`, and the
generalization/reachability traversal together. The current `open_constructor`
opens all parameters and replays captures, leaking unsupplied parameters into
settlement. `CanType::App` already has an argument vector. `same_kind` is not
a general solver-state identity or proof of finiteness.

Keep `proves_extension` protected-root checks, existential witness reuse,
annotation specialization, `proves_values`, impl overlap, and ground-kind
proof APIs fail-closed. A limit or fragment failure must not become evidence
of disjoint impls, a ground Big proof, or successful trait evidence.

Diagnostics distinguish a shape/bound mismatch, an inductive cycle, a
structural infinite kind, a fragment restriction, and a work limit. Store
the application path, declaration, captures and argument index needed for
existing reporting. Budget exhaustion is not `KindInfinite`.

## Primary references

* [Selsam, Ullrich and de Moura, Tabled Typeclass Resolution](https://arxiv.org/pdf/2001.04301)
  supplies the distinction between completed answers and pending dependencies.
  Its termination result assumes bounded term size; it does not prove that
  arbitrary Nash constructor expansion is finite.
* [Vytiniotis et al., OutsideIn(X)](https://simon.peytonjones.org/outsideinx/)
  motivates scoped givens/wanteds and constraint implication. It is context
  for preserving quantifier scope, not a mandate to replace Plan 03.
* [GHC instance termination rules](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/instances.html#instance-termination-rules)
  are an example of a language using an explicit decreasing restriction.
  Nash's finite-atom restriction is different; GHC is not a proof
  of these rules or of this exact `KindInfinite` policy.

The termination and soundness arguments above are for the proposed Nash
fragment. They are not attributed to these papers.
