# Representation-classed impl heads

Status: deferred. Not started. Independent of Plans 11–13.

## Goal

Let an impl's representation predicates take part in coherence and selection,
so these coexist:

```elm
impl FromData ('a : Big) where ...
impl (Lift 'a 'b, FromData 'b) => FromData ('a : Little) where ...
impl FromData int where ...            -- Const, outside Big
```

Today all three overlap. Overlap and selection look at head patterns only, and
both blankets canonicalize to `ImplKey { trait_: FromData, heads: [Var(0)] }`.
`docs/traits.md` states the rule ("they do not distinguish overlapping heads",
"contexts do not make these ordinary impls disjoint") and
`nash-can/src/impls.rs` repeats it in two comments.

This plan narrows that rule for the five compiler-owned representation traits
only. Ordinary trait contexts still never disambiguate heads.

## Why this is sound where general contexts are not

- Representation depends on the head constructor alone (`docs/kinds.md`), so
  it is decidable the moment a head is known and never depends on impl search.
- The five traits are closed and compiler-owned; no user impl can change the
  answer later. Disjointness of `Big` and `Little` is a fact, not an absence.
- The solver already defers representation predicates on flexible variables.
  Classed selection defers under exactly the same condition.

It is not a kind. Kinds stay Haskell 98. The class is a `ReprSet` derived from
the impl context.

## Design

### Data model (`nash-ast`)

`Head::Var` carries the admitted representations of that variable:

```rust
pub enum Head<'a> {
    /// `repr` is the intersection of every representation predicate the impl
    /// places on this variable; `ReprSet::ALL` when it places none.
    Var { index: u16, repr: ReprSet },
    Named { reference: QualifiedName<'a>, args: &'a [Head<'a>] },
    Tuple(&'a [Head<'a>]),
    Function(&'a Head<'a>, &'a Head<'a>),
}
```

`ReprSet` gains the derives `ImplKey` needs (`PartialOrd, Ord, Hash`) and one
helper:

```rust
impl ReprSet {
    pub const fn only(repr: Repr) -> Self { /* single bit */ }
    pub const fn is_all(self) -> bool { self.0 == Self::ALL.0 }
}
```

`ImplKey` needs no new field: the class lives inside its heads, so ordering,
hashing, `impls_for` ranges, evidence (`ImplRef`) and codegen specialization
keys pick it up with no further change. Every occurrence of one variable in an
impl carries the same `repr`.

### Deriving the class (`nash-can/src/impls.rs`)

Inline `('a : Big)` already lowers to a context entry through
`types::repr_predicates`, so inline and context spellings are one case. After
`kinds::check_impl` returns the final context:

```rust
/// Admitted representations per head variable, from the impl's own context.
fn head_classes<'a>(variables: &[&'a str], context: &[Pred<'a>]) -> Vec<ReprSet> {
    let mut classes = vec![ReprSet::ALL; variables.len()];
    for pred in context {
        let Some(trait_) = pred.trait_ref().and_then(ReprTrait::of) else { continue };
        let [subject] = pred.args() else { continue };
        let Type::Var(name) = subject.value else { continue };
        let index = variables.iter().position(|v| *v == name).expect("context var is a head var");
        classes[index] = classes[index].intersect(trait_.admits());
    }
    classes
}
```

`ContradictoryRepresentation` already rejects an empty intersection, so every
class is non-empty. `canonicalize_pattern` runs before the context exists, so
heads are built with `ReprSet::ALL` and rewritten once:

```rust
fn classed<'a>(bump: &'a Bump, head: &Head<'a>, classes: &[ReprSet]) -> Head<'a> {
    match *head {
        Head::Var { index, .. } => Head::Var { index, repr: classes[usize::from(index)] },
        Head::Named { reference, args } => Head::Named {
            reference,
            args: bump.alloc_slice_fill_iter(args.iter().map(|a| classed(bump, a, classes))),
        },
        Head::Tuple(args) => Head::Tuple(
            bump.alloc_slice_fill_iter(args.iter().map(|a| classed(bump, a, classes))),
        ),
        Head::Function(from, to) => Head::Function(
            bump.alloc(classed(bump, from, classes)),
            bump.alloc(classed(bump, to, classes)),
        ),
    }
}
```

Only predicates whose subject is exactly a head variable refine a class.
`Big (list 'a)` and `Big ('f 'a)` stay plain context obligations.

### Overlap (`nash-ast/src/head.rs`)

`unifiable` has no kind environment, so the caller supplies constructor
representations:

```rust
pub fn overlaps<'a>(
    left: &[Head<'a>],
    right: &[Head<'a>],
    repr_of: &dyn Fn(HeadCon<'a>) -> Option<Repr>,
    remaining: &mut usize,
) -> Result<bool, Limit>
```

`nash-can` passes a closure over `KindEnv::constructor`; tuples and functions
are `Term`. `can_equal` (reflexive `Lift` check) takes the same argument.

Inside `unifiable`, keep a narrowed class per `(side, index)` beside the
substitution, and check it whenever a variable is bound:

```rust
let mut classes: BTreeMap<(bool, u16), ReprSet> = BTreeMap::new();
let class_of = |classes: &BTreeMap<_, _>, side, index, declared| {
    classes.get(&(side, index)).copied().unwrap_or(declared)
};

if let Head::Var { index, repr } = *left.pattern {
    let mine = class_of(&classes, left.side, index, repr);
    match *right.pattern {
        Head::Var { index: other, repr: theirs } => {
            let theirs = class_of(&classes, right.side, other, theirs);
            let both = mine.intersect(theirs);
            if both.is_empty() {
                return Ok(false); // disjoint representations: no common type
            }
            classes.insert((right.side, other), both);
        }
        _ => match repr_of(right.pattern.con().expect("constructor pattern")) {
            Some(actual) if !mine.contains(actual) => return Ok(false),
            // `None`: constructor with no fixed representation (transparent
            // alias over a variable). Stay conservative and report overlap.
            _ => {}
        },
    }
    // existing occurs check and `substitution.insert` follow unchanged
}
```

`impls::insert` keeps its shape; only the call gains `&repr_of`. Delete the two
comments that state the old rule. The `StructuralEqOverride` and
`ReflexiveLiftOverlap` guards change from "bare variable" to "variable whose
class admits `Big`":

```rust
let may_be_big = |head: &Head<'_>| matches!(head, Head::Var { repr, .. } if repr.contains(Repr::Big));
```

so `impl Eq ('a : Little)` becomes legal and `impl Eq 'a` stays rejected.

### Matching (`nash-ast/src/head.rs`)

`Types` gains one query. It answers for the node itself and never chooses a
type:

```rust
pub trait Types<'a> {
    type Node: Copy;
    fn constructor(&mut self, node: Self::Node, expected: HeadCon<'a>) -> Match<Vec<Self::Node>>;
    fn equal(&mut self, a: Self::Node, b: Self::Node, remaining: &mut usize) -> Result<Match<()>, Limit>;
    /// `Yes` when `node`'s representation is known to lie in `class`, `No`
    /// when known to lie outside it, `Deferred` while its head is unknown.
    fn in_class(&mut self, node: Self::Node, class: ReprSet) -> Match<()>;
}
```

`matches` consults it on first binding only; repeats are covered by `equal`:

```rust
Head::Var { index, repr } => {
    let binding = &mut bindings[usize::from(*index)];
    if let Some(previous) = *binding {
        /* unchanged */
    } else {
        if !repr.is_all() {
            match types.in_class(argument, *repr) {
                Match::Yes(()) => {}
                Match::No => return Ok(Match::No),
                Match::Deferred => deferred = true,
            }
        }
        *binding = Some(argument);
    }
}
```

Unclassed variables skip the query, so every existing impl matches exactly as
before.

### The three `Types` implementations

**Inference (`nash-solve/src/resolve.rs`, `InferenceTypes`).** Uses the existing
oracle `representation::known` and the givens in scope. `select` gains the
kind environment and a given-class lookup for rigid variables:

```rust
struct InferenceTypes<'u, 'a> {
    uf: &'u mut UnionFind<'a>,
    kinds: &'u KindEnv<'a>,
    /// Intersection of given representation predicates on a rigid variable.
    given: &'u dyn Fn(Variable) -> ReprSet,
    allocated: &'u mut Vec<Variable>,
}

fn in_class(&mut self, node: Variable, class: ReprSet) -> Match<()> {
    if let Some(actual) = representation::known(self.uf, self.kinds, node, self.allocated) {
        return if class.contains(actual) { Match::Yes(()) } else { Match::No };
    }
    match self.view(node).0 {
        View::Rigid(variable) => {
            let proven = (self.given)(variable);
            // Every representation the givens still allow must be admitted.
            if proven.intersect(class) == proven && !proven.is_all() { Match::Yes(()) } else { Match::No }
        }
        View::Flexible | View::Application | View::Error => Match::Deferred,
        View::Named(_) | View::Tuple(_) | View::Function | View::Record(_) => Match::No,
    }
}
```

A rigid variable with no representation given cannot match a classed blanket:
nothing in its scope will ever prove the class. The user adds `Big 'a =>`.

`select` today keeps the last `Match::Yes` and has no `break`. With disjoint
classes at most one impl can answer `Yes` for a known head, and a flexible head
yields `Deferred`, which already wins over any selection
(`if deferred || ... { Selection::Deferred }`). No change to that tail.

The `given` closure is built in `solve.rs` where `select` is called, from the
same given list that discharges `Storable 'a` from `Big 'a`.

**Ground evidence (`nash-solve/src/evidence.rs`, `head::Canonical`).** Inputs
are ground, so `nash_can::kinds::repr_of` always answers. `Canonical` becomes a
struct holding `bump` and `&KindEnv`:

```rust
fn in_class(&mut self, node: &'a Located<Type<'a>>, class: ReprSet) -> Match<()> {
    match nash_can::kinds::repr_of(self.bump, self.kinds, node) {
        Some(actual) if class.contains(actual) => Match::Yes(()),
        Some(_) => Match::No,
        None => Match::Deferred,
    }
}
```

`Canonical` lives in `nash-ast`, which cannot see `nash-can::kinds`. Move the
canonical `Types` impl next to `repr_of` (into `nash-can`) or give it the same
`&dyn Fn(HeadCon) -> Option<Repr>` closure used by `overlaps`. Prefer the
closure: `canonical_view` already yields the `HeadCon`.

**Declaration-time entailment (`nash-can/src/entailment.rs`).** Arguments are
canonical types that may mention the impl's own variables. For `Type::Var`,
answer from the `given` predicates already threaded through `resolve`, with
the same rule as the rigid case above.

### Interfaces and fingerprints

Impl heads are exported inside `ImplKey`, so the class travels with them. The
contract fingerprint must change when a class changes; add a case to
`crates/nash-driver/tests/predicate_fingerprint.rs` that flips one head from
`ALL` to `Big` and asserts a different fingerprint.

### Diagnostics (`nash-report`)

Two sites print `'a{index}` for a variable head: `canonicalize.rs`
(`OverlappingImpls`) and `type_/traits.rs` (`MissingImpl` "implemented for
these heads"). Print the class when it is not `ALL`:

```rust
Head::Var { index, repr } if repr.is_all() => format!("'a{index}"),
Head::Var { index, repr } => format!("('a{index} : {})", repr_name(*repr)),
```

`repr_name` maps the five named sets (`Big`, `Const`, `Term`, `Storable`,
`Little`) exactly; those are the only non-`ALL` sets a context can produce
except `Const` via `Storable ∩ Little`, which prints `Const`.

## Behaviour changes to accept

- A wanted `T 'x` with flexible `'x` and only classed blankets available now
  defers instead of selecting eagerly. Some definitions that resolved early
  will report `AmbiguousType` if `'x` is never determined. That is the correct
  outcome: today the eager pick can be wrong.
- `impl C ('a : Big)` next to `impl C int` becomes legal.
- Nothing changes for impls with no representation predicate on a bare head
  variable.

## Chunks

Tests first in every chunk. Each chunk is one reviewed jj commit and passes
`cargo fmt --all`, strict Clippy, and `cargo test`.

- [ ] 1. `Head::Var { index, repr }` with `ReprSet::ALL` everywhere. Pure
  refactor across `nash-ast`, `nash-can`, `nash-solve`, `nash-codegen`,
  `nash-report`, `nash-parse` (13 match sites plus constructors). No snapshot
  may change.
- [ ] 2. Derive and store classes in `impls.rs`. Snapshot the canonical heads
  for inline, context, mixed, nested (`list ('a : Big)`) and repeated-variable
  spellings. Overlap still ignores them.
- [ ] 3. Classed overlap. Matrix snapshot in `nash-can/tests/impls.rs`:
  `Big`×`Little`, `Big`×`Const`, `Const`×`Term`, `Storable`×`Little`
  (overlap), `Big`×`Storable` (overlap), classed×unclassed (overlap),
  `('a : Big)`×`int` (disjoint), `('a : Big)`×`Int` (overlap),
  `list ('a : Big)`×`list ('a : Const)` (disjoint), transparent-alias head
  (conservative overlap). Update the `Eq` and `Lift` guards.
- [ ] 4. `Types::in_class` and the three implementations. Solver tests in
  `nash-solve/tests/inference.rs`: known head picks by representation in both
  declaration orders; flexible head defers then resolves; rigid with given;
  rigid without given is `MissingImpl`; class excludes the only blanket;
  repeated variable; `'f 'a` application defers. Ground evidence test selects
  the same impl inference did.
- [ ] 5. Interface fingerprint case, reporter output, scratch projects for the
  `FromData` motivating example against the real core package.
- [ ] 6. Docs: replace the two sentences in `docs/traits.md` ("Overlap" and
  the impl-head paragraph), add "Classes on impl heads" under
  `docs/kinds.md` "Representation predicates", tick SPEC.md. Changeset:
  `nash-ast` minor (public enum shape), `nash-can` / `nash-solve` /
  `nash-report` / `nash-codegen` minor or patch by dependency order.

## Out of scope

- General contexts disambiguating heads. Still forbidden.
- Negative reasoning over user traits.
- `derive` for `FromData`; that is Plan 11.
- Specialization or priority between overlapping impls.
