# Traits

Traits are Nash's type classes: `trait` declares an interface over one or
more type parameters, `impl` provides it for concrete type constructors,
and the type checker infers *qualified types* (`Eq 'a => 'a -> 'a -> bool`)
for anything that uses them. There are no runtime dictionaries. The solver
records, for every overloaded use, which impl (or which enclosing constraint)
satisfies it, and codegen specializes definitions on that evidence.

This document covers surface syntax, the static semantics, the inference
algorithm and its outputs, the core trait hierarchy, and the error cases.
Implementation steps are in [plans/03-traits.md](../plans/03-traits.md).

## Concepts

| Term | Meaning |
|---|---|
| Trait | A named set of method signatures over type parameters, with optional superclasses and default method bodies. |
| Impl | The methods of a trait for a specific *instance head*: a type constructor applied to distinct type variables. |
| Predicate | `Tr t1 .. tn`: the claim that the types `t1..tn` have an impl of `Tr`. Written `Eq 'a`. |
| Context | The predicates in front of `=>` in a signature, trait, or impl. |
| Qualified type / scheme | `forall 'a 'b. (Eq 'a, Show 'b) => 'a -> 'b -> string`. The context is part of the scheme. |
| Given | A predicate the current definition may assume: it is in the enclosing annotation, impl, or trait context. |
| Wanted | A predicate an expression needs: produced by every use of an overloaded name or literal. |
| Evidence | The solver's answer for a wanted predicate: an impl (with sub-evidence for its context), a given, or a superclass of a given. |
| Specialization | Codegen copies a definition once per distinct evidence vector it is used with. This is Nash's monomorphization. |

## Surface syntax

```elm
-- a trait with a superclass, two methods, one default body
trait Eq 'a => Ord 'a where
    compare : 'a -> 'a -> ordering

    lt : 'a -> 'a -> bool
    lt a b = compare a b == LT

-- multi-parameter trait
trait Lift 'small 'big where
    lift : 'small -> 'big
    lower : 'big -> 'small

-- impl for a base type
impl Ord int where
    compare = Builtin.compareInteger

-- impl with a context
impl Eq 'a => Eq (List 'a) where
    eq xs ys = ...

-- impl for a type constructor (higher-kinded trait)
impl Functor List where
    map = List.map

-- multi-parameter impl
impl Lift int Int where
    lift = Builtin.iData
    lower = Builtin.unIData

-- contexts in annotations
member : Eq 'a => 'a -> List 'a -> bool
render : (Show 'a, Show 'b) => 'a -> 'b -> string
```

Layout: `where` opens a layout block like `let`. Every item in the block is
either a method signature `name : type` or a method definition
`name args = body`. Inside a `trait` block a definition is a default body
and must follow its signature. Inside an `impl` block only definitions are
allowed.

### Grammar

The grammar lives in [syntax.md](syntax.md): `trait_decl`, `trait_item`,
`impl_decl`, `impl_item`, `context`, `type_scheme`, and `type_param`
(which allows a kind annotation, `('f : Big -> Big)`). Instance heads are
`type_term`s syntactically; the shape rules below are checked in
canonicalization, not by the parser.

Type variables are written `'a`; a bare lowercase name in type position is
a little type (`int`, `list 'a`). An instance head names a constructor and
applies it to zero or more *distinct* type variables. `impl Functor List`
applies `List` to nothing: the head has kind `Big -> Big`.

Exposing: `exposing (Ord)` exports the trait and all its methods. There is
no `Ord(..)` form. Impls are never listed; every impl is visible wherever
both its trait and its head type are visible.

## Static semantics

### Trait declarations

`trait C => T 'p1 .. 'pn where ...`:

- `n >= 1`. Parameters are distinct type variables. Every parameter must
  occur in every method signature (the method type would otherwise be
  ambiguous; see [ambiguity](#ambiguity-and-defaulting)).
- The context `C` lists the superclasses. Each superclass predicate may
  mention only the trait's own parameters. The superclass graph across a
  build must be acyclic.
- Each method `m : Cm => t` gets the *method scheme*
  `forall ps fv(t). (T 'p1 .. 'pn, Cm) => t`. The trait predicate always
  comes first in the context. The method's own context `Cm` may constrain
  extra variables (`traverse : Applicative 'f => ...` inside `Traversable`).
- A default body is checked exactly like a top-level definition with that
  method scheme as its annotation: the parameters are rigid and the trait
  predicate plus its superclass closure are given.
- Each parameter has a kind inferred from the method signatures. The trait
  stores a *kind scheme*: `Functor 'f` gets `'f : k1 -> k2` with kind
  variables `k1`, `k2`. See [kinds.md](kinds.md).

### Impl declarations

`impl C => T h1 .. hn where ...`:

- `T` must resolve to a trait of arity `n`.
- Each head has an outer named constructor, unit, or tuple. Constructor
  arguments are recursive type patterns, including concrete types and nested
  applications: `list int`, `list (pair 'k 'v)`, and `List 'a` are legal.
  Variables may recur across patterns; every occurrence denotes the same type.
  Matching must preserve that equality. Bare variable heads and function heads
  remain excluded; reflexive Big Lift remains a compiler-provided rule.
  This rule applies uniformly to user and core impls, with no Map-specific
  exception or enumeration of permitted nested shapes.
  Aliases remain nominal: matching uses their qualified names and arguments,
  not structural equality of their expanded bodies.
  A partially applied alias retains the unsupplied suffix of its formal
  parameters. Applying it binds those parameters in declaration order;
  the alias body stays closed over its formals until saturation. Supplied
  caller variables are not captured by remaining formal names.
  Solved aliases retain their original parameterized body as well as the
  substituted body. Interfaces carry both, so later inference can recover a
  partial constructor from a saturated alias without reverse-substituting
  caller types. Substitution changes the filled body and supplied arguments;
  it never changes the closed template.
  Evidence identity uses the qualified alias name, supplied arguments and
  remaining parameters. Whether its body is open or already substituted does
  not change the evidence key.
- The context `C` may only mention the variables of the heads.
- Kinds: each head's kind must instantiate the trait's kind scheme. `impl
  Functor List` instantiates `k1 -> k2` at `Big -> Big`; `impl Functor list`
  at `Storable -> Const`; `impl Functor option` at `Any -> Term`.
  Different applications of an abstract constructor check their element
  kinds independently against its domain bounds. For example, map over cons
  may turn Const integers into Term tuples; map over builtin list may not
  produce those tuples. Do not unify the actual input and output element
  kinds merely because they use the same abstract constructor.
- Every method without a default must be defined. Defining a name that is
  not a method of `T` is an error. Each method body is checked against the
  method scheme with `'p1 .. 'pn := h1 .. hn`, the head variables rigid, and
  the impl context (plus superclass closure) given.
- Superclasses: for every superclass `S` of `T`, `S h1 .. hn` must be
  satisfiable from the impl table plus the impl's own context. `impl Ord
  int` needs `impl Eq int` somewhere in the build. `impl Eq 'a => Ord (List
  'a)` needs `Eq (List 'a)` given `Eq 'a`. Checked when the impl is added
  to the table, so declaration order does not matter.

### Coherence

An impl's identity retains its trait, full recursive head patterns and kind
requirements, with bound variables normalized independently of their spelling.
Outer constructors may index candidates, but are not a complete impl identity
or an overlap test. Matching substitutes through every nested argument and
checks repeated variables, kind bounds and trait prerequisites.

**Orphan rule.** An impl in module `M` is legal only if the trait `T` is
defined in `M`, or at least one head constructor is defined in `M`. Unit and
tuples count as defined in `nash/core`. This is Rust's rule specialized to
heads that are always constructors. There are no uncovered type parameters
to worry about because a head is never a bare variable, so Rust's extra
ordering condition for multi-parameter traits is vacuous.

**Overlap.** Two impls overlap when their full head patterns can match a
common well-kinded type assignment. Freshen their variables independently
before checking this. Trait prerequisites do not establish disjointness merely
because an impl is currently absent. Thus `SomeTrait (list int)` and
`SomeTrait (list bytes)` are disjoint, while `SomeTrait (list 'a)` overlaps
both. Overlap remains an error; declaration order does not select an impl.
Check this across
all build interfaces as well as within a module. In particular, separate
modules in `nash/core` may both satisfy the orphan rule for unit or tuple
heads; that ownership does not permit duplicate impls.

Consequence: the impl table is global. Canonicalization builds it from
every interface in the build plus the current module, and a module can use
any impl whose trait and head it can name.

### Qualified types

A value's scheme is `forall vs. C => t` where `C` is a set of predicates.
Contexts are not limited to variables: `Lift 'a Data` is a legal retained
predicate (Haskell's FlexibleContexts). Two predicates are equal when trait
and argument types are equal.

Annotations with contexts are checked, not trusted: the body may only use
predicates *entailed* by the annotation's context, and the solver reports
when it needs one that is missing.

### Entailment

`Given ⊢ P` when one of:

- `by_given`: `P` is in `Given`, or is reachable from a given by following
  superclasses (`Ord 'a` gives `Eq 'a`).
- `by_instance`: every argument of `P` has a head constructor, an impl with
  `P`'s key exists, and `Given ⊢ Q` for every `Q` in the impl's context
  instantiated at `P`'s arguments.

After checking givens, resolution recognizes the compiler-owned reflexive
rule only for package `nash/core`, module `Lift`, trait `Lift`. Both arguments
must already be equal and their kind must be proven Big. Resolution must
not unify unknown arguments or narrow a rigid variable's kind to select
this rule. A same-named trait elsewhere receives no special behavior.
Explicit impls that can overlap this rule are rejected: the same nominal
constructor at the same application arity conflicts when corresponding
argument kinds can unify and the resulting type can be Big. This check
does not assume that different head variable names make impls disjoint.

Superclass checking during canonicalization performs a bounded search.
A predicate already active in the instance-resolution chain is a cycle,
not a proof. Expanding flexible contexts are bounded too: each impl check
allows 16,384 work steps and fewer than 128 nested resolution or type
conversion calls. Predicate comparisons count toward the work limit.
Failure reports the instantiated superclass requirement and distinguishes
a missing proof, a cycle, and the search limit. The given superclass
closure is computed once per impl and reused for its requirements.

Evidence is the derivation:

```
Evidence = Impl  { impl_: ImplRef, type_args: [Type], args: [Evidence] } -- by_instance; type_args = head vars
         | ReflexiveLift { typ: Type }                -- compiler rule for equal Big types
         | Given { binder: NodeId, index: u16 }         -- i-th predicate of the scheme of definition `binder`
         | Super { of: Evidence, index: u16 }           -- i-th superclass of an evidence's trait
```

`NodeId` is the identity of an AST node (its arena address), the key
[codegen.md](codegen.md) uses for every per-node solver result.

Context reduction: a retained context never contains both `Ord 'a` and
`Eq 'a`; the superclass is dropped and its uses get `Super` evidence.

## Inference

Nash keeps Elm's rank-based solver (`nash-solve`). Predicates ride along
with unification variables.

### Where predicates live

Each unification variable's descriptor carries the set of predicates that
mention it. A predicate with several arguments is attached to every
argument variable. Unifying two variables unions their predicate sets. A
variable unified with a structure keeps its predicates: `Show 'a` on a
variable that became `List int` is now the ground predicate `Show (List
int)`.

### Producing wanted predicates

- A use of a name with scheme `forall vs. C => t` (a `Constraint::Local`
  on a generalized variable, or a `Constraint::Foreign` with an annotation)
  instantiates `vs` with fresh variables and creates one wanted predicate
  per element of `C`, tagged with the use site's region and its index in
  `C`.
- An integer literal is typed as a use of `fromInt : FromInt 'a => 'a`;
  a string literal as `fromString : FromString 'a => 'a`; a bytes literal
  as `fromBytes : FromBytes 'a => 'a`.
- A literal in a pattern additionally wants `Eq 'a` (matching needs
  equality).
- A resolved impl's context produces sub-wanteds tagged with the parent
  predicate.

Wanted predicates are queued per rank, parallel to the solver's variable
pools.

### Generalization

At a generalizing `Let` (Elm's `CLet rigids flexs header headerCon
bodyCon`, third branch), after `generalize` has adjusted ranks, every
wanted predicate queued at the young rank is classified by the ranks and
contents of its argument variables:

1. **Resolve.** If every argument has a head constructor, look up the impl
   table (`by_instance`). Missing impl is an error. The impl's context
   becomes new wanteds, classified in turn.
2. **Discharge by given.** If an argument is a rigid variable of this Let,
   or the predicate is ground but appears in the given set, search the
   givens of this and all enclosing Lets (`by_given`, with superclass
   closure). Not found is an error: the annotation lacks a constraint.
3. **Defer.** If no argument was generalized here (all variable arguments
   belong to an outer scope), move the predicate to the outer rank's queue.
4. **Retain.** Otherwise at least one argument is a variable generalized by
   this Let. The predicate joins the scheme's context. Its index in the
   context, in creation order, is the `Given` index used inside the body.

Inferred contexts keep the first exact duplicate and remove requirements
entailed by another retained predicate's superclass closure. Compare the
substituted arguments without unification. Assign slots after reduction,
preserving the surviving predicates' creation order; removed requirements
refer to those final slots through `Given` or `Super` evidence. Declared
annotation contexts retain their declared order and slots.

Untyped definitions therefore infer their contexts (`member x xs = ...`
gets `Eq 'a => 'a -> List 'a -> bool`). Typed definitions get the same
treatment; the rigid variables make step 2 apply, and the annotation's
context is the given set.

Impl selection waits for the surrounding definition's type equalities.
Captured outer variables must also be fixed before rejecting an enclosing
given in favor of an impl. Impl contexts may expand indefinitely; resolution
is bounded to 128 levels and 16,384 attempts per definition boundary. Exceeding
either bound reports `ImplResolutionLimit` at the originating call. An
exhausted search never counts as evidence that a constraint was satisfied.

Elm's `SaveTheEnvironment` step is unchanged: top-level annotations are
read back with `to_annotation`, which now also emits the retained
predicates on the reached generalized variables as the annotation's
context.

### Ambiguity and defaulting

A retained predicate is *ambiguous* when one of its generalized variables
is not reachable from the definition's header type. `x = show 1` retains
`FromInt 'a` and `Show 'a` but `'a` does not appear in `x : string`.
Use the full types of all members when checking a shared untyped recursive
context. An annotated body can introduce a hidden flexible variable too;
check its pending requirements at the same boundary. Report the innermost
definition and list equal requirements once.

For each ambiguous variable, in order:

- If its predicates include exactly one distinct literal trait from package
  `nash/core`, module `Literal` (`FromInt`, `FromString`, `FromBytes`), unify
  it with `Builtin.int`, `Builtin.string`, or `Builtin.bytes` respectively.
  The predicate's sole argument must be that variable, not a type containing
  it. Repeated requirements of the same trait still select one default.
- Otherwise report an ambiguous type error listing the predicates.

Apply available defaults and retry resolution before reporting remaining
ambiguity: an impl selected by one default can introduce the literal
requirement for another variable. Keep the same enclosing givens, boundary
rank, predicate origins and resolution limits across these rounds.

Defaulting runs at every generalizing Let, not only at the top level, so
the error region is the innermost definition that lost the variable. This
is the one place the design departs from Haskell; a variable an enclosing
annotation could still fix belongs to an outer rank and is deferred, not
defaulted, so the outcome is the same.

There is no monomorphism restriction: `x = 1` is `x : FromInt 'a => 'a`
and is specialized per use. A polymorphic constant is evaluated once per
evidence vector, not once per program.

### Recursion

Recursive groups follow Elm: untyped definitions in a group are monomorphic
within the group and share one scheme (one context) after generalization.
Typed definitions are instantiated polymorphically at recursive calls.

**Polymorphic recursion with growing evidence is an error.** Track each
definition's context slots separately. A local call connects each callee
slot to the `Given` slots used to construct its evidence. A dependency
through `Impl` adds a wrapper; `Given` and superclass projections only
forward evidence. Reject a dependency cycle containing an impl wrapper.
This includes cycles through local helpers with their own declared contexts.
Closed evidence breaks the dependency: an impl around another slot that is
replaced with closed evidence on the next call does not grow indefinitely.
Example:

```elm
nest : Show 'a => int -> 'a -> string
nest n x = if n == 0 then show x else nest (n - 1) [x]
```

The recursive call needs `Show (List 'a)`, resolved as `Impl (Show List)
[Given nest 0]`. Rejected at the definition. Polymorphic recursion whose
evidence is exactly the definition's own `Given`s (or closed, like `Impl
(Show int) []`) is fine, as is polymorphic recursion without any trait
constraint: UPLC is untyped, so a definition with no evidence has exactly
one specialization.

### Solver output

`nash_solve::run` returns the top-level `Annotations` (now with contexts)
plus the per-node table that [codegen.md](codegen.md) consumes,
`SolvedTypes`, keyed by `NodeId`:

```rust
pub struct SolvedTypes<'a> {
    pub exprs: HashMap<NodeId, &'a Located<Type<'a>>>,      // type of every expression node
    pub patterns: HashMap<NodeId, &'a Located<Type<'a>>>,   // type of every pattern node
    /// Every use of a generalized name, method, operator, literal, or `<-`:
    /// the scheme's type variables and one evidence per context predicate.
    pub instances: HashMap<NodeId, Instance<'a>>,
    /// Every named definition and generalized destructuring pattern: its scheme.
    pub schemes: HashMap<NodeId, Scheme<'a>>,
}

pub struct Instance<'a> {
    pub type_args: &'a [&'a Located<Type<'a>>],   // in `Annotation.free_vars` order
    pub evidence: &'a [Evidence<'a>],             // in `Annotation.context` order
}

pub struct Scheme<'a> {
    pub annotation: &'a Annotation<'a>,   // free vars, kinds, context, type
    pub binder: NodeId,                   // what `Evidence::Given` inside the body refers to
}
```

Traits fill `instances` and `schemes`; the codegen plan fills `exprs` and
`patterns`. Contexts of unexported local definitions exist only in
`schemes`, never in an interface, so the driver retains every module's
canonical AST and `SolvedTypes` in the build-wide arena (`SolvedModule`
in [cli.md](cli.md)'s build pipeline) until codegen runs.

A generalized let-destructuring owns one aggregate scheme keyed by
`NodeId::pattern` of its original root pattern. Its type is the full RHS/pattern
type, and its context and quantifier order are shared by all extracted names.
A use of an extracted name instantiates the aggregate type, context and selected
component together, then returns the component type. Its `Instance` contains
all aggregate type arguments, including those absent from that component, and
all aggregate evidence slots. Codegen associates that lexical name with its
root pattern scheme and projection; evidence inside the RHS refers to the
pattern binder. Tuple, record and alias patterns use the same rule. This
preserves polymorphic destructuring without losing qualified constraints.

### Resolving outside the solver

Power-assert ([testing.md](testing.md)) needs `Show` at whatever ground
type a subexpression has, and `@derive` ([macros.md](macros.md)) needs to
know whether a field type already has an impl. Both run after solving, on
fully ground canonical types, so the resolver is also exposed as a plain
function:

```rust
pub fn resolve<'a>(
    bump: &'a Bump,
    tables: &Tables<'a>,
    pred: &Pred<'a>,          // args are ground `Located<Type>`s: no `Type::Var`
) -> Result<Evidence<'a>, nash_solve::evidence::Error<'a>>
```

It is the same `by_instance` walk without unification variables: match
each argument's head constructor against the impl table, instantiate the
impl's context at the argument's type arguments, recurse. A ground
predicate never needs `Given`, so the result is a closed tree of `Impl` and
exact-core `ReflexiveLift` evidence. Reflexive Lift requires nominally equal
arguments with an already-proven Big kind; it does not select a kind.
Failures distinguish `MissingImpl`, `NonGround`, and `Limit`, retaining the
failed predicate. Resolution and type traversal share a 16,384-step work
budget, with a depth limit of 128. Context substitution is charged before
allocation. Callers supply well-kinded canonical types and complete tables.

Codegen never looks at types to specialize. It walks a definition with a
substitution `Given { binder, index } -> Evidence` for the definition's
context, and at each recorded use site it substitutes, then either inlines
the method body of the resulting `Impl` or requests a specialization of the
callee at the resulting evidence vector. Two calls of `member` at `int` and
`Int` produce two copies of `member`; two calls at `int` share one.

`Super { of, index }` after substitution is an `Impl`, and the superclass
impl is found by key: the `index`-th superclass of the impl's trait, at the
impl's heads. The impl check guarantees it exists.

## `do` notation

`do` is a layout block of statements. It desugars in canonicalization,
before type inference:

```
do { p <- e; rest }   =  bind e (\p -> do { rest })
do { e; rest }        =  bind e (\_ -> do { rest })
do { let defs; rest } =  let defs in do { rest }
do { e }              =  e
```

`bind` is `Monad.bind`, always resolved as the core method regardless of
what is in scope. The `<-` pattern must be irrefutable (a variable, `_`, a
tuple, or a record pattern); a refutable pattern is a canonicalization
error since there is no `fail`. `pure` is not inserted. The synthetic
`bind` use has its own generated method `NodeId` for evidence; the statement
region supplies diagnostics.

## Core trait hierarchy

Decision: shipped in `nash/core` as one module per trait (`Eq`, `Ord`,
`Show`, `Num`, `Integral`, `Semigroup`, `Monoid`, `Functor`,
`Applicative`, `Monad`, `Lift`, `Data` for `ToData`/`FromData`, and
`Literal` for the three literal traits), all imported implicitly with the
trait and its methods exposed (like Elm's default imports of `Basics`).
`Prelude` holds the `infix` declarations that bind operators to methods and
the impls for the prelude types. The compiler recognizes these core identities:
`Literal.FromInt`, `Literal.FromString`, `Literal.FromBytes`, `Eq.Eq`
(literal patterns), `Num.Num` (prefix negation), `Monad.Monad` (`do`), and
`Lift.Lift` (reflexive Big evidence). Negation uses the checked `Num.negate`
method annotation, independent of lexical values named `negate`. The declarations match
[stdlib.md](stdlib.md):

```elm
trait Eq 'a where
    eq : 'a -> 'a -> bool                       -- (==)
    neq : 'a -> 'a -> bool                      -- (/=)
    neq a b = not (eq a b)

trait Eq 'a => Ord 'a where
    compare : 'a -> 'a -> ordering
    lt, le, gt, ge : 'a -> 'a -> bool           -- defaults via compare; (<) (<=) (>) (>=)
    max, min : 'a -> 'a -> 'a                   -- defaults via lt

trait Show 'a where
    show : 'a -> string

trait Num 'a where
    add, sub, mul : 'a -> 'a -> 'a              -- (+) (-) (*)
    negate : 'a -> 'a                           -- prefix `-`

trait Num 'a => Integral 'a where
    div, mod : 'a -> 'a -> 'a                   -- (/) (%): floor division, modInteger
    quot, rem : 'a -> 'a -> 'a                  -- quotientInteger, remainderInteger

trait Semigroup 'a where
    append : 'a -> 'a -> 'a                     -- (++)

trait Semigroup 'a => Monoid 'a where
    empty : 'a

trait Functor 'f where
    map : ('a -> 'b) -> 'f 'a -> 'f 'b

trait Functor 'f => Applicative 'f where       -- map is (<$>)
    pure : 'a -> 'f 'a
    apply : 'f ('a -> 'b) -> 'f 'a -> 'f 'b     -- (<*>)

trait Applicative 'm => Monad 'm where
    bind : 'm 'a -> ('a -> 'm 'b) -> 'm 'b      -- (>>=), `do`

trait ToData ('a : Big) where
    toData : 'a -> Data

trait FromData ('a : Big) where
    fromData : Data -> 'a                       -- shallow: reinterprets the constant
    validateData : Data -> 'a                   -- full structural check; traps on bad data

trait Lift 'small 'big where
    lift : 'small -> 'big
    lower : 'big -> 'small

trait FromInt 'a where
    fromInt : int -> 'a
trait FromString 'a where
    fromString : string -> 'a
trait FromBytes 'a where
    fromBytes : bytes -> 'a
```

Notes:

- `fromInt : int -> 'a` takes a UPLC integer constant. The compiler types
  the literal `42` as `fromInt 42` where the inner `42` is the raw `int`
  constant, so `impl FromInt int` is `fromInt x = x` and specialization
  inlines it away. Same for `string` and `bytes`.
- `impl FromInt Int` builds `I n`; `impl FromString bytes` is a
  compile-time UTF-8 encode. Users may add impls for their own types
  (`impl FromInt Lovelace`).
- `Eq` and `Ord` are also what `case` on literal patterns uses.
- `Num` has no `fromInteger`; literals go through `FromInt`. `/` and `%`
  are `Integral` methods, so a `Num` impl for a non-integer type (a
  fixed-point `Lovelace`, say) does not have to invent division.
- Operators are `infix` declarations in `Prelude` bound to methods
  (`infix non 4 (==) = eq`, `infix left 6 (+) = add`, `infix left 7 (/) = div`,
  `infix left 1 (>>=) = bind`, ...; the full table is in
  [stdlib.md](stdlib.md)). An operator whose function is a method
  canonicalizes to a `Binop` whose annotation is the checked method scheme.
  Interfaces retain the backing method's defining module independently of
  the module that declares the operator. Operator values and sections use
  the same scheme; each operator node owns its solved evidence.
- Kinds: `ToData`/`FromData` parameters are `Big`; `Lift` pairs a `Const`
  or `Term` type with a `Big` type; `Functor`/`Applicative`/`Monad` are
  kind-polymorphic (`List`, `list`, `option`, `fuzzer`).
- `@derive(Eq, Ord, Show, ToData, FromData)` generates impls as macros
  ([macros.md](macros.md)); the generated impls are ordinary impls subject
  to the orphan rule (always satisfied: the type is local).

## Interfaces

A module's interface gains:

- `traits`: trait parameters, kind schemes, superclasses, and method
  schemes, with export visibility. Private metadata is retained to check
  exported schemes that reference it; private trait and method names do
  not enter import scopes. Default bodies are not in the interface;
  codegen reads them from the defining module.
- `impls`: every impl of the module, regardless of the export list: trait,
  heads, context, and the set of methods defined (so codegen knows which
  ones fall back to defaults), plus its defining module and source region.
- `values` annotations now carry contexts.

Canonicalization returns resolution tables built from every available
build interface and the current module. These tables retain private trait
metadata without adding private names to source import scopes. Duplicate
impl keys report the defining module and source region for both entries.

Importing a trait brings its method names into the value namespace with
their method schemes. Method uses canonicalize to `Expr::VarMethod`, never
to `VarTopLevel`/`VarForeign`, even inside the defining module: the method
has a declared scheme, so it is always constrained through its annotation.

## Interactions

- **Kinds** ([kinds.md](kinds.md)): trait parameters carry a
  `KindScheme` inferred from the method signatures; impl heads instantiate
  it; `'f 'a` in signatures requires type-variable application in the
  solver (`FlatType::AppV1`). Kind bounds on a value's type variables
  (`'a : Storable` in `cons : 'a -> list 'a -> list 'a`) are *kind
  predicates*: they ride the same machinery as trait predicates (attached
  to the variable, instantiated per use, classified at generalization) but
  produce no evidence. `cons (Some 1) nil` is rejected at the call site
  with a kind error, and `impl Lift 'a 'a` carries `Big 'a`.
- **Representation** ([representation.md](representation.md)): `Lift`,
  `ToData`, `FromData` are the only bridges between reprs. Nothing in trait
  resolution depends on reprs; specialization is by evidence only.
- **Macros** ([macros.md](macros.md)): `@derive` expands to `impl` decls
  before canonicalization of the expanded module; `@derive` on a type in
  another module is an orphan error like any hand-written impl.
- **Codegen** ([codegen.md](codegen.md)): consumes `SolvedTypes`;
  specialization worklist keyed by (definition, type arguments, ground
  evidence vector).
- **Testing** ([testing.md](testing.md)): the little type `fuzzer 'a`
  (constructor `Fuzzer`) has a `Monad` impl; `prop` bodies are `do` blocks.
- **Diagnostics** ([diagnostics.md](diagnostics.md)): the errors below are
  `nash-can` errors (declaration checks) or `nash-constrain` type errors
  (resolution), rendered by `nash-report`.

## Errors

Sketches in Elm's voice. Regions point at the use site or declaration named.

**Missing impl** (type error, at the use site)

```
-- MISSING IMPL ------------------------------------------------- Main.nash

I cannot show this value:

12|     trace (show (a, b))
                     ^^^^^^
It has type:

    ( int, bytes )

but there is no `impl Show (int, bytes)` in this project or its
dependencies. Impls of Show exist for: int, string, bytes, List 'a, ...

Hint: Write `impl Show (int, bytes) where ...` in this module, or turn the
tuple into a type of your own and derive Show for it.
```

**Ambiguous type** (type error, at the definition)

```
-- AMBIGUOUS TYPE ------------------------------------------------ Main.nash

I cannot pick a type for this expression:

5|  width = size (empty <> empty)
                 ^^^^^^^^^^^^^^^^
The type variable 'a in

    Monoid 'a, Sized 'a

does not appear in the type of `width`, so nothing decides what 'a is.

Hint: Add an annotation to a subexpression, for example
`(empty : list int)`.
```

**Orphan impl** (canonicalization error)

```
-- ORPHAN IMPL ---------------------------------------------------- Main.nash

This impl does not belong here:

8|  impl Show List where
    ^^^^^^^^^^^^^^
The trait Show is defined in `Show` and the type List is defined in `List`.
An impl must live in the module that defines its trait or in the module
that defines one of its types. Otherwise two modules could give Show
different meanings for List, and I could not tell which one you meant.

Hint: Wrap the type in one of your own: `type Rows = Rows (List Row)`.
```

**Overlapping impls** (canonicalization error)

```
-- OVERLAPPING IMPLS ---------------------------------------------- Main.nash

There are two impls of Eq for Color:

10|  impl Eq Color where
     ^^^^^^^^^^^^^
and

24|  impl Eq Color where
     ^^^^^^^^^^^^^
Only one impl is allowed per trait and type. Delete one, or merge them.
```

**Missing method** (canonicalization error)

```
-- MISSING METHOD ------------------------------------------------ Main.nash

This impl of Ord for Color does not define `compare`:

10|  impl Ord Color where
     ^^^^^^^^^^^^^^
`compare` has no default body, so every impl must define it. The methods
of Ord are: compare, lt, le, gt, ge, min, max.
```

**Superclass not satisfied** (canonicalization error)

```
-- MISSING SUPERCLASS --------------------------------------------- Main.nash

This impl of Ord for Color needs an impl of Eq for Color:

10|  impl Ord Color where
     ^^^^^^^^^^^^^^
Ord is declared as `trait Eq 'a => Ord 'a`, so anything with Ord must
also have Eq. I could not find `impl Eq Color` anywhere.

Hint: Add `@derive(Eq)` to `type Color`, or write the impl by hand.
```

**Missing constraint** (type error, at the use inside a typed definition)

```
-- MISSING CONSTRAINT --------------------------------------------- Main.nash

This use of `==` needs `Eq 'a`:

14|      if x == y then
            ^^^^^^
but the annotation for `same` does not provide it:

12|  same : 'a -> 'a -> bool

Hint: Change the annotation to `same : Eq 'a => 'a -> 'a -> bool`.
```

**Polymorphic recursion** (type error, at the recursive call)

```
-- POLYMORPHIC RECURSION ------------------------------------------ Main.nash

This recursive call would need an endless chain of impls:

5|      nest (n - 1) [x]
        ^^^^^^^^^^^^^^^^
`nest` is defined for any 'a with Show 'a, but this call uses it at
List 'a. Compiling `nest` for 'a needs `nest` for List 'a, which needs
`nest` for List (List 'a), and so on forever.

Nash compiles each function once per set of impls it uses, so a function
cannot call itself at a bigger type under a trait constraint.
```

Also reported: unknown trait, wrong trait arity, bad instance head (nested
type, repeated variable, function type, bare variable), unknown method in
impl, duplicate trait, duplicate method, method signature missing a trait
parameter, context mentioning a variable outside the type, cyclic
superclasses, refutable pattern in `<-`, kind mismatch between head and
trait parameter (reported by the kind checker).

## Lenient mode

Macro expansion ([macros.md](macros.md)) type-checks partially expanded
modules. In `Mode::Lenient` the resolver drops any predicate it cannot
discharge (missing impl, missing constraint, ambiguous variable,
polymorphic recursion) instead of reporting it, and records no evidence
for it. `Mode::Strict`, the default, is everything above.

## Open questions

- **Method-bound operators inside the defining module.** Today a module's
  own `infix` declarations do not enter its env (Elm rule); operators
  bound to local methods therefore need the method called by name in the
  defining module. `nash/core` is written that way.
