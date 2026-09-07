# Kinds and representation

Nash has two separate questions about a type, answered by two separate
mechanisms:

1. **Kind**: how many type arguments does it take? Haskell 98 kinds,
   `Type` and `k1 -> k2`, inferred by unification.
2. **Representation**: which UPLC value shape do its values have? `Big`
   (Plutus `Data`), `Const` (UPLC builtin constant), or `Term` (UPLC
   `constr` term or lambda). Decided by the head constructor of a ground
   type; enforced on polymorphic code by **representation predicates**
   resolved through the trait machinery of [traits.md](traits.md).

Representation is not part of the kind. This document supersedes the
subkinding design (`Big -> Big -> Const`, `Storable` bounds on kind
variables, finite satisfiability); see "Why not subkinding" at the end.
[overview.md](overview.md) is authoritative where the two disagree.

Status: implemented by [plans/02-kind-predicates.md](../plans/02-kind-predicates.md).
The retained-obligation engine and value-kind metadata are removed.

## Purpose

UPLC has three value shapes that cannot be mixed:

| Representation | Runtime value | Examples |
|---|---|---|
| `Big` | a Plutus `Data` constant | `Int`, `Bytes`, `List 'a`, `Map 'k 'v`, `Data`, user `type Foo`, `type alias Foo` |
| `Const` | a UPLC builtin constant | `int`, `bytes`, `string`, `bool`, `unit`, `list 'a`, `pair 'a 'b`, `array 'a`, `bls_g1`, `bls_g2`, `bls_mlr`, `value` |
| `Term` | a UPLC `constr` term, or a lambda | user `type option 'a`, tuples, little records, `'a -> 'b` |

A builtin list can only hold constants, a `Data.Constr` can only hold
`Data`, and `if` needs a `bool`. These facts must be static: codegen never
inspects a value to pick a representation.

## Kinds

```
Kind ::= Type | Kind -> Kind | k        -- k: inference variable
```

Kind inference is Haskell 98:

- Every type constructor has a kind `k1 -> ... -> kn -> Type`. Every fully
  applied type has kind `Type`.
- `'f 'a` requires `'f : k -> Type` for the kind `k` of `'a`. Two uses
  `'f 'a` and `'f 'b` in one signature give `'a` and `'b` the same kind.
- Unification with an occurs check. `type self 'f = Self ('f 'f)` is an
  infinite kind, as in Haskell.
- Unconstrained kind variables default to `Type` at generalization
  (`type tag 'a = Tag` gives `tag : Type -> Type`). There is no kind
  polymorphism in v1.
- Traits: `trait Functor 'f where map : ('a -> 'b) -> 'f 'a -> 'f 'b`
  gives `'f : Type -> Type`. An impl head must have exactly that kind:
  `impl Functor List`, `impl Functor list`, `impl Functor option` all do.
  `impl Functor Map` does not (`Map : Type -> Type -> Type`); `impl Functor
  (Map 'k)` does.

Kinds never mention `Big`, `Const`, or `Term`.

There is no surface syntax for kinds. The former `('f : Big -> Big)`
annotation is removed; `('a : Storable)` remains and is representation
sugar (below).

## Representation of a ground type

`repr(t)` is a function of `t`'s head constructor:

| Head | `repr` |
|---|---|
| `Int`, `Bytes`, `Data`, `List _`, `Map _ _` | `Big` |
| uppercase user union, uppercase alias | `Big` |
| `int`, `bytes`, `string`, `bool`, `unit`, `list _`, `pair _ _`, `array _`, `bls_*`, `value` | `Const` |
| lowercase user union, tuple, function, little record alias | `Term` |
| lowercase alias of a non-record body | `repr` of the body |

Arguments never change the head's representation: `List (option int)` is
still `Big` (and ill-formed for a different reason, see below). This is what
makes representation a *predicate* rather than a kind: it is decidable as
soon as the head is known, and deferrable while the head is a variable.

The casing rule is therefore a definition, not an inference:

- Uppercase type name: `Big`. Every constructor field must be `Big`; every
  field of an uppercase record alias must be `Big`.
- Lowercase union: `Term`. Fields may have any representation.
- Lowercase alias: the body must be `Little` (`Const` or `Term`). A
  lowercase record alias is `Term` with fields of any representation.

```elm
type Datum = Datum { owner : Bytes, deadline : Int }   -- Big, fields must be Big
type step 'a = Done 'a | Next int 'a                    -- Term, fields any
type alias Vault = { owner : Bytes, amount : Int }      -- Big record, fields Big
type alias acc = { total : int, seen : list Int }       -- little record, fields any
type alias Id = Int                                     -- ok
type alias id = Int                                     -- error: lowercase alias, Big body
type alias count = int                                  -- ok
```

## Representation predicates

Five compiler-owned traits live in `Builtin` and are re-exposed by the
prelude. They have no user impls; the compiler resolves them structurally
from the head constructor:

| Predicate | Holds when |
|---|---|
| `Big t` | `repr(t) = Big` |
| `Const t` | `repr(t) = Const` |
| `Term t` | `repr(t) = Term` |
| `Storable t` | `Big t` or `Const t` (what `list`, `array`, `pair` can hold) |
| `Little t` | `Const t` or `Term t` (what a lowercase alias may be) |

They are ordinary predicates in every other respect: they appear in scheme
contexts, are instantiated with the type, are discharged by structural
resolution when the argument's head is known, are deferred (retained in the
generalized scheme) while it is a variable, and can be written by the user
in a context: `cons : Storable 'a => 'a -> list 'a -> list 'a`. The
annotation form `('a : Storable)` is sugar for that context entry;
`('a : Big)`, `('a : Const)`, `('a : Term)` likewise. There is no arrow
form: kinds have no syntax.

Implication between them is expressed as superclasses, so plan 03's
superclass entailment discharges a wanted `Storable 'a` from a given
`Big 'a` with no new machinery:

| Trait | Superclasses |
|---|---|
| `Big` | `Storable` |
| `Const` | `Storable`, `Little` |
| `Term` | `Little` |
| `Storable`, `Little` | none |

A context that requires incompatible representations of one variable
(`Big 'a` and `Const 'a`; `Big 'a` and `Little 'a`) is rejected at the
annotation with `ContradictoryRepresentation`: intersect the admitted
sets of every repr predicate on the variable; empty means no type can
ever satisfy the scheme. `Storable 'a` with `Little 'a` is a legal (narrow)
context: only `Const` heads satisfy it.

Resolution never chooses a type: `Storable 'a` on a flexible `'a` waits.
At generalization it becomes part of the context, exactly like `Eq 'a`.
After monomorphization every predicate is ground, so nothing is ever
checked at runtime and no evidence is passed: `Evidence::Repr` is a marker.

No user impl of these traits is accepted (`ImplOfBuiltinTrait`).

## Datatype contexts

Every type constructor carries a **context**: the predicates its arguments
must satisfy for an application to be well-formed. Contexts are inferred
from the declaration body and never written by the user.

| Constructor | Context |
|---|---|
| `List 'a` | `Big 'a` |
| `Map 'k 'v` | `Big 'k, Big 'v` |
| `list 'a`, `array 'a` | `Storable 'a` |
| `pair 'a 'b` | `Storable 'a, Storable 'b` (only `mkPairData : Data -> Data -> pair Data Data` constructs one; `unConstrData` yields `pair int (list Data)`) |
| `type Box 'a = Box 'a` | `Big 'a` |
| `type Tag 'a = Tag Int` | none (phantom) |
| `type option 'a = None \| Some 'a` | none |
| `type wrap 'f 'a = Wrap ('f 'a)` | `Apply 'f 'a` |
| `type Box 'f 'a = Box ('f 'a)` | `Apply 'f 'a, Big ('f 'a)` (the field's representation is unknown until `'f` is) |
| tuples, `->` | none |

`Apply 'f t1 .. tn` is an **internal obligation**, not a trait: "the
application `'f t1 .. tn` is well-formed". It lives in the predicate store
beside trait predicates (`Predicate::Apply { head, args }`), is attached
to the descriptors of its head and arguments, is instantiated with the
scheme that carries it, is never user-writable and never displayed. It
reduces as soon as the head is known, to that head's context instantiated
at the arguments. A partial head `T u1 .. uk` supplies `u1 .. uk` first;
contexts of `T` that mention only supplied parameters are checked at the
partial application itself, the rest wait for the `Apply` that supplies
the remaining arguments. This is what lets `wrap` be declared once and
`wrap list (option int)` be rejected at the use: `Apply list (option int)`
reduces to `Storable (option int)`, which fails.

### Inferring a context

For one strongly connected group of unions and aliases:

1. Walk every constructor field and alias body. At each type application
   `T args` whose head is outside the group, instantiate `T`'s context at
   `args` and add the result; at a variable-headed application add `Apply
   'f args`; at a reference to a group member, record the reference and
   read no context yet (its context is still being computed). Add the
   positional requirement of the enclosing position: `Big` for a field of
   an uppercase union or record alias; nothing for a lowercase union field,
   tuple component, or function argument. A requirement on a
   variable-headed application is retained as is (`Big ('f 'a)`), not
   decided.
2. Resolve every predicate whose argument head is known. A failure is a
   declaration error (`BigField`, `AliasCasing`, `ListElement`, ...) reported
   at that field. A predicate on a declaration parameter is retained.
3. References to members of the same group use the group's current
   contexts. Iterate steps 1–2 until no context grows. Retained predicates
   have exactly two forms: a representation predicate on a bare parameter
   (a finite set), or a predicate containing a variable-headed application
   (`Apply 'f 'a`, `Big ('f 'b)`). Only the second form can grow under
   substitution. A parameter is **applied-relevant** if it occurs anywhere
   — head or argument — in a retained predicate of the second form.
   **Rule, checked at every round for every recursive reference:** the
   reference's substitution must map each applied-relevant parameter to a
   bare parameter; every other parameter may be mapped to anything
   (wrapped, closed, or variable-headed). A violation is
   `IrregularRecursion` at that reference. Since contexts and the relevant
   set only grow, a parameter that becomes relevant in a later round trips
   the rule then.

   Under the rule the second-form predicates are closed under substitution
   up to renaming, so the iteration reaches a fixpoint. A non-relevant
   parameter substituted by a fixed type can create a second-form
   predicate at most once per shape, after which its parameters are
   relevant and frozen. The implementation retains predicates in a
   deduplicated set and drives replay with a worklist keyed by (reference,
   predicate); it ends when the worklist is empty. There is no round cap:
   a reference that permutes many parameters legitimately produces many
   distinct predicates before repeating, and each is inserted once.
4. The retained predicates over the declaration's parameters are its
   context. Its kind is the Haskell 98 kind from the same walk.

The walk that infers the context is the same walk that reports declaration
errors: `type Pair2 'a = P (list 'a)` fails at `list 'a` because the field
must be `Big` and `list _` is `Const`, before any context is retained.

### Where contexts are enforced

Every place a type is *formed* generates its context:

- **Annotations.** Checking `f : Box 'a -> ...` instantiates `Box`'s context
  at `'a`: `Big 'a` joins `f`'s scheme context. `map : ('a -> 'b) -> 'f 'a
  -> 'f 'b` gets `Apply 'f 'a, Apply 'f 'b`.
- **Inference.** When the solver forms `list α` for a list literal, or
  `App1(T, args)` for a constructor use, it emits `T`'s context at the
  fresh variables. Predicates that survive generalization join the inferred
  scheme's context; the rest resolve when their heads become known.
- **Instantiation.** Instantiating a scheme instantiates its context, as
  for every predicate. `f (Some 1)` at `f : Big 'a => Box 'a -> unit` wants
  `Big (option int)`, which fails: "`Box` needs a Big argument, but `option
  int` is a Term type".
- **Impls.** `impl Functor list` specializes `map` to `('a -> 'b) -> list 'a
  -> list 'b`; the specialized scheme's context becomes `Storable 'a,
  Storable 'b`. Inside the impl body those are givens. At a use,
  `map f (xs : list int)` wants `Storable int` and `Storable b`; mapping to
  `option int` fails at the use with the chain `Storable (option int)` ⇐
  `list b` in `map` at `impl Functor list`.
- **Substitution.** When a variable head becomes known (`'f := list`), the
  pending `Apply 'f 'a` reduces to `Storable 'a`. Predicates live on the
  descriptors of their arguments (plans/03), so this is the existing
  re-examination path.

Predicates implied by the annotated type itself (a context entry `Big 'a`
where `Box 'a` appears in the type; every `Apply`) are not displayed in
rendered signatures. They are still stored and still instantiated.

### Contexts and `Data`

`Data`'s constructors (`Constr int (list Data)`, `Map (list (pair Data
Data))`, `List (list Data)`, `I int`, `B bytes`) are compiler-known and
exempt from the Big-field rule: they mirror `chooseData`.

## Traits and impls

- A trait parameter has a kind, inferred from the method signatures by
  unification. `Functor : Type -> Type`, `Eq : Type`, `Lift : Type -> Type`
  (two parameters, each `Type`).
- An impl head must have the parameter's kind. No representation
  requirement is placed on the head; representation requirements arise
  from the specialized method signatures, as above.
- Compiler-owned impls that depend on representation state it as a context:
  the reflexive `impl Big 'a => Lift 'a 'a`, structural `impl Big 'a => Eq
  'a`. These are the only impls with a representation predicate in their
  context that the user cannot write themselves; they are exempt from the
  Haskell 98 head-shape rule (see [traits.md](traits.md)).
- Coherence (impl overlap) is a unification question over heads and their
  kinds. Contexts do not participate: two impls with unifiable heads overlap
  regardless of `Big`/`Const` contexts, because contexts are not part of
  the impl's identity. Consequently core ships one `impl Eq 'a => Eq (list
  'a)` (elementwise), not a second `Big 'a => Eq (list 'a)`; the
  whole-list `equalsData` fast path is a codegen rewrite on ground types
  whose element is Big ([codegen.md](codegen.md), plans/08). Same for
  `Ord` and `Show`.

## Recursion and infinite kinds

- `type Tree 'a = Node 'a (List (Tree 'a))`: regular recursion, `Tree :
  Type -> Type`, context `Big 'a` (from the field `'a`; the recursive
  reference contributes the same context).
- `type Nest 'a = N (Nest (List 'a))`: nested datatype, accepted; `'a` is
  not applied-relevant, and `Big (List 'a)` resolves structurally each
  round.
- `type compose 'f 'g 'a = C ('f ('g 'a))` with `type r 'f 'g 'a = R ('f
  'a) (r (compose 'f 'g) 'g 'a)`: kinds unify, but `'f` is applied-relevant
  from the field `'f 'a` and the recursive reference maps it to `compose 'f
  'g` ⇒ `IrregularRecursion` at round 1. Without the rule the contexts
  would grow `Apply 'f ('g ('g .. 'a))` forever.
- `type self 'f = Self ('f 'f)`: `'f : k -> Type` and `'f : k` ⇒ occurs
  check ⇒ `KindInfinite`. Same as Haskell. `self tag` is not expressible.
- `type bad 'f = Bad (bad bad)`: `bad : k -> Type` applied to itself ⇒
  `KindInfinite`.
- `type r 'f 'a = R ('f 'a) (r 'f (list 'a))`: `IrregularRecursion`
  (`'a` is applied-relevant and is mapped to `list 'a`).

## Kinds and representation in codegen

Every `Core` type carries its representation. After monomorphization every
type is ground; `repr` of a ground type is computed from its head
(builtins, casing, alias bodies). Codegen uses it to choose:

- how a constructor allocates (`Constr` data vs `constr` term),
- how a `case` scrutinizes (`unConstrData` + tag vs UPLC `case`),
- the UPLC `Type` of `list`/`array`/`pair` element constants (`Big`
  elements are `Type::Data`),
- which `Lift` implementation applies,
- whether `if` may use `ifThenElse` (the condition must be `bool`).

Representation predicates produce no runtime evidence.

## Errors

| Error | When | Reported at |
|---|---|---|
| `KindMismatch` | arity mismatch: a `Type` applied, or `k1 -> k2` where `Type` is needed | the application |
| `KindInfinite` | occurs check in kind unification | the declaration |
| `IrregularRecursion` | a recursive substitution constructs an applied-relevant parameter instead of renaming it | the reference |
| `RepresentationMismatch` with its field/body context | a representation predicate fails while checking a declaration | the field / body |
| `MissingImpl` with a representation trait | a representation predicate fails at a use or annotation | the use, with the provenance chain |
| `ContradictoryRepresentation` | one variable's repr predicates admit no representation | the annotation or inferred definition |
| `ImplOfBuiltinTrait` | a user impl of `Big`/`Const`/`Term`/`Storable`/`Little` | the impl |
| `KindAnnotationArrow` | `('f : Big -> Big)` (no kind syntax exists) | the annotation |
| `MissingConstraint` | a body requires `Storable 'a` and the annotation does not promise it | the body use, pointing at the annotation |

The last two are the ordinary predicate errors of [traits.md](traits.md)
with representation-specific prose. Intended rendering:

```
-- KIND MISMATCH ------------------------------------------------ Main.nash

The 1st argument to `list` is not something a builtin list can hold:

12|     seen : list (option int)
                      ^^^^^^^^^^
`list` needs a Big or Const element type, but `option int` is a Term type
(a little ADT is a UPLC `constr` term).

Hint: use the Big twin `List (Option Int)` if the values cross the data
boundary anyway, or keep a little record of the fields you need.
```

```
-- BIG TYPE WITH LITTLE FIELD ------------------------------------ Main.nash

`Datum` is a Big type, so every field must be Big:

4| type Datum = Datum { owner : bytes, deadline : Int }
                                ^^^^^
`bytes` is a Const type (a UPLC builtin bytestring).

Hint: use `Bytes`, or name the type `datum` to make it a little type whose
fields can have any representation.
```

```
-- LOWERCASE ALIAS OF A BIG TYPE ---------------------------------- Main.nash

`id` starts with a lowercase letter, so it must be a little type:

7| type alias id = Int
                   ^^^
`Int` is Big.

Hint: rename the alias to `Id`.
```

```
-- MISSING REPRESENTATION ----------------------------------------- Main.nash

`cons` needs its element to be Big or Const:

9|     cons (Some x) rest
            ^^^^^^^^
`cons` is used at `option int`, but `option int` is a Term type.

`cons : Storable 'a => 'a -> list 'a -> list 'a`
```

```
-- INFINITE KIND ------------------------------------------------- Main.nash

`self` applies its parameter `'f` to itself:

3| type self 'f = Self ('f 'f)
                        ^^^^^
`'f` would need the kind `k -> Type` and the kind `k` at the same time.
```

```
-- TYPE APPLIED TO TOO MANY ARGUMENTS ------------------------------ Main.nash

`int` is a base type and takes no arguments:

9| x : int Int
       ^^^^^^^
```

`BadArity` (too many arguments to a named constructor) is still Elm's
canonicalization error. Fewer arguments than the arity leave a
higher-kinded type, which must fit the enclosing position by kind.

## Interactions

- **Canonicalization** (`nash-can`): kind inference and context inference
  run over the type-declaration SCCs before value canonicalization.
  Declarations store `kind` and `context`; interfaces export both; they are
  part of the incremental-build fingerprint.
- **Traits** (plans/03): representation predicates, `Apply`, and datatype
  contexts are predicates in the existing store; structural resolution is a
  third resolver branch beside `by_instance` and `by_given`, like
  `StructuralEq`.
- **Representation** ([representation.md](representation.md)): casing decides
  the encoding of records and ADTs; `repr` is the single lookup.
- **Data** ([data.md](data.md)): `Data`'s constructors are exempt from the
  Big-field rule.

## Why not subkinding

The previous design put representation inside kinds (`list : Storable ->
Const`, `option : Any -> Term`) and made application `kind(arg) <= domain`.
Bounds plus contravariant arrows plus polymorphism is a subtype-constraint
problem: cyclic constraint graphs, finite-satisfiability proofs, relational
kind schemes with private witnesses, and an annotation-entailment problem
with no total procedure. All of it served one guarantee, "a `list` holds
constants", which is a property of a head constructor and is expressed
exactly by a predicate. Moving representation out of the kind restores
Haskell 98 kinds (unification, occurs check, nothing to prove) and reuses
the predicate machinery Nash already needs for traits.
