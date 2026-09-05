# Representation

This document fixes the runtime layout of every Nash type on the Plutus CEK
machine, and the explicit bridges between layouts. [kinds.md](kinds.md)
explains which layout a type gets; this document says what the layout is.
[overview.md](overview.md) is authoritative where the two disagree.

Runtime types below refer to `crates/nash-plutus/src/constant.rs`
(`Constant`) and `crates/nash-plutus/src/typ.rs` (`Type`).

## Const types

A `Const` value is a UPLC constant. Each Nash Const type maps to one
`Constant` variant and one UPLC `Type`:

| Nash type | `Constant` variant | UPLC `Type` | Notes |
|---|---|---|---|
| `int` | `Integer(&BigInt)` | `Integer` | arbitrary precision |
| `bytes` | `ByteString(&[u8])` | `ByteString` | |
| `string` | `String(&str)` | `String` | UTF-8, only for `trace` and tests |
| `bool` | `Boolean(bool)` | `Bool` | the only type `if` accepts |
| `unit` | `Unit` | `Unit` | written `()` in types and values |
| `list 'a` | `ProtoList(&Type, &[&Constant])` | `List(elem)` | `'a : Storable` |
| `pair 'a 'b` | `ProtoPair(&Type, &Type, &Constant, &Constant)` | `Pair(a, b)` | `'a 'b : Storable`; `mkPairData` constructs `pair Data Data`, while `unConstrData` returns `pair int (list Data)` |
| `array 'a` | `ProtoArray(&Type, &[&Constant])` | `Array(elem)` | `'a : Storable` |
| `bls_g1` | `Bls12_381G1Element` | `Bls12_381G1Element` | |
| `bls_g2` | `Bls12_381G2Element` | `Bls12_381G2Element` | |
| `bls_mlr` | `Bls12_381MlResult` | `Bls12_381MlResult` | |
| `value` | `Value(&LedgerValue)` | `Value` | ledger `Value` builtin |

The element `Type` of `list`, `array` and `pair` is the *erasure* of the
element's Nash type: a `Const` element erases to its own UPLC `Type`; a
`Big` element erases to `Type::Data`. So `list int` is `List(Integer)`,
`list Int` and `list Data` and `list (List Int)` are all `List(Data)`, and
`list (list bytes)` is `List(List(ByteString))`.

`bool` and `unit` are Const types with constructors: `True`/`False` and
`()` are the constants `(con bool True)`, `(con bool False)`, `(con unit ())`.
A `case` on `bool` lowers to `ifThenElse`; a `case` on `unit` has one branch.

## Big types

A `Big` value is a `Data` constant, `Constant::Data(&PlutusData)`, UPLC
`Type::Data`. Its Nash type says which `PlutusData` shape it has:

| Nash type | `PlutusData` shape |
|---|---|
| `Data` | any shape; the universal Big type |
| `Int` | `I n` |
| `Bytes` | `B bs` |
| `List 'a` | `List [x0, x1, ...]`, each `xi : 'a` |
| `Map 'k 'v` | `Map [(k0, v0), ...]` |
| user `type Foo = C0 ... \| C1 ... \| ...` | `Constr i [fields...]` where `i` is the declaration index of the constructor and fields are in declaration order |
| user `type alias Foo = { f0 : ..., f1 : ... }` | `List [f0, f1, ...]` in field declaration order |
| a constructor with labeled fields `type Foo = Foo { a : ..., b : ... }` | `Constr i [a, b]`: flat, exactly as positional fields; labels exist only at compile time |

Alias records are `List` rather than `Constr 0` because records are
aliases, not nominal constructors; use a constructor with labeled fields
when the `Constr` encoding is required.

Constructor and field order are the *declaration* order, which the
canonical AST already records (`Ctor.index`, `FieldType.index` in
`crates/nash-ast/src/lib.rs`). Canonical records sort fields by name for
lookup, so codegen must sort by `index` before emitting.

`Data` is a Big type with pattern-matchable constructors
`Constr int (list Data) | Map (list (pair Data Data)) | List (list Data) | I int | B bytes`.
Its fields are exactly what `chooseData`, `unConstrData`, `unMapData`,
`unListData`, `unIData`, `unBData` return, so matching on `Data` costs one
`chooseData` plus one unwrap and no conversion. `Data` is exempt from the
Big-field rule. See [data.md](data.md).

A Big ADT with no fields anywhere (`type Redeemer = Claim | Cancel`) is
still `Constr i []`; there is no integer-tag optimization at the Data level
because on-chain consumers expect `Constr`.

## Term types

A `Term` value is a UPLC term that is not a constant: a `constr` term or a
lambda.

| Nash type | UPLC term |
|---|---|
| user `type foo = C0 ... \| C1 ... \| ...` | `constr i [fields...]`, `i` the constructor's declaration index |
| a little constructor with labeled fields `type foo = Foo { a : ..., b : ... }` | `constr i [a, b]`, flat |
| tuples `( a, b )`, `( a, b, c )` | `constr 0 [a, b]`, `constr 0 [a, b, c]` |
| `type alias foo = { f0 : ..., f1 : ... }` | `constr 0 [f0, f1, ...]` in field declaration order |
| `'a -> 'b` | `lam` |

Fields of a `constr` term may be constants, `Data`, other `constr` terms, or
lambdas: little types hold values of any kind. `case` on a `constr` term is
the UPLC `case` with one branch lambda per constructor; matching a tuple or a
little record is a one-branch `case`.

Little ADTs with no fields anywhere still use `constr i []`; the optimizer
may not turn them into integers because `case` needs a `constr` scrutinee.

## Records and labeled fields

Two declaration forms give named fields, with different encodings:

| Form | Big encoding | little encoding | `.field` access |
|---|---|---|---|
| `type alias Foo = { a : .., b : .. }` | `List [a, b]` | `constr 0 [a, b]` | always |
| `type Foo = Foo { a : .., b : .. }` | `Constr 0 [a, b]` | `constr 0 [a, b]` | only when the type has a single constructor |

Anonymous record types do not exist otherwise. `x.a` on a labeled
multi-constructor type is a type error; use `case`. Record update
`{ x | a = e }` is for alias records only; on a labeled constructor,
rebuild the value with the constructor. A labeled constructor is
built positionally (`Datum owner deadline`) or by label
(`Datum { owner = o, deadline = d }`); the labeled form is sugar that the
compiler rewrites to the positional call in wire order, and its field set
must match the constructor's labels exactly. The pattern
`Datum { owner, deadline }` is the same sugar for matching: it rewrites to
the positional constructor pattern, the named fields must be a subset of
the labels, and labels not mentioned become `_`. The alias form has both
the record literal `{ a = .., b = .. }` (resolved to the alias by its field
set) and the constructor function `Foo a b`.

Labels are part of a constructor, so they follow Elm's encapsulation:
a module that imports `Datum` without `(..)` sees neither its constructors
nor its labels, and `.owner` on it is a type error there. Export
`Datum(..)` to share the fields.

Field access costs the same in both forms: a Big field is one
`unListData` or `unConstrData` + `sndPair` followed by a `headList`/`tailList`
chain; a little field is one `case` with a selecting lambda.

## Prelude twins

The prelude defines each control-flow type twice, once per side of the
boundary. The little twin is the one used in ordinary code; the Big twin is
the on-chain encoding.

| little (kind) | Big | Big encoding |
|---|---|---|
| `bool` (Const) `False \| True` | `type Bool = False \| True` | `Constr 0 []`, `Constr 1 []` |
| `unit` (Const) `()` | `type Unit = Unit` | `Constr 0 []` |
| `type option 'a = Some 'a \| None` (Term) | `type Option 'a = Some 'a \| None` | `Constr 0 [x]`, `Constr 1 []` |
| `type result 'e 'a = Ok 'a \| Err 'e` (Term) | `type Result 'e 'a = Ok 'a \| Err 'e` | `Constr 0 [x]`, `Constr 1 [e]` |
| `type ordering = LT \| EQ \| GT` (Term) | `type Ordering = LT \| EQ \| GT` | `Constr 0 []` .. `Constr 2 []` |

Constructor order matches the PlutusTx and Aiken encodings (`Some`/`Just`
before `None`/`Nothing`, `False` before `True`).

Constructor names are shared between the twins, so the prelude resolves
them by qualification: the little constructors are in scope unqualified
(`True`, `False`, `()`, `Some`, `None`, `Ok`, `Err`, `LT`, `EQ`, `GT`), and
the Big constructors are always qualified by their type's module
(`Bool.True`, `Unit.Unit`, `Option.Some`, `Result.Ok`, `Ordering.LT`). The
Big type names themselves (`Bool`, `Option`, ...) are in scope unqualified.
Patterns follow the same rule: `case b of Bool.True -> ...`.

`if c then a else b` requires `c : bool`. Branch on a `Bool` with `case`, or
`lower` it first.

## Case lowering

- `case` on a **Big ADT**: `unConstrData d` gives `pair int (list Data)`;
  `fstPair` is the tag; branch selection is an `equalsInteger` chain over
  the tag in declaration order (see [codegen.md](codegen.md)); fields come from `sndPair` by `headList`/`tailList`
  chains, memoized per branch. Big record fields: `unListData` then the same
  chains. See [codegen.md](codegen.md) for accessor memoization.
- `case` on a **little ADT / tuple / little record**: UPLC `case` on the
  `constr` term; each branch is a lambda over the fields.
- `case` on **`bool`**: `ifThenElse`. On **`unit`**: the single branch.
- `case` on **`Data`**: `chooseData` with five delayed branches.
- `case` on **`list 'a`**: `chooseList` for `[]` vs `x :: xs`, then
  `headList`/`tailList`.
- `case` on **`List 'a`** (Big): `unListData` then as `list`.

## Bridging Big and little

There is no implicit coercion. Two mechanisms cross the boundary.

### `Lift 'small 'big`

```elm
trait Lift 'small 'big where
    lift  : 'small -> 'big
    lower : 'big -> 'small
```

A multi-parameter trait relating a little representation to its Big twin.
`lift` always succeeds. `lower` traps (evaluation error) when the `Data`
does not have the expected shape, because the builtin unwrappers trap.

Builtin impls, with their UPLC:

| Impl | `lift` | `lower` |
|---|---|---|
| `Lift int Int` | `iData` | `unIData` |
| `Lift bytes Bytes` | `bData` | `unBData` |
| `Lift bool Bool` | `ifThenElse c (Constr 1 []) (Constr 0 [])` | tag compare |
| `Lift unit Unit` | `Constr 0 []` | `()` |
| `Lift (list 'a) (List 'b)` given `Lift 'a 'b` | map `lift` over the elements, then `listData` | `unListData` then map `lower` |
| `Lift (list (pair 'k 'v)) (Map 'k 'v)` with `'k 'v : Big` | `mapData` | `unMapData` |
| `Lift 'a 'a` for every `'a : Big` (compiler built-in) | identity | identity |
| `Lift (option 'a) (Option 'b)` given `Lift 'a 'b` | `case`, rebuild | `unConstrData`, rebuild |
| `Lift (result 'e 'a) (Result 'f 'b)` | as `option` | as `option` |
| `Lift ordering Ordering` | rebuild | rebuild |

Rules:

- The reflexive impl `impl Big 'a => Lift 'a 'a` is provided by the
  compiler, not written in Nash: its head is a bare type variable, which
  the Haskell 98 head rules for user impls reject (see
  [traits.md](traits.md)). It is restricted to Big types by the kind
  predicate `Big 'a` (see [kinds.md](kinds.md)) and is what makes
  `lift : list Int -> List Int` a single `listData`, because mapping the
  identity is removed by the optimizer.
- `Lift (list 'a) (List 'b)` does not overlap the reflexive impl because
  `list 'a` is never Big.
- Users write `Lift` impls for their own pairs of twins, or derive them
  with `@derive(Lift Foo)` on the little type, naming the Big twin. Deriving
  requires the same constructor names and arities; each field pair must
  itself have a `Lift` impl. Orphan rules apply as for any trait.
- `lift`/`lower` on tuples and function types do not exist: there is no Big
  tuple and no Big function.

### `ToData` and `FromData`

Defined only for Big types:

```elm
trait ToData 'a where
    toData : 'a -> Data

trait FromData 'a where
    fromData     : Data -> 'a
    validateData : Data -> 'a
```

- `toData` is the identity at runtime: a Big value already is a `Data`
  constant. It exists to forget the static shape.
- `fromData` is *shallow*: it checks only that the outer shape matches
  (`Constr` with a tag in range and the right field count for an ADT, `List`
  for a Big record, `I`/`B` for `Int`/`Bytes`) and then reinterprets. Field
  contents are not inspected. It traps on a mismatch.
- `validateData` is *full*: it recursively checks every field against the
  type's shape and traps on the first mismatch, then reinterprets like
  `fromData`. Use it once at the validator boundary when the datum comes
  from an untrusted source; use `fromData` everywhere else. The
  non-failing path is the `Data.Decode` combinators (see
  [data.md](data.md)), which return `option`/`result` instead of trapping.

`@derive(ToData, FromData)` generates these for user Big types; the
builtins have compiler impls. `Data` itself has trivial impls.

## Costs

Rough CEK costs, to guide the choice of representation:

| Operation | Big | little |
|---|---|---|
| construct, n fields | `Constr` constant: O(1) if all fields are constants, else `constrData` on a built list, O(n) | `constr`: O(1) |
| match, k constructors | `unConstrData` + `fstPair` + up to k tag compares | one `case` |
| field i | `sndPair` + i `tailList` + `headList`, plus one `un*Data` if the field is used as a little value | one `case` with a lambda that selects the field |
| `lift`/`lower` of `int`/`bytes` | one builtin call each way | |
| `lift`/`lower` of a list | O(n) map unless the element impl is reflexive | |
| `validateData` | O(size of the Data) | |

Consequences:

- Keep computation in little types; convert at the boundary.
- A validator that only reads two fields of a large datum should use Big
  field access (two chains) rather than `lower` the whole datum.
- `List Int` iterated with `unListData` once and then `list Data` builtins
  costs one unwrap per element access, versus a full `lower` up front. The
  optimizer memoizes accessors but does not change representation.

## Interactions

- **Kinds** decide the table row for every type; ground kinds reach codegen
  on every `Core` type.
- **Records** are nominal, so a record literal always has a known alias and
  hence a known encoding at canonicalization time
  (plans/04-representation.md).
- **Literals** default to little (`int`, `string`, `bytes`) via `FromInt`
  and friends; a `1 : Int` literal is `FromInt Int`, which is `iData 1`
  folded to a constant at compile time.
- **Comptime** results must be a `Const` or `Big` value, because `Term`
  values (lambdas, `constr`) have no constant form in the flat encoding.
- **Tests** and **traces** use `string`; on-chain code should use `bytes`.

## Open questions

- Whether `fromData` should verify field *count* for ADTs, or only the tag.
  This document says both, since both are O(1) for `Constr` data.
