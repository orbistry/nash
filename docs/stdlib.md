# Standard library: `nash/base`

`nash/base` is the compiler-owned foundation library. Its Nash sources live
in `crates/nash-driver/base/src/` and are embedded in `nash-driver` at build
time. Applications neither declare a Base dependency nor download it. A new
Base version ships with a new compiler version; there is no independently
versioned Base package or separate `nash-base` crate.

The driver reads these sources through stable `nash-base:///Module.nash`
URIs. Compilation and diagnostics need no installed source directory and do
not depend on the compiler checkout. Bundled sources are read-only. The
package identity `nash/base` and its module names are reserved.

Decisions here follow [overview.md](overview.md), the trait hierarchy and
module split in [traits.md](traits.md), the layouts in
[representation.md](representation.md), the builtin constructors in
[kinds.md](kinds.md), and the generator and test statements in
[testing.md](testing.md).

## Layout

The layout includes planned library modules as well as the current foundation.
`Primitive` and `Builtin` are synthetic modules with no Nash source file.

```
crates/nash-driver/base/
  src/
    Prelude.nash          infix declarations, basics
    Eq.nash               trait Eq + impls for compiler-known types
    Ord.nash              trait Ord
    Show.nash             trait Show
    Num.nash              trait Num
    Integral.nash         trait Integral
    Semigroup.nash        trait Semigroup
    Monoid.nash           trait Monoid
    Functor.nash          trait Functor
    Applicative.nash      trait Applicative
    Monad.nash            trait Monad
    Lift.nash             trait Lift + impls for compiler-known types
    Data.nash             traits ToData/FromData/Validate; functions over Data
    Literal.nash          traits FromInt, FromString, FromBytes, FromBool, FromUnit
    Bool.nash             bool functions; Big Bool
    Unit.nash             Big Unit
    Option.nash           option / Option
    Result.nash           result / Result
    Ordering.nash         ordering / Ordering
    Int.nash              int functions; Big Int
    Bytes.nash            bytes functions; Big Bytes
    String.nash           string (little only; no Big twin)
    List.nash             list functions; Big List
    Cons.nash             cons: Term linked list (elements of any representation)
    Pair.nash             pair
    Array.nash            array
    Map.nash              Map (Big; its little form is `list (pair 'k 'v)`)
    Data/Decode.nash      decoders
    Data/Encode.nash      encoders
    Prop.nash             generators
    Test.nash             label, assertFailed (`assert` is a keyword)
    Ast.nash              macro AST: little ADTs over cons (see macros.md)
    Derive.nash           @derive
    Cardano/Tx.nash       script context (sketch)
    Cardano/Address.nash
    Cardano/Value.nash
    Cardano/Time.nash
```

Module naming: one module per type pair (`List`, `Int`, `Bytes`, `Option`,
`Result`, `Ordering`, `Bool`, `Unit`). Named helpers accept either Big or little
outer inputs and return the little outer representation. Different arguments
may use different representations. Payloads retain their types: reversing a
`List Int` returns `list Int`, and `Option.unwrap : Option Int -> Int` leaves
its payload intact. `List.map` returns exactly the callback's result element type.
Trait methods such as `Functor.map` retain their trait's container contract.

`lift` and `lower` change only the outer representation; both have identity
conversion for already-matching types. Recursive element conversion requires
explicit mapping. There is no Big String: use `String.toBytes` / `fromBytes`
for UTF-8 and `lift` / `lower` for bytes/Bytes. `Data`, `Data.Decode` and
`Data.Encode` operate on Data itself. Map's dedicated API is planned separately.

## Default imports

Every module outside `nash/base` receives the following scope during
canonicalization. These imports are not inserted into the source AST or
formatted source. The driver adds dependency edges using the same catalog,
and diagnostic localization uses the same exposing rules.
`exposing (Trait)` exposes the trait and all its methods (traits.md).

```elm
import Primitive exposing (..)
import Prelude exposing (..)
import Eq exposing (Eq)
import Ord exposing (Ord)
import Show exposing (Show)
import Num exposing (Num)
import Integral exposing (Integral)
import Semigroup exposing (Semigroup)
import Monoid exposing (Monoid)
import Functor exposing (Functor)
import Applicative exposing (Applicative)
import Monad exposing (Monad)
import Lift exposing (Lift)
import Data exposing (ToData, FromData, Validate)
import Literal exposing (FromInt, FromString, FromBytes, FromBool, FromUnit)
import Bool exposing (Bool, not, and, or, xor)
import Unit exposing (Unit)
import Option exposing (Option, type option(..))
import Result exposing (Result, type result(..))
import Ordering exposing (Ordering, type ordering(..))
import Cons exposing (type cons(..))
import Builtin
import Pair
import Array
import Prop
import Test
```

Consequences, matching representation.md "Prelude twins":

- Little constructors are unqualified: `True`, `False`, `()`, `Some`,
  `None`, `Ok`, `Err`, `LT`, `EQ`, `GT`. `type option(..)` in the import
  is what exposes them; `True`/`False`/`()` come with the compiler-known
  `bool`/`unit`.
- Big twin constructors are qualified only: `Bool.True`, `Unit.Unit`,
  `Option.Some`, `Result.Ok`, `Ordering.LT`. Their modules are imported
  but the Big types are exposed without `(..)`, so the constructors never
  enter the unqualified scope. The Big type names are unqualified
  (`Option`, `Bool`). No resolution by expected type is involved.
- Operators come from `Prelude` (the `infix` declarations) and the methods
  they bind to come from the trait modules; both are in scope.
- Modules inside `nash/base` get no defaults and import explicitly (Elm
  does the same for `elm/core`).
- `Prop` and `Test` are default imports so `tests` blocks can use
  `Prop.int` and `label`; `Test.label` is additionally exposed unqualified
  inside `tests` blocks only (testing.md). `assert` is a keyword
  (syntax.md), not a `Test` function.

Differences from Elm's list: no `Char`, `Tuple`, `Platform`, `Cmd`, `Sub`.
`(::)` is exposed by `Prelude` rather than `List`.

## Compiler-known types

The `Primitive` module is seeded by the compiler with every builtin type
constructor (kinds.md "Datatype contexts", plans/02's
`nash-ast/src/primitives.rs`). They have no Nash declaration and every
one is in scope everywhere:

| Representation | Types |
|---|---|
| `Const` | `int`, `bytes`, `string`, `bool`, `unit`, `bls_g1`, `bls_g2`, `bls_mlr`, `value`, `list 'a`, `array 'a`, `pair 'a 'b` |
| `Big` | `Int`, `Bytes`, `Data`, `List 'a`, `Map 'k 'v` |

`bool` has constructors `False`, `True` (UPLC `bool` constants; `Ctor::Bool`
in `nash-can`). `unit` has `()`. `Data` has `Constr`, `Map`, `List`, `I`,
`B` with the little field types from data.md. There is no Big `String`;
`Bytes` carries UTF-8 text where a Big type is needed (`Ast` names).

Impls for compiler-known types live in the trait's module, because the
orphan rule needs either the trait or the head type to be local and
`Primitive` has no source.

## Prelude

`Prelude` holds only the `infix` table, the non-method functions the
table binds (`|>`, `<|`, `<<`, `>>`, `::` targets, plus
`identity`/`always`), and the prelude impls: impls for tuples, which
count as defined in `nash/base` under the orphan rule. Everything else
lives in the trait modules or the type modules. `Prelude` imports the traits it needs and `Bool` (for `&&`/`||`).
The application default-import catalog exposes all shipped traits independently;
Base modules use explicit imports to keep the bootstrap graph acyclic.
The `::` target is named `prepend` (not `cons`, which is the `Cons`
module's constructor) and stays in `Prelude` rather than `List` because
`List` imports `Prelude` for its operators.

```elm
module Prelude exposing (..)

import Eq exposing (Eq)
import Ord exposing (Ord)
import Show exposing (Show)
import Num exposing (Num)
import Integral exposing (Integral)
import Semigroup exposing (Semigroup)
import Functor exposing (Functor)
import Applicative exposing (Applicative)
import Monad exposing (Monad)
import Bool exposing (and, or)
import Builtin

infix left  0 (|>)  = applyForward
infix right 0 (<|)  = applyBackward
infix right 9 (<<)  = composeLeft
infix left  9 (>>)  = composeRight
infix right 2 (||)  = or
infix right 3 (&&)  = and
infix non   4 (==)  = eq
infix non   4 (/=)  = neq
infix non   4 (<)   = lt
infix non   4 (>)   = gt
infix non   4 (<=)  = le
infix non   4 (>=)  = ge
infix right 5 (++)  = append
infix right 5 (::)  = prepend
infix left  6 (+)   = add
infix left  6 (-)   = sub
infix left  7 (*)   = mul
infix left  7 (/)   = div
infix left  7 (%)   = mod
infix left  1 (>>=) = bind
infix left  4 (<$>) = map
infix left  4 (<*>) = apply

identity : 'a -> 'a
identity x = x

always : 'a -> 'b -> 'a
always x _ = x

applyForward : 'a -> ('a -> 'b) -> 'b
applyForward x f = f x

applyBackward : ('a -> 'b) -> 'a -> 'b
applyBackward f x = f x

composeLeft : ('b -> 'c) -> ('a -> 'b) -> 'a -> 'c
composeLeft g f x = g (f x)

composeRight : ('a -> 'b) -> ('b -> 'c) -> 'a -> 'c
composeRight f g x = g (f x)

prepend : 'a -> list 'a -> list 'a
prepend = Builtin.mkCons

impl (Eq 'a, Eq 'b) => Eq ('a, 'b) where
    eq (a1, b1) (a2, b2) = eq a1 a2 && eq b1 b2

-- ... Eq, Ord, Show for tuples up to 4
```

`/` is `Integral.div`; there is no `Float`. `-x` is `Num.negate x`.
`not`, `and`, `or`, `xor` are in `Bool` (functions over the little
`bool`).

## Trait modules

The declarations are exactly traits.md "Core trait hierarchy"; they are
not repeated here. What each module adds beyond its trait:

| Module | Impls for compiler-known types |
|---|---|
| `Eq` | `int`, `bytes`, `string`, `bool`, `unit`, `list 'a` (given `Eq 'a`), `pair 'a 'b`, `Data`, `Int`, `Bytes`, `List 'a`, `Map 'k 'v` (all Big ones via `equalsData`) |
| `Ord` | `int`, `bytes`, `string` (bytewise), `bool`, `unit`, `list 'a`, `Int`, `Bytes` |
| `Show` | `int`, `bytes` (hex), `string`, `bool`, `unit`, `list 'a`, `pair`, `Data`, `Int`, `Bytes`, `List 'a`, `Map 'k 'v` |
| `Num`, `Integral` | `int`, `Int` |
| `Semigroup`, `Monoid` | `bytes`, `string`, `list 'a`, `Bytes`, `List 'a`, `Map 'k 'v` (right-biased union), `unit` |
| `Functor` | `list`, `List`; each applied element must satisfy its constructor's datatype context (`Storable` for `list`, `Big` for `List`). No builtin pair Functor impl. |
| `Applicative`, `Monad` | No builtin `list` impls: list cannot hold functions required by apply. No impls for Big List. |
| `Lift` | Outer-only conversions in representation.md; reflexive identity for every type. |
| `Data` | Blanket `ToData` and `FromData` for every Big type; `Validate` for `Data`, `Int`, `Bytes`, `List 'a`, `Map 'k 'v` |
| `Literal` | `FromInt int`, `FromInt Int`, `FromString string`, `FromString bytes` (UTF-8), `FromBytes bytes`, `FromBytes Bytes`, `FromBool bool`, `FromUnit unit` |
| `Bool`, `Unit` | `FromBool Bool`, `FromUnit Unit` |

Tuple impls (`Eq`, `Ord`, `Show` up to 4) are in `Prelude`. Impls for the
twin types (`option`, `Option`, ...) are in the twin's module.

The shipping hierarchy provides Functor for `list`, `List`, `cons`, `option`
and `result 'e`, and Applicative/Monad for `option` and `result 'e`.
Big List mapping uses an explicitly typed little-list helper between Lift
conversions, keeping the intermediate container unambiguous. Builtin list
mapping can change element representations within Storable; it cannot produce Term
elements. Builtin pair has no Functor impl: `mkPairData` accepts only Big
components, not arbitrary Storable components needed by `map`. Pair.fst,
Pair.snd and Pair.make remain the specified projection/construction helpers.
The `generator` impls require the real Prop implementation from plan 10.

### Equality at the Big boundary

Every Big type uses structural Data equality. User Eq impls for Big types
are forbidden; a custom Eq method cannot change equality of a Big value.
The compiler supplies Eq for Big types, including user-defined ADTs and
nominal aliases, through the shared kind, representation and evidence contracts.

For builtin `list 'a` with `'a : Big`, the stdlib equality route is
`equalsData (listData left) (listData right)` *as a codegen rewrite*: there
is exactly one `impl Eq 'a => Eq (list 'a)` (elementwise), because impl
coherence is head-only and contexts are not part of an impl's identity
(kinds.md "Traits and impls"). plans/08 replaces the monomorphized
elementwise body with the `equalsData` comparison whenever the ground
element type is Big; the semantics are identical because Big equality is
structural equality on each element. Typed `listData` accepts Big elements
directly and produces `List 'a`; any rewrite to equalsData must account for
the concrete runtime representation without introducing source coercions. Generic `Ord (list 'a)` retains `Ord 'a` in its context
and gets `Eq (list 'a)` through the superclass.
Map Data equality compares the encoded sequence of entries, including order
and duplicates. It is not dictionary-style equality.

Lowercase `value` is a dedicated Const ledger-value representation. Its Eq
impl compares `valueData` results with `equalsData`. There is no
`equalsValue` entry in the target builtin table. `valueContains` is a
quantity containment operation and rejects negative amounts, so it must not
be substituted for equality. `unionValue` adds asset quantities; it is not
ordinary Map's right-biased union. Big `Cardano.Value` remains subject to
structural Big equality; converting to lowercase value uses the dedicated
validated `unValueData` boundary.

`Show` uses these diagnostic text formats:

- Integers use decimal digits, with a leading minus for negative values.
- Bytes use lowercase hexadecimal inside Nash's `#"..."` literal syntax.
- Strings are quoted. Quotes, backslashes, newline, carriage return and tab
  use their Nash escapes; other ASCII controls below U+0020 use four-digit
  Unicode escapes. Other Unicode text is preserved.
- Booleans render as `True` or `False`; unit renders as `()`.
- Lists use `[a, b]`; builtin pairs use `(a, b)`, recursively showing elements.
- Big Int and Bytes use the same decimal and hexadecimal formats as their
  little twins. Big List uses `[a, b]` and requires Show for its element type.
  Big Map uses `Map [(key, value)]` and requires Show for both key and value
  types. These containers show their typed elements, not erased Data fields.
- Tuples through four components use `(a, b, ...)`, recursively showing each
  component with a comma and space between components.
- Data uses `Constr payload` with a `pair int (list Data)` payload, `Map [(key, value)]`, `List [values]`,
  `I integer`, or `B bytes`, recursively using the formats above.

These are display formats, not a promise that arbitrary shown values can be
parsed back into their original nominal types. Plan 07 execution acceptance
must verify the actual strings, including zero and negative integers, empty
bytes/lists, leading-zero hex bytes, escaped controls, multibyte Unicode and
nested Data maps and heterogeneous four-component tuples. Plan 03 compilation
only verifies types and trait evidence.

Examples of the bodies (`Eq.nash`, `Lift.nash`, `Data.nash`):

```elm
module Eq exposing (Eq)

import Builtin

trait Eq 'a where
    eq : 'a -> 'a -> bool

    neq : 'a -> 'a -> bool
    neq a b = if eq a b then False else True

impl Eq int where
    eq = Builtin.equalsInteger

impl Eq bytes where
    eq = Builtin.equalsByteString

impl Eq string where
    eq = Builtin.equalsString

impl Eq bool where
    eq a b = if a then b else if b then False else True

impl Eq unit where
    eq _ _ = True

-- One impl; plans/08 rewrites it to `equalsData` on `listData` when the
-- ground element is Big.
impl Eq 'a => Eq (list 'a) where
    eq xs ys =
        if Builtin.nullList xs then Builtin.nullList ys
        else if Builtin.nullList ys then False
        else if eq (Builtin.headList xs) (Builtin.headList ys) then eq (Builtin.tailList xs) (Builtin.tailList ys)
        else False

-- Big Eq is compiler-provided; no explicit Data or Int impl is declared.
```

```elm
module Lift exposing (Lift)

import Builtin

import Primitive exposing (Big)

trait Lift 'small 'big where
    lift : 'small -> 'big
    lower : 'big -> 'small

impl Lift int Int where
    lift = Builtin.iData
    lower = Builtin.unIData

impl Lift bytes Bytes where
    lift = Builtin.bData
    lower = Builtin.unBData

impl Lift (list ('a : Big)) (List 'a) where
    lift = Builtin.listData
    lower = Builtin.unListData

impl (Big 'k, Big 'v) => Lift (list (pair 'k 'v)) (Map 'k 'v) where
    lift = Builtin.mapData
    lower = Builtin.unMapData
```

Twin modules own their outer-only Lift implementations. The compiler provides
identity when both Lift arguments are already the same type. Map wrapping
requires Big keys and values and does not convert its entries.

```elm
module Data exposing (ToData, FromData, Validate, serialise, tag, fields)

import Primitive exposing (Data(..))

trait ToData ('a : Big) where
    toData : 'a -> Data

impl ToData ('a : Big) where
    toData = Primitive.coerce

trait FromData ('a : Big) where
    fromData : Data -> 'a

impl FromData ('a : Big) where
    fromData = Primitive.coerce

trait Validate ('a : Big) where
    validate : Data -> 'a

-- Data itself requires no decoding.
impl Validate Data where
    validate value = value

impl Validate Int where
    validate value =
        case value of
            I _ -> Primitive.coerce value
            _ -> fail

serialise : Data -> bytes
serialise = Builtin.serialiseData

tag : Data -> option int
tag d =
    case d of
        Constr pair(t, _) -> Some t
        _ -> None

fields : Data -> option (list Data)
fields d =
    case d of
        Constr pair(_, fs) -> Some fs
        _ -> None
```

`Data` fields in patterns are little (`Constr (pair int (list Data))`), as data.md
specifies, so no `lower` is needed on `t` and `fs`.
Other Big types remain nominally distinct from Data. `fromData` uses
unchecked `Primitive.coerce`, with no outer-shape or nested checks. Malformed
data fails only if a later operation needs its expected shape. Its blanket impl
also covers user ADTs, nominal record aliases and collections, without any
validation constraints. The required
`Validate.validate` method is separate: Int and Bytes check the shape and coerce
the original value, while List and Map retain recursive source validation.
`toData` uses `Primitive.coerce` directly in one ordinary blanket impl
for every Big type. User ADTs, nominal aliases, lists and maps all qualify,
without element `ToData` constraints or reconstruction. Additional concrete
impls overlap this blanket impl and are rejected. Data.Decode provides the
non-failing decoder API.

## Twin modules

Option and Result helpers accept either twin and return little containers.
Their Lift instances preserve payloads, including errors, without mapping.
`Option.unwrap` fails on None; `withDefault` returns its supplied fallback.
`map2` returns None if either option is None, or the leftmost error for Result.
`andThen` also normalizes the callback's returned outer container.

## Compiler special cases

| Name | Treatment |
|---|---|
| `Primitive.bool`, `False`, `True` | `if` scrutinee type; UPLC `bool` constants; `Ctor::Bool` in `nash-can` (`crates/nash-can/src/environment/foreign.rs`, `make_union_ctor`) |
| `Primitive.unit`, `()` | UPLC `unit` constant |
| `Primitive.Data` and its constructors | pattern-matchable Big type (data.md) |
| `Primitive.list`, `[..]`, `::` patterns | list literals and patterns; element predicate `Storable` |
| `Bool.and`, `Bool.or` | second argument delayed (`&&`, `||` are lazy) |
| `Literal.FromInt`, `FromString`, `FromBytes`, `FromBool`, `FromUnit` | expression literal desugaring and defaulting to `int`, `string`, `bytes`, `bool`, `unit` |
| `Eq.Eq` | literal patterns |
| `Monad.Monad` | `do` desugaring target |
| `Show.Show` | power-assert rendering of operands |
| Real `Builtin.*` operations | direct UPLC builtin nodes (below) |
| `Primitive.coerce` | unchecked compiler intrinsic; runtime identity |
| `trace`, `todo`, `fail` syntax | trace levels, compiler-generated traces switch |
| `assert` keyword, `Test.assertFailed` | power-assert rewrite in `tests` blocks traces the operands and calls `Test.assertFailed`; elsewhere `assert e` is `if e then () else fail` (testing.md) |
| `Prop.generator`, `Prop.Prng` | `prop`/`via` desugaring and the runner protocol (`draw`/`run` programs, plans/10 chunk 4) |
| `Ast.*`, `Cons.cons` | reified by `nash-macro` as `Term::Constr` trees by constructor index and walked back after evaluation (macros.md); the compiler knows the tag table, the Nash side is plain little ADTs |
| `Derive.derive` | nothing special beyond being a macro; listed because default imports expose it |

## Builtin

`Builtin` is a synthetic module containing only actual Plutus Core builtin
functions. Its interface comes from `BUILTINS` in
`crates/nash-ast/src/primitives/builtins.rs`, re-exported by `primitives.rs`.
Each entry maps a `DefaultFunction` variant to a Nash name and type.
The canonicalizer derives representation predicates from those signatures;
the backend resolves the symbolic variant without making the AST depend on
the runtime.

`Primitive` separately owns the compiler-known types, their constructors,
and the representation traits `Const`, `Little`, `Big`, `Term`, and `Storable`.
The `unit` spelling and `()` canonicalize to the same primitive type.

`nash-can` resolves real `Builtin.foo` operations to `VarForeign { home: Builtin }` and
`nash-codegen` lowers that to `Core::Builtin` (plans/07 chunk 4),
applying the variant's `force_count()` forces. `nash docs` renders the
same table.

`Primitive` exposes `coerce : 'a -> 'b`, an unchecked compiler
intrinsic separate from the real Plutus builtin inventory. Its type variables
independently accept any value types, including functions, without
representation constraints. It returns the original runtime value with no
validation or representation change.

Rules:

- Every builtin is strict in every argument, including `ifThenElse`,
  `chooseList`, `chooseData`, and `trace`. `if`, `case`, `&&`, `||` use
  lazy native case dispatch at protocol 11; `Builtin.ifThenElse` is the raw
  strict builtin.
- Type variables have representation prerequisites as UPLC requires: elements of
  `list`/`array` and components of `pair` are `Storable`; `'a` in
  `ifThenElse`, `chooseUnit`, `chooseList`, `chooseData`, `trace` has no
  representation restriction. All these value variables have kind `Type`.
- The table contains only actual UPLC DefaultFunction operations. Identity
  is an ordinary Nash function and failure uses `fail` syntax.
- Data conversion builtin signatures preserve nominal primitive and collection
  types. Existing universal Data constructors and patterns provide explicit
  source codecs between those types and Data. `coerce` is a separate unchecked
  intrinsic; no compiler-generated checkers are exposed.

| DefaultFunction | Nash name | Type |
|---|---|---|
| `AddInteger` | `addInteger` | `int -> int -> int` |
| `SubtractInteger` | `subtractInteger` | `int -> int -> int` |
| `MultiplyInteger` | `multiplyInteger` | `int -> int -> int` |
| `DivideInteger` | `divideInteger` | `int -> int -> int` |
| `QuotientInteger` | `quotientInteger` | `int -> int -> int` |
| `RemainderInteger` | `remainderInteger` | `int -> int -> int` |
| `ModInteger` | `modInteger` | `int -> int -> int` |
| `EqualsInteger` | `equalsInteger` | `int -> int -> bool` |
| `LessThanInteger` | `lessThanInteger` | `int -> int -> bool` |
| `LessThanEqualsInteger` | `lessThanEqualsInteger` | `int -> int -> bool` |
| `AppendByteString` | `appendByteString` | `bytes -> bytes -> bytes` |
| `ConsByteString` | `consByteString` | `int -> bytes -> bytes` |
| `SliceByteString` | `sliceByteString` | `int -> int -> bytes -> bytes` |
| `LengthOfByteString` | `lengthOfByteString` | `bytes -> int` |
| `IndexByteString` | `indexByteString` | `bytes -> int -> int` |
| `EqualsByteString` | `equalsByteString` | `bytes -> bytes -> bool` |
| `LessThanByteString` | `lessThanByteString` | `bytes -> bytes -> bool` |
| `LessThanEqualsByteString` | `lessThanEqualsByteString` | `bytes -> bytes -> bool` |
| `Sha2_256` | `sha2_256` | `bytes -> bytes` |
| `Sha3_256` | `sha3_256` | `bytes -> bytes` |
| `Blake2b_256` | `blake2b_256` | `bytes -> bytes` |
| `Blake2b_224` | `blake2b_224` | `bytes -> bytes` |
| `Keccak_256` | `keccak_256` | `bytes -> bytes` |
| `Ripemd_160` | `ripemd_160` | `bytes -> bytes` |
| `VerifyEd25519Signature` | `verifyEd25519Signature` | `bytes -> bytes -> bytes -> bool` |
| `VerifyEcdsaSecp256k1Signature` | `verifyEcdsaSecp256k1Signature` | `bytes -> bytes -> bytes -> bool` |
| `VerifySchnorrSecp256k1Signature` | `verifySchnorrSecp256k1Signature` | `bytes -> bytes -> bytes -> bool` |
| `AppendString` | `appendString` | `string -> string -> string` |
| `EqualsString` | `equalsString` | `string -> string -> bool` |
| `EncodeUtf8` | `encodeUtf8` | `string -> bytes` |
| `DecodeUtf8` | `decodeUtf8` | `bytes -> string` |
| `IfThenElse` | `ifThenElse` | `bool -> 'a -> 'a -> 'a` |
| `ChooseUnit` | `chooseUnit` | `unit -> 'a -> 'a` |
| `Trace` | `trace` | `string -> 'a -> 'a` |
| `FstPair` | `fstPair` | `pair 'a 'b -> 'a` |
| `SndPair` | `sndPair` | `pair 'a 'b -> 'b` |
| `ChooseList` | `chooseList` | `list 'a -> 'b -> 'b -> 'b` |
| `MkCons` | `mkCons` | `'a -> list 'a -> list 'a` |
| `HeadList` | `headList` | `list 'a -> 'a` |
| `TailList` | `tailList` | `list 'a -> list 'a` |
| `NullList` | `nullList` | `list 'a -> bool` |
| `DropList` | `dropList` | `int -> list 'a -> list 'a` |
| `ChooseData` | `chooseData` | `Data -> 'a -> 'a -> 'a -> 'a -> 'a -> 'a` |
| `ConstrData` | `constrData` | `int -> list Data -> Data` |
| `MapData` | `mapData` | `(Big 'k, Big 'v) => list (pair 'k 'v) -> Map 'k 'v` |
| `ListData` | `listData` | `list ('a : Big) -> List 'a` |
| `IData` | `iData` | `int -> Int` |
| `BData` | `bData` | `bytes -> Bytes` |
| `UnConstrData` | `unConstrData` | `Data -> pair int (list Data)` |
| `UnMapData` | `unMapData` | `(Big 'k, Big 'v) => Map 'k 'v -> list (pair 'k 'v)` |
| `UnListData` | `unListData` | `Big 'a => List 'a -> list 'a` |
| `UnIData` | `unIData` | `Int -> int` |
| `UnBData` | `unBData` | `Bytes -> bytes` |
| `EqualsData` | `equalsData` | `Data -> Data -> bool` |
| `SerialiseData` | `serialiseData` | `Data -> bytes` |
| `MkPairData` | `mkPairData` | `(Big 'a, Big 'b) => 'a -> 'b -> pair 'a 'b` |
| `MkNilData` | `mkNilData` | `unit -> list Data` |
| `MkNilPairData` | `mkNilPairData` | `unit -> list (pair Data Data)` |
| `Bls12_381_G1_Add` | `bls12_381_g1_add` | `bls_g1 -> bls_g1 -> bls_g1` |
| `Bls12_381_G1_Neg` | `bls12_381_g1_neg` | `bls_g1 -> bls_g1` |
| `Bls12_381_G1_ScalarMul` | `bls12_381_g1_scalarMul` | `int -> bls_g1 -> bls_g1` |
| `Bls12_381_G1_Equal` | `bls12_381_g1_equal` | `bls_g1 -> bls_g1 -> bool` |
| `Bls12_381_G1_Compress` | `bls12_381_g1_compress` | `bls_g1 -> bytes` |
| `Bls12_381_G1_Uncompress` | `bls12_381_g1_uncompress` | `bytes -> bls_g1` |
| `Bls12_381_G1_HashToGroup` | `bls12_381_g1_hashToGroup` | `bytes -> bytes -> bls_g1` |
| `Bls12_381_G1_MultiScalarMul` | `bls12_381_g1_multiScalarMul` | `list int -> list bls_g1 -> bls_g1` |
| `Bls12_381_G2_Add` | `bls12_381_g2_add` | `bls_g2 -> bls_g2 -> bls_g2` |
| `Bls12_381_G2_Neg` | `bls12_381_g2_neg` | `bls_g2 -> bls_g2` |
| `Bls12_381_G2_ScalarMul` | `bls12_381_g2_scalarMul` | `int -> bls_g2 -> bls_g2` |
| `Bls12_381_G2_Equal` | `bls12_381_g2_equal` | `bls_g2 -> bls_g2 -> bool` |
| `Bls12_381_G2_Compress` | `bls12_381_g2_compress` | `bls_g2 -> bytes` |
| `Bls12_381_G2_Uncompress` | `bls12_381_g2_uncompress` | `bytes -> bls_g2` |
| `Bls12_381_G2_HashToGroup` | `bls12_381_g2_hashToGroup` | `bytes -> bytes -> bls_g2` |
| `Bls12_381_G2_MultiScalarMul` | `bls12_381_g2_multiScalarMul` | `list int -> list bls_g2 -> bls_g2` |
| `Bls12_381_MillerLoop` | `bls12_381_millerLoop` | `bls_g1 -> bls_g2 -> bls_mlr` |
| `Bls12_381_MulMlResult` | `bls12_381_mulMlResult` | `bls_mlr -> bls_mlr -> bls_mlr` |
| `Bls12_381_FinalVerify` | `bls12_381_finalVerify` | `bls_mlr -> bls_mlr -> bool` |
| `IntegerToByteString` | `integerToByteString` | `bool -> int -> int -> bytes` |
| `ByteStringToInteger` | `byteStringToInteger` | `bool -> bytes -> int` |
| `AndByteString` | `andByteString` | `bool -> bytes -> bytes -> bytes` |
| `OrByteString` | `orByteString` | `bool -> bytes -> bytes -> bytes` |
| `XorByteString` | `xorByteString` | `bool -> bytes -> bytes -> bytes` |
| `ComplementByteString` | `complementByteString` | `bytes -> bytes` |
| `ReadBit` | `readBit` | `bytes -> int -> bool` |
| `WriteBits` | `writeBits` | `bytes -> list int -> bool -> bytes` |
| `ReplicateByte` | `replicateByte` | `int -> int -> bytes` |
| `ShiftByteString` | `shiftByteString` | `bytes -> int -> bytes` |
| `RotateByteString` | `rotateByteString` | `bytes -> int -> bytes` |
| `CountSetBits` | `countSetBits` | `bytes -> int` |
| `FindFirstSetBit` | `findFirstSetBit` | `bytes -> int` |
| `ExpModInteger` | `expModInteger` | `int -> int -> int -> int` |
| `LengthOfArray` | `lengthOfArray` | `array 'a -> int` |
| `ListToArray` | `listToArray` | `list 'a -> array 'a` |
| `IndexArray` | `indexArray` | `array 'a -> int -> 'a` |
| `InsertCoin` | `insertCoin` | `bytes -> bytes -> int -> value -> value` |
| `LookupCoin` | `lookupCoin` | `bytes -> bytes -> value -> int` |
| `UnionValue` | `unionValue` | `value -> value -> value` |
| `ValueContains` | `valueContains` | `value -> value -> bool` |
| `ValueData` | `valueData` | `value -> Data` |
| `UnValueData` | `unValueData` | `Data -> value` |
| `ScaleValue` | `scaleValue` | `int -> value -> value` |

Availability by Plutus version is not modelled in the type; `nash build`
rejects programs that use builtins newer than the target's version
(overview: target is V3 with the latest builtins).

## Little-type modules

### `Bool`

```elm
not : Lift bool 'a => 'a -> bool
and : (Lift bool 'a, Lift bool 'b) => 'a -> 'b -> bool
or : (Lift bool 'a, Lift bool 'b) => 'a -> 'b -> bool
xor : (Lift bool 'a, Lift bool 'b) => 'a -> 'b -> bool
```

### `Int`

```elm
abs : Lift int 'n => 'n -> int
pow : (Lift int 'n, Lift int 'e) => 'n -> 'e -> int
powMod : (Lift int 'n, Lift int 'e, Lift int 'm) => 'n -> 'e -> 'm -> int
toBytes : (Lift bool 'b, Lift int 's, Lift int 'n) => 'b -> 's -> 'n -> bytes
fromBytes : (Lift bool 'b, Lift bytes 'v) => 'b -> 'v -> int
toString : Lift int 'n => 'n -> string
```

### `Bytes`

```elm
length : Lift bytes 'b => 'b -> int
at : (Lift bytes 'b, Lift int 'n) => 'b -> 'n -> int
slice : (Lift int 's, Lift int 'n, Lift bytes 'b) => 's -> 'n -> 'b -> bytes
take : (Lift int 'n, Lift bytes 'b) => 'n -> 'b -> bytes
drop : (Lift int 'n, Lift bytes 'b) => 'n -> 'b -> bytes
concat : (Lift (list 'b) ('f 'b), Lift bytes 'b) => ('f 'b) -> bytes
sha2_256 : Lift bytes 'b => 'b -> bytes
sha3_256 : Lift bytes 'b => 'b -> bytes
blake2b_256 : Lift bytes 'b => 'b -> bytes
blake2b_224 : Lift bytes 'b => 'b -> bytes
keccak_256 : Lift bytes 'b => 'b -> bytes
ripemd_160 : Lift bytes 'b => 'b -> bytes
complement : Lift bytes 'b => 'b -> bytes
countSetBits : Lift bytes 'b => 'b -> int
findFirstSetBit : Lift bytes 'b => 'b -> int
and : (Lift bool 'p, Lift bytes 'a, Lift bytes 'b) => 'p -> 'a -> 'b -> bytes
or : (Lift bool 'p, Lift bytes 'a, Lift bytes 'b) => 'p -> 'a -> 'b -> bytes
xor : (Lift bool 'p, Lift bytes 'a, Lift bytes 'b) => 'p -> 'a -> 'b -> bytes
readBit : (Lift bytes 'b, Lift int 'n) => 'b -> 'n -> bool
shift : (Lift bytes 'b, Lift int 'n) => 'b -> 'n -> bytes
rotate : (Lift bytes 'b, Lift int 'n) => 'b -> 'n -> bytes
writeBits : (Lift bytes 'b, Lift (list 'n) ('f 'n), Lift int 'n, Lift bool 'v) => 'b -> ('f 'n) -> 'v -> bytes
toHex : Lift bytes 'b => 'b -> string
```

### `String`

```elm
toBytes : string -> bytes
fromBytes : Lift bytes 'b => 'b -> string
fromInt : Lift int 'n => 'n -> string
concat : list string -> string
join : string -> list string -> string
```

### `List`

```elm
singleton : 'a -> list 'a
repeat : Lift int 'n => 'n -> 'a -> list 'a
range : (Lift int 's, Lift int 'e) => 's -> 'e -> list int
head : Lift (list 'a) ('f 'a) => ('f 'a) -> option 'a
tail : Lift (list 'a) ('f 'a) => ('f 'a) -> option (list 'a)
isEmpty : Lift (list 'a) ('f 'a) => ('f 'a) -> bool
length : Lift (list 'a) ('f 'a) => ('f 'a) -> int
isLength : (Lift (list 'a) ('f 'a), Lift int 'n) => ('f 'a) -> 'n -> bool
reverse : Lift (list 'a) ('f 'a) => ('f 'a) -> list 'a
append : (Lift (list 'a) ('f 'a), Lift (list 'a) ('g 'a)) => ('f 'a) -> ('g 'a) -> list 'a
concat : (Lift (list ('f 'a)) ('g ('f 'a)), Lift (list 'a) ('f 'a)) => 'g ('f 'a) -> list 'a
map : Lift (list 'a) ('f 'a) => ('a -> 'b) -> ('f 'a) -> list 'b
indexedMap : Lift (list 'a) ('f 'a) => (int -> 'a -> 'b) -> ('f 'a) -> list 'b
filter : (Lift (list 'a) ('f 'a), Lift bool 'p) => ('a -> 'p) -> ('f 'a) -> list 'a
filterMap : (Lift (list 'a) ('f 'a), Lift (option 'b) ('g 'b)) => ('a -> 'g 'b) -> ('f 'a) -> list 'b
foldl : Lift (list 'a) ('f 'a) => ('a -> 'b -> 'b) -> 'b -> ('f 'a) -> 'b
foldr : Lift (list 'a) ('f 'a) => ('a -> 'b -> 'b) -> 'b -> ('f 'a) -> 'b
any : (Lift (list 'a) ('f 'a), Lift bool 'p) => ('a -> 'p) -> ('f 'a) -> bool
all : (Lift (list 'a) ('f 'a), Lift bool 'p) => ('a -> 'p) -> ('f 'a) -> bool
find : (Lift (list 'a) ('f 'a), Lift bool 'p) => ('a -> 'p) -> ('f 'a) -> option 'a
member : (Eq 'a, Lift (list 'a) ('f 'a)) => 'a -> ('f 'a) -> bool
take : (Lift int 'n, Lift (list 'a) ('f 'a)) => 'n -> ('f 'a) -> list 'a
drop : (Lift int 'n, Lift (list 'a) ('f 'a)) => 'n -> ('f 'a) -> list 'a
at : (Lift int 'n, Lift (list 'a) ('f 'a)) => 'n -> ('f 'a) -> option 'a
map2 : (Lift (list 'a) ('f 'a), Lift (list 'b) ('g 'b)) => ('a -> 'b -> 'c) -> ('f 'a) -> ('g 'b) -> list 'c
sort : (Ord 'a, Lift (list 'a) ('f 'a)) => ('f 'a) -> list 'a
sortBy : (Lift (list 'a) ('f 'a), Lift ordering 'o) => ('a -> 'a -> 'o) -> ('f 'a) -> list 'a
sum : (Lift int 'a, Lift (list 'a) ('f 'a)) => 'f 'a -> int
partition : (Lift (list 'a) ('f 'a), Lift bool 'p) => ('a -> 'p) -> ('f 'a) -> (list 'a, list 'a)
toArray : Lift (list 'a) ('f 'a) => ('f 'a) -> array 'a
```

`isLength values count` checks for exactly `count` elements. Negative counts
return `False`; zero matches only an empty list. Positive counts use one
`dropList (count - 1)` and a singleton pattern, without counting the full list.

The higher-kinded inputs above share their element types with the little
result; e.g. `Lift (list 'a) ('f 'a)` admits both list and List without
converting `'a`. Binary helpers use independent input constructors.

List counts at or below zero produce empty take/repeat results and unchanged
input for drop. Range is start-inclusive and end-exclusive; start >= end is
empty. Access outside the list returns None. map2 truncates to the shorter
input. Sorting is stable. any/all stop at the first decisive element; empty
any is False and empty all is True. Sum computes an int for native or Big integer elements; empty sum is zero.
Predicates, comparators, and filterMap callbacks accept either outer representation. Fold callbacks take
item then accumulator. Native dropList handles list skipping.

Int.pow rejects negative exponents and returns 1 for exponent zero. Int.powMod
uses Plutus expModInteger semantics, including its invalid-modulus and modular
inverse failures. Int.toBytes rejects negative values, invalid sizes and values
that do not fit. Bytes.at/readBit/writeBits fail for invalid indices; slice
uses Plutus start/count clamping. String.fromBytes fails for invalid UTF-8.
Byte logic takes the builtin padding flag; shift/rotate and hash functions
use the corresponding Plutus builtin semantics. Bytes.toHex is lowercase
without a prefix. String.join inserts separators only between entries.

Bool.and/or and &&/|| short-circuit fully applied calls, including mixed
Big/little operands. Partial applications remain strict. A polymorphic failing
operand needs an annotation to identify its representation.

### `Cons`

A Term linked list for elements `list` cannot hold (functions,
tuples, little ADTs such as `Ast` nodes). Each cell is a UPLC `constr`
(`Nil` = `constr 0 []`, `Cons x xs` = `constr 1 [x, xs]`), so elements
may have any representation. It is the list type of the macro `Ast` family
(macros.md) and is default-imported with its constructors open.

```elm
module Cons exposing (type cons(..), map, indexedMap, map2, foldr, foldl, append, length, singleton, fromList, toList)

import Prelude exposing (..)
import Functor exposing (Functor)
import Eq exposing (Eq)

type cons 'a = Nil | Cons 'a (cons 'a)

singleton : 'a -> cons 'a
map : ('a -> 'b) -> cons 'a -> cons 'b            -- also `impl Functor cons`
indexedMap : (int -> 'a -> 'b) -> cons 'a -> cons 'b
map2 : ('a -> 'b -> 'c) -> cons 'a -> cons 'b -> cons 'c
foldr : ('a -> 'b -> 'b) -> 'b -> cons 'a -> 'b
foldl : ('a -> 'b -> 'b) -> 'b -> cons 'a -> 'b
append : cons 'a -> cons 'a -> cons 'a
length : cons 'a -> int
fromList : Lift (list 'a) ('f 'a) => 'f 'a -> cons 'a                     -- 'a : Storable, from `list`
toList : cons 'a -> list 'a                       -- 'a : Storable, from `list`
```

`fromList`/`toList` carry the `Storable 'a` predicate that `list 'a`
implies (kinds.md); `Eq (cons 'a)` and `Show (cons 'a)` impls are in
`Cons` too.

`indexedMap` starts at zero. `map2` stops at the shorter input. The folds
pass the element before the accumulator; foldl traverses left to right and
foldr associates from the right. Show renders `Cons [a, b]` (or `Cons []`),
using each element's Show impl, so Term elements such as tuples are supported.

### `Pair`, `Array`

```elm
module Pair exposing (..)
fst : pair 'a 'b -> 'a
fst pair(first, _) = first
snd : pair 'a 'b -> 'b
snd pair(_, second) = second
make : Data -> Data -> pair Data Data      -- only Data pairs can be built

module Array exposing (..)
fromList : Lift (list 'a) ('f 'a) => 'f 'a -> array 'a
length : array 'a -> int
at : Lift int 'n => array 'a -> 'n -> 'a                 -- errors out of bounds
get : Lift int 'n => array 'a -> 'n -> option 'a
```

`Array.get` returns None for a negative index or an index at least the
array length; otherwise it returns Some of the indexed element. Both array
conversion and access retain the element's Storable bound. Pair projections
accept all Storable component types, while Pair.make only constructs Data
pairs, matching the target builtin.

### `Map`

`Map 'k 'v` is an association list in `Data.Map` encoding, keys and
values Big, insertion-ordered, no dedup on construction:

The Semigroup impl requires `Eq 'k`. Right-biased union removes each left
entry whose key equals any right key, then appends the right entries. It
preserves the order and duplicates of surviving left entries and all right
entries. The Monoid impl has the same key constraint through its superclass;
its identity is the empty map. Thus appending empty does not deduplicate a
map. Key comparison uses the key's Eq impl, as do the Map lookup helpers.

```elm
module Map exposing (..)

empty : Map 'k 'v
singleton : 'k -> 'v -> Map 'k 'v
insert : Eq 'k => 'k -> 'v -> Map 'k 'v -> Map 'k 'v
get : Eq 'k => 'k -> Map 'k 'v -> option 'v
remove : Eq 'k => 'k -> Map 'k 'v -> Map 'k 'v
keys : Map 'k 'v -> list 'k
values : Map 'k 'v -> list 'v
toList : Map 'k 'v -> list (pair 'k 'v)
fromList : list (pair 'k 'v) -> Map 'k 'v
foldl : ('k -> 'v -> 'b -> 'b) -> 'b -> Map 'k 'v -> 'b
```

## `Data.Decode`, `Data.Encode`

Structured decoders for untrusted `Data` return failure as a value.
`Validate.validate` is the separate trapping validation path; future
derivation generates recursive source checks.

```elm
module Data.Decode exposing (..)

import List

type alias decoder 'a = Data -> option 'a

run : decoder 'a -> Data -> option 'a
int : decoder int
bytes : decoder bytes
string : decoder string
bool : decoder bool
data : decoder Data
list : decoder 'a -> decoder (list 'a)
map : decoder 'k -> decoder 'v -> decoder (list (pair 'k 'v))
constr : int -> decoder 'a -> decoder 'a          -- checks the tag, then decodes the same node
field : int -> decoder 'a -> decoder 'a           -- nth field of a Constr
fields2 : decoder 'a -> decoder 'b -> decoder ('a, 'b)
fields3 : ...
oneOf : list (decoder 'a) -> decoder 'a
succeed : 'a -> decoder 'a
fail : decoder 'a
andThen : ('a -> decoder 'b) -> decoder 'a -> decoder 'b
```

`decoder` is a little alias of a function type (`Term` representation), so it has no
`Functor`/`Monad` impls (impls attach to nominal types); v1 uses
`andThen`. `Data.Encode` is the inverse: `int : int -> Data`, `bytes`,
`string`, `bool`, `list : ('a -> Data) -> list 'a -> Data`,
`constr : int -> list Data -> Data`, `map`.

## `Prop`

Plan 10 supplies the executable core subset. `oneOf` uses `Cons.cons` because
generators contain functions and cannot inhabit the native Storable-only list.
Further helpers below remain Plan 12 work. See testing.md "Generators": `Prng` is Big, choices are `Int`s, the
primitive is `choice`.

```elm
module Prop exposing (..)

import Prelude exposing (..)
import Builtin
import Functor exposing (Functor)
import Applicative exposing (Applicative)
import Monad exposing (Monad)
import Lift exposing (Lift)
import List

type Prng = Seeded Bytes (List Int) | Replayed Int (List Int)

type generator 'a = Generator (Prng -> option (Prng, 'a))

run : generator 'a -> Prng -> option (Prng, 'a)
run (Generator f) = f

-- Draw an integer in [0, bound]. The only primitive.
choice : int -> generator int
choice bound =
    Generator
        (\prng ->
            case prng of
                Seeded seed choices ->
                    let
                        seed2 = Builtin.blake2b_256 (lower seed)
                        n = Builtin.byteStringToInteger True seed2 % (bound + 1)
                    in
                    Some (Seeded (lift seed2) (lift (lift n :: lower choices)), n)

                Replayed 0 _ -> None
                Replayed k rest ->
                    case lower rest of
                        c :: cs ->
                            if lower c <= bound then Some (Replayed (lift (k - 1)) (lift cs), lower c) else None
                        [] -> None)

impl Functor generator where
    map f (Generator g) =
        Generator (\prng ->
            case g prng of
                None -> None
                Some (p, a) -> Some (p, f a))

impl Applicative generator where
    pure a = Generator (\prng -> Some (prng, a))
    apply ff fa = bind ff (\f -> map f fa)

impl Monad generator where
    bind (Generator g) k =
        Generator (\prng ->
            case g prng of
                None -> None
                Some (p, a) -> run (k a) p)

constant : 'a -> generator 'a
int : generator int                         -- small-biased, full range possible
intBetween : int -> int -> generator int
intAtLeast : int -> generator int
bool : generator bool
bytes : generator bytes                     -- length 0..32
bytesBetween : int -> int -> generator bytes
bytesExactly : int -> generator bytes
option : generator 'a -> generator (option 'a)
listOf : generator 'a -> generator (list 'a)   -- length 0..20
listBetween : int -> int -> generator 'a -> generator (list 'a)
oneOf : cons (generator 'a) -> generator 'a
frequency : list (int, generator 'a) -> generator 'a
suchThat : ('a -> bool) -> generator 'a -> generator 'a       -- gives up after 100 draws
data : generator Data                        -- arbitrary well-formed Data, depth-bounded
tuple2 : generator 'a -> generator 'b -> generator ('a, 'b)
```

Shrinking-friendliness rules for generators, so smaller choices give
smaller values (testing.md "Shrinking"):

- draw sizes before contents (`listOf` draws `choice 1` per element as a
  continue bit, where `0` means stop);
- `intBetween lo hi` is `lo + choice (hi - lo)`; `int` draws a width with
  `choice 2` (0: `choice 255`, 1: 16-bit, 2: 64-bit) so small draws give
  small magnitudes;
- never consume choices on a path that cannot fail differently.

The `prop` desugaring, the `draw`/`run` programs, and the runner protocol
are in testing.md and plans/10.

## `Test`

```elm
module Test exposing (label, assertFailed)

import Builtin

label : string -> unit
label s = Builtin.trace (Builtin.appendString "\u{0}label\u{0}" s) ()

-- Target of the power-assert rewrite: one `\0assert\0` payload line per
-- captured operand, traced in order, then the error (testing.md "Payload").
assertFailed : list string -> 'a
assertFailed msgs =
    case msgs of
        [] -> fail
        m :: rest -> Builtin.trace m (\() -> assertFailed rest) ()
```

`assert` is a keyword with its own expression node (syntax.md); inside a
`tests` block the compiler rewrites it into the power-assert form that
ends in `Test.assertFailed` (plans/10 chunk 3). Test bodies have type
`unit`; the required `do` in a `tests` block is a sequencing block that
desugars to plain `let` (testing.md), so no test monad exists. `fail` /
`fail once` on a `test`/`prop` header and `within (cpu N, mem M)` are
handled by the runner.

## `Ast` and `Derive`

See [macros.md](macros.md). `Ast` holds the macro AST as **little** ADTs
(representation `Term`: `constr` trees with `string`/`int`/`bytes` leaves and
`cons` child lists) plus the builders; `Derive` holds the `derive` macro
and the per-trait derivations. Both are plain Nash with no `Data` or
`Lift` involvement. `Derive` imports `Ast`, `Cons`, `String`; `Ast`
imports `Prelude`, `Cons`, and the trait modules; neither is imported by
anything else in core, so no cycle.

## `Cardano.*` (sketch)

Big ADTs mirroring the Plutus V3 `ScriptContext`. Ledger types are
`Constr` nodes, so they are declared as **constructors with labeled
fields** (`Constr i [fields]`, representation.md), never as record
aliases (which would encode as `List`). Nothing is decoded until a field
is touched, because Big values are `Data` until `lower`ed.

```elm
module Cardano.Tx exposing (..)

import Cardano.Address exposing (Address, Credential)
import Cardano.Value exposing (Value)
import Cardano.Time exposing (Interval)

type ScriptContext = ScriptContext { tx : Tx, redeemer : Data, purpose : ScriptPurpose }

type ScriptPurpose
    = Minting Bytes
    | Spending OutputReference (Option Data)
    | Withdrawing Credential
    | Publishing Int Certificate
    | Voting Voter
    | Proposing Int Proposal

type Tx = Tx
    { inputs : List Input
    , referenceInputs : List Input
    , outputs : List Output
    , fee : Int
    , mint : Value
    , certificates : List Certificate
    , withdrawals : Map Credential Int
    , validityRange : Interval
    , extraSignatories : List Bytes
    , redeemers : Map ScriptPurpose Data
    , datums : Map Bytes Data
    , id : Bytes
    , votes : Map Voter (Map GovernanceActionId Vote)
    , proposals : List Proposal
    , currentTreasury : Option Int
    , treasuryDonation : Option Int
    }

type Input = Input { outputReference : OutputReference, output : Output }
type OutputReference = OutputReference { txId : Bytes, index : Int }
type Output = Output { address : Address, value : Value, datum : Datum, script : Option Bytes }
type Datum = NoDatum | DatumHash Bytes | InlineDatum Data
```

`Cardano.Value` wraps the `value` builtin type (`lookupCoin`, `unionValue`,
`valueContains`, `scaleValue`) with `Lift value Value` via
`valueData`/`unValueData`, where `type alias Value = Map Bytes (Map Bytes Int)`
(a `Map`, which is the ledger encoding). `Cardano.Address` and
`Cardano.Time` (`Interval`, `IntervalBound`) follow the same pattern. Field
order and constructor tags must match the ledger's `Data` encoding
exactly; each is covered by a golden test against a real transaction.

## Tracing and failure

`trace`, `todo`, and `fail` are native expressions and need no import or
Debug module. `trace "message" value` emits its message before evaluating
`value` and returns that value. `todo "message"` fails with a TODO trace;
`fail "message"` fails with the supplied trace. Trace settings control
message emission; silent builds still fail. Failure lowers to UPLC error.

## Open questions

1. **`Prop`/`Test` as default imports.** Elm does not default-import test
   modules. Alternative: `tests` blocks get their own implicit
   `import Prop` / `import Test exposing (assert, label)` only.
2. **`value` builtins** (`InsertCoin` .. `ScaleValue`) exist in
   `nash-plutus` but not in Plutus V3 mainnet; the table types them and
   `nash build` gates them on the target version.
