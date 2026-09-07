# Kinds

Nash kinds describe the runtime representation of a type. Every well-formed
type has a kind, and the base kind of a ground type tells codegen exactly
which UPLC shape a value of that type has. This document specifies the kind
language, the casing rule, kind inference, kind annotations, the errors,
and how kinds flow into codegen. [overview.md](overview.md) is authoritative
where the two disagree.

## Purpose

UPLC has three distinct value shapes that cannot be mixed:

| Base kind | Runtime value | Examples |
|---|---|---|
| `Big` | a Plutus `Data` constant | `Int`, `Bytes`, `List 'a`, `Map 'k 'v`, `Data`, user `type Foo`, `type alias Foo` |
| `Const` | a UPLC builtin constant | `int`, `bytes`, `string`, `bool`, `unit`, `list 'a`, `pair 'a 'b`, `array 'a`, `bls_g1`, `bls_g2`, `bls_mlr`, `value` |
| `Term` | a UPLC `constr` term, or a lambda | user `type option 'a`, tuples, little records, `'a -> 'b` |

A builtin such as `mkCons : 'a -> list 'a -> list 'a` only works when the
element is something a builtin list can hold. A `Data` constructor can only
hold `Data`. The kind system makes these facts static so that no value ever
has to be inspected at runtime to pick a representation.

## Kind language

A type occurrence can carry a bound: `('a : Big)`, `('a : Const)`, or
`('a : Storable)`. Parenthesized annotations apply to the enclosed type,
including within recursive impl patterns: `list (pair ('a : Big) 'b)`.
Occurrences of the same variable share one kind, so contradictory annotations
are errors. Bounds survive aliases, method substitution and interfaces. They
constrain types without changing their identity or runtime representation.

```ebnf
kind       = kind_atom [ '->' kind ] ;
kind_atom  = 'Big' | 'Const' | 'Term' | 'Storable' | '(' kind ')' ;
```

Internally a kind is:

```
Kind ::= Big | Const | Term          -- base kinds
       | Kind -> Kind                -- kind arrow (right associative)
       | k                           -- kind variable, with a bound
       | Constructor(scheme, args)   -- quantified constructor with captured arguments
```

A kind variable carries a *bound*, which is the set of shapes it may take.
Bounds are subsets of `{ Big, Const, Term, Arrow }`:

| Bound | Members | Used for |
|---|---|---|
| `Any` | `Big`, `Const`, `Term` | fields of little ADTs, tuple components, function arguments and results |
| `Storable` | `Big`, `Const` | elements of `list`, `array`, and components of `pair` |
| `Little` | `Const`, `Term` | the body of a lowercase alias |
| `All` | `Big`, `Const`, `Term`, `Arrow` | unannotated declaration parameters (may be higher-kinded) |

`Storable` is the only bound with surface syntax. Every `Big` value is also a
`Data` constant at runtime, which is why `Storable` includes `Big`: `list Int`
and `list (List Int)` are legal, `list (option int)` is not.

Bounds intersect when two variables unify; an empty intersection is a kind
error. Binding a variable to a base kind requires that base kind to be in the
bound; binding it to an arrow requires `Arrow` in the bound.

## Kinds of the builtin constructors

| Constructor | Kind |
|---|---|
| `Int`, `Bytes`, `Data` | `Big` |
| `List` | `Big -> Big` |
| `Map` | `Big -> Big -> Big` |
| `int`, `bytes`, `string`, `bool`, `unit`, `bls_g1`, `bls_g2`, `bls_mlr`, `value` | `Const` |
| `list`, `array` | `Storable -> Const` |
| `pair` | `Storable -> Storable -> Const` (values from `unConstrData`, `unMapData`; only `mkPairData : Data -> Data -> pair Data Data` constructs one, so the API, not the kind, restricts construction) |
| `->` | `Any -> Any -> Term` |
| tuple `( , )`, `( , , )` | `Any -> Any -> Term`, `Any -> Any -> Any -> Term` |
| `()` (the unit type) | `Const` (it is `unit`) |

These are seeded by the compiler into the `Builtin` module of the `nash/core`
package. See [representation.md](representation.md) for their layouts.

## The casing rule

The first character of a type name fixes the shape class of its result kind:

- Uppercase name: the result kind is `Big`.
- Lowercase name: the result kind is `Const` for a compiler builtin, and
  `Term` for a user little ADT. A lowercase alias must have a `Little` body
  (`Const` or `Term`).

The rule is a *constraint added during inference*, not a lookup. It applies
to unions, aliases and record aliases:

```elm
type Datum = Datum { owner : Bytes, deadline : Int }   -- Big, fields must be Big
type step 'a = Done 'a | Next int 'a                    -- Term, fields any kind
type alias Vault = { owner : Bytes, amount : Int }      -- Big record, fields Big
type alias acc = { total : int, seen : list Int }       -- little record, fields any
type alias Id = Int                                     -- ok: Big body
type alias id = Int                                     -- error: lowercase alias, Big body
type alias count = int                                  -- ok: Const body
```

Type variables are always written `'a`, so a bare lowercase name in type
position is never ambiguous with a variable.

## Declarations and their kinds

### Big ADTs

`type Foo 'a ... = C1 t11 ... | C2 ...` with an uppercase `Foo`:

- Result kind `Big`.
- Every constructor field type must have kind `Big`. Fields of kind `Const`
  or `Term` are errors, because a `Data.Constr` can only hold `Data`.
- Parameters get fresh kind variables with bound `All`; the field constraints
  refine them. A phantom parameter stays a kind variable and is generalized.

```elm
type Box 'a = Box 'a          -- Box : Big -> Big
type Tag 'a = Tag Int         -- Tag : forall k. k -> Big
type Pair2 'a = P (list 'a)   -- error: field `list 'a` is Const, `Pair2` is Big
```

A constructor may label its fields, `type Datum = Datum { owner : Bytes, deadline : Int }`.
Labels change nothing for kinds: the fields are the constructor's fields
and follow the enclosing type's rule (Big here, so both must be Big).
Labeled fields are not an anonymous record type.

### Little ADTs

`type foo 'a ... = ...` with a lowercase `foo`:

- Result kind `Term`.
- Fields may have any base kind (`Any`), including functions and tuples.

```elm
type option 'a = None | Some 'a          -- option : Any -> Term
type thunk 'a = Thunk (unit -> 'a)       -- thunk : Any -> Term
type wrap 'f 'a = Wrap ('f 'a)           -- requires applying f to a to produce a value
```

### Aliases

The kind of an alias is the kind of its body, then the casing rule applies
to the result:

- `type alias Foo ... = body`: result unifies with `Big`. If the body is a
  record type, it is a *Big record* and every field must be `Big`.
- `type alias foo ... = body`: result unifies with a fresh `Little` variable.
  If the body is a record type, it is a *little record* with kind `Term` and
  fields of any kind.

An alias is expanded transparently by the type checker, so its kind is only
ever a description of its body. Record aliases are nominal (see
[representation.md](representation.md)) but their kinds follow the same rule.

### Tuples, functions, unit

`( 'a, 'b )` and `( 'a, 'b, 'c )` have kind `Term` with `Any` components.
`'a -> 'b` has kind `Term` with `Any` argument and result. `()` in a type is
the Const type `unit`.

### Traits

`trait Functor 'f where map : ('a -> 'b) -> 'f 'a -> 'f 'b` gives `'f` a
fresh `All` variable. Inference requires an arrow-capable constructor and
retains separate application obligations for `'f 'a` and `'f 'b`. Both
arguments and results must be values. Each impl supplies a constructor scheme;
each application instantiates that scheme independently:

| Impl head | Instantiation |
|---|---|
| `impl Functor List` | Each argument must be Big; each result is Big. |
| `impl Functor list` | Each argument must be Storable; each result is Const. |
| `impl Functor option` | Each argument may have any base kind; each result is Term. |

Method signatures of an impl are then kind-checked with the instantiated
kinds. Kind variables in trait schemes have no surface syntax; they are
always inferred. See [traits.md](traits.md).

Internally, both declaration kind schemes and value kind schemes retain
`Apply(head, argument, result)` obligations in their shared binder. A known
constructor retains its own quantified scheme. Partial application retains
that scheme and the supplied argument kinds: later applications instantiate
the scheme and replay those arguments before checking the new argument.
This preserves dependent results such as `forall k:Little. k -> k` and
relationships between parameter positions without equating separate uses.

## Kind inference

Inference is Haskell 98 style, run in `nash-can` after type declarations are
canonicalized and before value declarations are canonicalized:

1. Build the dependency graph of the module's unions and aliases (an edge for
   every `Type::Named` / `Type::Alias` reference to a local declaration) and
   compute strongly connected components in dependency order. Union and
   alias declarations share one graph because `type Tree = Node (List Branch)` with
   `type alias Branch = Tree` is legal. Both the union field and alias
   body have kind `Big`.
2. For each SCC, give every declaration a monomorphic kind
   `p1 -> ... -> pn -> r`: each parameter `pi` is a fresh `All` variable (or
   the user annotation), and `r` is `Big` or `Term` for unions, or a fresh
   `All` variable for aliases.
3. Walk every constructor field type and alias body, unifying as described
   above. References to declarations in the same SCC use the monomorphic
   kind; references to earlier SCCs and imported declarations instantiate
   their kind scheme with fresh variables.
4. After the SCC is solved, apply the casing constraints, zonk each
   declaration's kind, and generalize the remaining variables into a
   `KindScheme`. Nothing is defaulted: an unconstrained variable stays
   polymorphic, which is what lets `type option 'a` accept any kind and lets
   phantom parameters carry any kind.

Value annotations are checked afterwards with the same walker. The compiler
retains the original annotation before splitting argument/result types or
expanding function aliases, so explicit alias parameter bounds remain
visible to this check. For each annotation, every free
type variable of the annotation gets a fresh `All` variable shared by all
its occurrences, every `Type::Named` application is checked against the
constructor's scheme, and the walk yields the kind of every free variable.
**Kind bounds on type variables are predicates.** A free variable's kind
(`'a : Storable` in `cons : 'a -> list 'a -> list 'a`) is stored on the
value's type scheme as a kind predicate, next to trait predicates, and the
qualified-type machinery of [traits.md](traits.md) (plans/03) discharges it
at every instantiation. All free-variable kinds share one binder, but an
abstract constructor's accepted argument kinds are not equated with the actual
kind of each argument. For `'f 'a` and `'f 'b`, check each application against
the bounds of `'f` independently. Thus mapping `option unit` to
`option (unit, unit)` is legal: Const and Term both fit option's element bound.
The corresponding builtin-list mapping is illegal because Term does not fit
Storable. Repeated occurrences of the same type variable still share its kind.
Generalize and instantiate the entire group together, preserving free-variable
order and the application constraints, including across module interfaces.
Multiple bounds intersect; checking a rigid annotation proves a requirement
without narrowing the annotation's promised kind. Plans/02 infers and reports the kinds; plans/03
enforces them at use sites. Without that step plain HM unification would
instantiate `'a` at `option int` with nothing to reject it, because
declaration-level inference never sees instantiations.

Inferred (unannotated) values carry no kind information in the front end.
Their kind predicates arrive from the instantiations they contain, again via
the predicate mechanism.

### Kind annotations

A declaration parameter may be annotated:

```ebnf
type_param = type_var | '(' type_var ':' kind ')' ;
```

```elm
type Fix ('f : Big -> Big) = Fix ('f (Fix 'f))
trait Foldable ('t : Storable -> Const) where ...
```

The annotation is unified with the inferred kind, not substituted for it, so
a wrong annotation is reported as a mismatch between the two. `Storable` in
an annotation stands for a fresh variable bounded `Storable`; each
occurrence is a separate variable. There is no syntax for naming a kind
variable.

### Partial application

Kinds allow a constructor to be applied to fewer arguments than its arity
(`Functor List`, `wrap List Int`). Kind checking accepts such applications;
where the surface grammar and the type solver accept them is decided in
[traits.md](traits.md). Applying a base-kinded type to an argument
is invalid. Named constructors and nominal aliases may be partial in type
annotations when the enclosing parameter accepts their arrow kind. A
constructor in a value-type position is rejected by kind checking. Supplying
more arguments than a named constructor declares reports `BadArity` before
kind inference runs.

## Errors

Error data is stored, like every other Nash error, and rendered by
`nash-report`. The intended prose:

**Kind mismatch on a type argument**

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

**Big field that is not Big**

```
-- BIG TYPE WITH LITTLE FIELD ------------------------------------ Main.nash

`Datum` is a Big type, so every field must be Big:

4| type Datum = Datum { owner : bytes, deadline : Int }
                                ^^^^^
`bytes` is a Const type (a UPLC builtin bytestring).

Hint: use `Bytes`, or name the type `datum` to make it a little type whose
fields can have any kind.
```

**Alias casing**

```
-- LOWERCASE ALIAS OF A BIG TYPE ---------------------------------- Main.nash

`id` starts with a lowercase letter, so it must be a little type:

7| type alias id = Int
                   ^^^
`Int` is Big.

Hint: rename the alias to `Id`.
```

**Annotation mismatch**

```
-- KIND ANNOTATION MISMATCH --------------------------------------- Main.nash

The annotation says `'f` has kind `Big -> Big`:

3| type Fix ('f : Big -> Big) = Fix ('f (Fix 'f))
              ^^^^^^^^^^^^^^
but the way `'f` is used gives it kind `Term -> Big`.
```

**Infinite kind**

```
-- INFINITE KIND ------------------------------------------------- Main.nash

I cannot find a finite kind for `bad` in `type bad 'f = Bad (bad bad)`:
the recursive constructor `bad` is applied to itself.
```

**Too many arguments**

```
-- TYPE APPLIED TO TOO MANY ARGUMENTS ------------------------------ Main.nash

`int` is a base type and takes no arguments:

9| x : int Int
       ^^^^^^^
```

Too many arguments to a named constructor still produce Elm's `BadArity`
error from `nash-can`. Fewer arguments retain an arrow kind, which must fit
the enclosing type position.

Canonical variable applications retain a general `App` head and argument
list. Plan 03 substitution normalizes a head that becomes known into a
named or nominal alias application, preserving every argument. Partial
aliases retain their remaining bound parameters in retained interfaces.
Kind checking and higher-kinded value inference support these applications,
including partial aliases and imported annotations. Named constructors in
ordinary type annotations and impl heads may be partial and are checked by kind.
A base-bounded variable applied to arguments instead
reaches the kind-specific too-many-arguments error.

## Kinds in codegen

Every `Core` type carries its base kind (`Big`, `Const`, `Term`). After
monomorphization every type is ground, so kind variables never reach `Core`;
the kind of a ground type is computed by instantiating the head
constructor's scheme and reading the result. Codegen uses it to choose:

- how a constructor allocates (`Constr` data vs `constr` term),
- how a `case` scrutinizes (`unConstrData` + tag vs UPLC `case`),
- the UPLC `Type` of `list`/`array`/`pair` element constants (`Big` elements
  are `Type::Data`),
- which `Lift` builtin implementation applies,
- whether `if` can use `ifThenElse` (the condition must be `bool`, kind
  `Const`).

The same scheme lookup answers `validateData` and `@derive(FromData)` about
field shapes. See [representation.md](representation.md) and
[codegen.md](codegen.md).

## Interactions

- **Canonicalization**: kind inference is a pass in `nash-can` between type
  declaration canonicalization and value canonicalization; declarations
  store their `KindScheme`, interfaces export it.
- **Traits**: kind schemes for traits, kind predicates on value schemes and
  the `Storable`/`Big` predicates used by `Lift` impls all live in the
  qualified-type machinery.
- **Representation**: casing decides the encoding of records and ADTs.
- **Data**: `Data`'s constructors are a compiler-known Big type whose
  constructor fields (`int`, `list Data`, ...) are exempt from the Big-field
  rule because they map directly onto `chooseData` results.
- **Interfaces**: an exported type's kind scheme is part of the interface
  and of the incremental-build fingerprint.
