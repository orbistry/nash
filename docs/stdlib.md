# Standard library: `nash/core`

`nash/core` is a normal Nash package that lives in this repository under
`core/`. It is embedded into the compiler at build time, so `nash check`
works offline and every compiler version ships exactly one core. The
compiler special-cases a small, listed set of its names. Everything else
is ordinary Nash.

Decisions here follow [overview.md](overview.md), the trait hierarchy and
module split in [traits.md](traits.md), the layouts in
[representation.md](representation.md), the builtin constructors in
[kinds.md](kinds.md), and the fuzzer and test statements in
[testing.md](testing.md).

## Layout

```
core/
  nash.jsonc
  src/
    Builtin.nash          synthetic; see "Builtin"
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
    Data.nash             traits ToData/FromData; functions over Data
    Literal.nash          traits FromInt, FromString, FromBytes
    Bool.nash             bool functions; Big Bool
    Unit.nash             Big Unit
    Option.nash           option / Option
    Result.nash           result / Result
    Ordering.nash         ordering / Ordering
    Int.nash              int functions; Big Int
    Bytes.nash            bytes functions; Big Bytes
    String.nash           string (little only; no Big twin)
    List.nash             list functions; Big List
    Cons.nash             cons: Term-kind linked list (elements of any kind)
    Pair.nash             pair
    Array.nash            array
    Map.nash              Map (Big; its little form is `list (pair 'k 'v)`)
    Debug.nash            trace, todo, fail
    Data/Decode.nash      decoders
    Data/Encode.nash      encoders
    Fuzz.nash             fuzzers
    Test.nash             label, assertFailed (`assert` is a keyword)
    Ast.nash              macro AST: little ADTs over cons (see macros.md)
    Derive.nash           @derive
    Cardano/Tx.nash       script context (sketch)
    Cardano/Address.nash
    Cardano/Value.nash
    Cardano/Time.nash
```

Module naming: one module per type pair, named by the uppercase name
(`List`, `Int`, `Bytes`, `Option`, `Result`, `Ordering`, `Map`, `Bool`,
`Unit`). The module declares the Big twin (`Option`), the little twin when
it is not compiler-known (`option`), functions over the **little** type
(`List.map : ('a -> 'b) -> list 'a -> list 'b`), and the `Lift` impl
between the two. Big twins get no function set: only the type and
constructor declarations and their `Lift`/`ToData`/`FromData` impls. Big
values are `lower`ed, worked on, and `lift`ed back. `Map` is the one
module whose functions take the Big type, because its little form
`list (pair 'k 'v)` is not a nominal type. `Data`, `Data.Decode` and
`Data.Encode` are about the `Data` type only. There is no Big `String`:
text in Big positions is `Bytes` holding UTF-8, and `String.nash` is the
little `string` module.

`core/nash.jsonc`:

```jsonc
{
    "type": "package",
    "name": "nash/core",
    "version": "0.1.0",
    "summary": "Nash standard library",
    "license": "Apache-2.0",
    "exposedModules": {
        "Prelude": ["Prelude", "Builtin", "Debug"],
        "Traits": ["Eq", "Ord", "Show", "Num", "Integral", "Semigroup", "Monoid", "Functor", "Applicative", "Monad", "Lift", "Data", "Literal"],
        "Types": ["Bool", "Unit", "Option", "Result", "Ordering", "Int", "Bytes", "String", "List", "Cons", "Pair", "Array", "Map"],
        "Data": ["Data.Decode", "Data.Encode"],
        "Testing": ["Fuzz", "Test"],
        "Macros": ["Ast", "Derive"],
        "Cardano": ["Cardano.Tx", "Cardano.Address", "Cardano.Value", "Cardano.Time"]
    },
    "dependencies": {}
}
```

## Default imports

Every module outside `nash/core` starts with these imports, prepended by
`nash-can` (Elm's `Elm.Compiler.Imports.defaults`,
`elm/compiler/src/Elm/Compiler/Imports.hs`). `exposing (Trait)` exposes
the trait and all its methods (traits.md).

```elm
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
import Data exposing (ToData, FromData)
import Literal exposing (FromInt, FromString, FromBytes)
import Bool exposing (Bool, not, and, or, xor)
import Unit exposing (Unit)
import Option exposing (Option, type option(..))
import Result exposing (Result, type result(..))
import Ordering exposing (Ordering, type ordering(..))
import Cons exposing (type cons(..))
import Derive exposing (derive)
import Debug
import Builtin
import Int
import Bytes
import String
import List
import Pair
import Array
import Map
import Fuzz
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
- Modules inside `nash/core` get no defaults and import explicitly (Elm
  does the same for `elm/core`).
- `Fuzz` and `Test` are default imports so `tests` blocks can use
  `Fuzz.int` and `label`; `Test.label` is additionally exposed unqualified
  inside `tests` blocks only (testing.md). `assert` is a keyword
  (syntax.md), not a `Test` function.

Differences from Elm's list: no `Char`, `Tuple`, `Platform`, `Cmd`, `Sub`.
`(::)` is exposed by `Prelude` rather than `List`.

## Compiler-known types

The `Builtin` module is seeded by the compiler with every builtin type
constructor (kinds.md "Kinds of the builtin constructors", plans/02's
`nash-ast/src/primitives.rs`). They have no Nash declaration and every
one is in scope everywhere:

| Kind | Types |
|---|---|
| `Const` | `int`, `bytes`, `string`, `bool`, `unit`, `bls_g1`, `bls_g2`, `bls_mlr`, `value`, `list 'a`, `array 'a`, `pair 'a 'b` |
| `Big` | `Int`, `Bytes`, `Data`, `List 'a`, `Map 'k 'v` |

`bool` has constructors `False`, `True` (UPLC `bool` constants; `Ctor::Bool`
in `nash-can`). `unit` has `()`. `Data` has `Constr`, `Map`, `List`, `I`,
`B` with the little field types from data.md. There is no Big `String`;
`Bytes` carries UTF-8 text where a Big type is needed (`Ast` names).

Impls for compiler-known types live in the trait's module, because the
orphan rule needs either the trait or the head type to be local and
`Builtin` has no source.

## Prelude

`Prelude` holds only the `infix` table, the non-method functions the
table binds (`|>`, `<|`, `<<`, `>>`, `::` targets, plus
`identity`/`always`), and the prelude impls: impls for tuples, which
count as defined in `nash/core` under the orphan rule. Everything else
lives in the trait modules or the type modules. `Prelude` imports every
trait module and `Bool` (for `&&`/`||`); none of those import `Prelude`.
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
| `Functor` | `list`, `List`, `pair 'k`; each applied element must satisfy its constructor's kind bounds. The pair impl requires resolution of the construction prerequisite below. |
| `Applicative`, `Monad` | No builtin `list` impls: list cannot hold functions required by apply. No impls for Big List. |
| `Lift` | representation.md's table verbatim: `Lift int Int`, `Lift bytes Bytes`, `Lift string Bytes` (UTF-8), `Lift bool Bool`, `Lift unit Unit`, `Lift 'a 'b => Lift (list 'a) (List 'b)`, `Lift (list (pair 'k 'v)) (Map 'k 'v)`, `Big 'a => Lift 'a 'a`; plus `Lift value Value` in `Cardano.Value` |
| `Data` | `ToData`/`FromData` for `Data`, `Int`, `Bytes`, `List 'a`, `Map 'k 'v` |
| `Literal` | `FromInt int`, `FromInt Int`, `FromString string`, `FromString bytes` (UTF-8), `FromBytes bytes`, `FromBytes Bytes` |

Tuple impls (`Eq`, `Ord`, `Show` up to 4) are in `Prelude`. Impls for the
twin types (`option`, `Option`, ...) are in the twin's module.

The shipping hierarchy provides Functor for `list`, `List`, `cons`, `option`
and `result 'e`, and Applicative/Monad for `option` and `result 'e`.
Big List mapping uses an explicitly typed little-list helper between Lift
conversions, keeping the intermediate container unambiguous. Builtin list
mapping can change element kinds within Storable; it cannot produce Term
elements. The required `pair 'k` Functor remains unresolved: `mkPairData`
constructs only `pair Data Data`, not the arbitrary pair needed by `map`.
The `fuzzer` impls require the real Fuzz implementation from plan 10.

### Equality at the Big boundary

Every Big type uses structural Data equality. User Eq impls for Big types
are forbidden; a custom Eq method cannot change equality of a Big value.
The compiler supplies Eq for Big types, including user-defined ADTs and
nominal aliases, through the shared kind/evidence contract.

For builtin `list 'a` with `'a : Big`, the stdlib equality route is
`equalsData (listData left) (listData right)`. This is the specified
semantics, not an optimizer proof about arbitrary Eq bodies. The Const
member of Storable uses element Eq. These cases must be disjoint under
kind-aware coherence; do not retain an overlapping unrestricted list impl.
`listData : list ('a : Big) -> Data` accepts those elements directly, since
they already have the Data representation. Generic `Ord (list 'a)` retains
both `Ord 'a` and `Eq (list 'a)` in its context; an unknown Storable kind
does not select either list equality route during generic type checking.
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
- Data uses `Constr tag [fields]`, `Map [(key, value)]`, `List [values]`,
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

impl Eq (list ('a : Big)) where
    eq a b = Builtin.equalsData (Builtin.listData a) (Builtin.listData b)

impl Eq 'a => Eq (list ('a : Const)) where
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

trait Lift 'small 'big where
    lift : 'small -> 'big
    lower : 'big -> 'small

impl Lift int Int where
    lift = Builtin.castLift
    lower = Builtin.castLower

impl Lift bytes Bytes where
    lift = Builtin.castLift
    lower = Builtin.castLower

-- text: `string` is the little twin of `Bytes` holding UTF-8
impl Lift string Bytes where
    lift s = Builtin.castLift (Builtin.encodeUtf8 s)
    lower b = Builtin.decodeUtf8 (Builtin.castLower b)

-- The reflexive `impl Big 'a => Lift 'a 'a` (identity both ways) is
-- compiler-provided (traits.md "Impl declarations"); it is not written here.

-- walks the list; `lift : list Int -> List Int` picks the reflexive
-- element impl and the optimizer removes the identity map, leaving `listData`
impl Lift 'a 'b => Lift (list 'a) (List 'b) where
    lift xs = wrapList (mapList lift xs)
    lower xs = mapList lower (unwrapList xs)

-- Private bridges preserve the element type across the intrinsic boundary.
wrapList : list 'a -> List 'a
wrapList = Builtin.castLift

unwrapList : List 'a -> list 'a
unwrapList = Builtin.castLower

impl Lift (list (pair 'k 'v)) (Map 'k 'v) where
    lift = Builtin.castLift
    lower = Builtin.castLower
```

This is representation.md's impl table. There is no overlap: `list 'a` is
never Big, so `Lift (list 'a) (List 'b)` and the reflexive impl have
disjoint keys. `Lift bytes Bytes` and `Lift string Bytes` differ in the
first head, so both exist.

```elm
module Data exposing (ToData, FromData, serialise, tag, fields)

import Builtin

trait ToData ('a : Big) where
    toData : 'a -> Data

trait FromData ('a : Big) where
    fromData : Data -> 'a
    validateData : Data -> 'a

-- Every Big value is Data at runtime; the impls give the retag a type.
impl ToData Data where
    toData = Builtin.identity

impl FromData Data where
    fromData = Builtin.identity
    validateData = Builtin.identity

impl ToData Int where
    toData = Builtin.castToData

impl FromData Int where
    fromData = Builtin.castFromDataShallow
    validateData = Builtin.castValidateData

serialise : Data -> bytes
serialise = Builtin.serialiseData

tag : Data -> option int
tag d =
    case d of
        Constr t _ -> Some t
        _ -> None

fields : Data -> option (list Data)
fields d =
    case d of
        Constr _ fs -> Some fs
        _ -> None
```

`Data` fields in patterns are little (`Constr int (list Data)`), as data.md
specifies, so no `lower` is needed on `t` and `fs`.
The Data identity impl is already well typed. Other Big types remain nominally
distinct from Data: their impls require the typed casts specified in codegen.md,
not `Builtin.identity` across different types. `validateData` returns the
validated value and traps on invalid structure; Data.Decode provides the
non-failing decoder API.

## Twin modules

```elm
module Option exposing (Option(..), type option(..), withDefault, map, map2, andThen, isSome, unwrap, toResult)

import Prelude exposing (..)
import Functor exposing (Functor)
import Applicative exposing (Applicative)
import Monad exposing (Monad)
import Eq exposing (Eq)
import Lift exposing (Lift)
import Debug

type option 'a = Some 'a | None
type Option 'a = Some 'a | None

withDefault : 'a -> option 'a -> 'a
withDefault d m =
    case m of
        Some x -> x
        None -> d

unwrap : option 'a -> 'a
unwrap m =
    case m of
        Some x -> x
        None -> Debug.fail "Option.unwrap: None"

impl Functor option where
    map f m =
        case m of
            Some x -> Some (f x)
            None -> None

impl Applicative option where
    pure = Some
    apply mf m =
        case mf of
            Some f -> map f m
            None -> None

impl Monad option where
    bind m f =
        case m of
            Some x -> f x
            None -> None

impl Eq 'a => Eq (option 'a) where
    eq a b =
        case (a, b) of
            (Some x, Some y) -> x == y
            (None, None) -> True
            _ -> False

impl Lift 'a 'b => Lift (option 'a) (Option 'b) where
    lift m =
        case m of
            Some x -> Option.Some (lift x)
            None -> Option.None

    lower m =
        case m of
            Option.Some x -> Some (lower x)
            Option.None -> None
```

`Result` and `Ordering` follow the same shape (`Result.mapError`,
`Result.withDefault`, `Ordering.invert`, `Ordering.then_`). `Bool` adds
the `bool` functions (below); `Unit` declares only the Big twin. Their
`Lift` impls are in `Lift.nash` (representation.md's table).

## Compiler special cases

| Name | Treatment |
|---|---|
| `Builtin.bool`, `False`, `True` | `if` scrutinee type; UPLC `bool` constants; `Ctor::Bool` in `nash-can` (`crates/nash-can/src/environment/foreign.rs`, `make_union_ctor`) |
| `Builtin.unit`, `()` | UPLC `unit` constant |
| `Builtin.Data` and its constructors | pattern-matchable Big type (data.md) |
| `Builtin.list`, `[..]`, `::` patterns | list literals and patterns; element kind `Storable` |
| `Bool.and`, `Bool.or` | second argument delayed (`&&`, `||` are lazy) |
| `Literal.FromInt`, `FromString`, `FromBytes` | literal desugaring and defaulting to `int`, `string`, `bytes` |
| `Eq.Eq` | literal patterns |
| `Monad.Monad` | `do` desugaring target |
| `Show.Show` | power-assert rendering of operands |
| `Builtin.*` | direct UPLC builtin nodes (below) |
| `Debug.trace`, `Debug.todo`, `Debug.fail` | trace levels, compiler-generated traces switch |
| `assert` keyword, `Test.assertFailed` | power-assert rewrite in `tests` blocks traces the operands and calls `Test.assertFailed`; elsewhere `assert e` is `if e then () else fail` (testing.md) |
| `Fuzz.fuzzer`, `Fuzz.Prng` | `prop`/`via` desugaring and the runner protocol (`draw`/`run` programs, plans/10 chunk 4) |
| `Ast.*`, `Cons.cons` | reified by `nash-macro` as `Term::Constr` trees by constructor index and walked back after evaluation (macros.md); the compiler knows the tag table, the Nash side is plain little ADTs |
| `Derive.derive` | nothing special beyond being a macro; listed because default imports expose it |

## Builtin

`Builtin` is a synthetic module: it has no `.nash` source. Its interface
is generated from the Rust tables in `crates/nash-ast/src/primitives.rs`:
`PRIMITIVES` (the compiler-known types, plans/02 chunk 3) and `BUILTINS`,
which maps each `DefaultFunction` variant by its symbolic Rust name
(`crates/nash-plutus/src/builtin/default_function.rs`) to a Nash name and
type, plus the type constructors from plans/02's primitives table. The typed
value table is in `primitives/builtins.rs`, re-exported by `primitives.rs`.
The canonicalizer derives value-kind bounds from those types; the backend
resolves the symbolic variant without making the AST depend on the runtime.
The `unit` spelling and `()` both canonicalize to the same unit type,
including in impl heads.
`nash-can` resolves `Builtin.foo` to `VarForeign { home: Builtin }` and
`nash-codegen` lowers that to `Core::Builtin` (plans/07 chunk 4),
applying the variant's `force_count()` forces. `nash docs` renders the
same table.

Rules:

- Every builtin is strict in every argument, including `ifThenElse`,
  `chooseList`, `chooseData`, and `trace`. `if`, `case`, `&&`, `||` are
  compiled by the compiler with delays; `Builtin.ifThenElse` is the raw
  strict builtin.
- Type variables are kind-restricted as UPLC requires: elements of
  `list`/`array` and components of `pair` are `Storable`; `'a` in
  `ifThenElse`, `chooseUnit`, `chooseList`, `chooseData`, `trace` is `Any`.
- `Builtin.identity : 'a -> 'a` and `Builtin.error : unit -> 'a` lower to
  nothing and to the UPLC `error` term.
- `Builtin.castToData`, `castFromDataShallow`, `castValidateData`, `castLift`,
  and `castLower` each have scheme `forall a b. a -> b`. Only the exact
  `nash/core` package can import these intrinsics, through any import route.
  Their symbolic operations are retained for typed Cast lowering in Plan 07;
  they are not UPLC DefaultFunction entries. Plan 03 supplies the frontend
  bindings needed by the core impl bodies; code generation and validation
  checkers remain Plan 07 work. These bindings do not make distinct nominal
  types unify.

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
| `MapData` | `mapData` | `list (pair Data Data) -> Data` |
| `ListData` | `listData` | `list ('a : Big) -> Data` |
| `IData` | `iData` | `int -> Data` |
| `BData` | `bData` | `bytes -> Data` |
| `UnConstrData` | `unConstrData` | `Data -> pair int (list Data)` |
| `UnMapData` | `unMapData` | `Data -> list (pair Data Data)` |
| `UnListData` | `unListData` | `Data -> list Data` |
| `UnIData` | `unIData` | `Data -> int` |
| `UnBData` | `unBData` | `Data -> bytes` |
| `EqualsData` | `equalsData` | `Data -> Data -> bool` |
| `SerialiseData` | `serialiseData` | `Data -> bytes` |
| `MkPairData` | `mkPairData` | `Data -> Data -> pair Data Data` |
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
| (none) | `identity` | `'a -> 'a` |
| (none) | `error` | `unit -> 'a` |

Availability by Plutus version is not modelled in the type; `nash build`
rejects programs that use builtins newer than the target's version
(overview: target is V3 with the latest builtins).

## Little-type modules

### `Bool`

```elm
module Bool exposing (Bool, not, and, or, xor)

import Builtin

type Bool = False | True

not : bool -> bool
not b = if b then Builtin.False else Builtin.True

-- special-cased: lazy in the second argument
and : bool -> bool -> bool
and a b = if a then b else Builtin.False

or : bool -> bool -> bool
or a b = if a then Builtin.True else b

xor : bool -> bool -> bool
xor a b = if a then not b else b
```

### `Int`

```elm
module Int exposing (..)

abs : int -> int
pow : int -> int -> int
powMod : int -> int -> int -> int          -- expModInteger
toBytes : bool -> int -> int -> bytes      -- integerToByteString bigEndian size n
fromBytes : bool -> bytes -> int
toString : int -> string
```

### `Bytes`

```elm
module Bytes exposing (..)

length : bytes -> int
at : bytes -> int -> int
slice : int -> int -> bytes -> bytes
take, drop : int -> bytes -> bytes
concat : list bytes -> bytes
sha2_256, sha3_256, blake2b_256, blake2b_224, keccak_256, ripemd_160 : bytes -> bytes
and, or, xor : bool -> bytes -> bytes -> bytes
complement : bytes -> bytes
readBit : bytes -> int -> bool
writeBits : bytes -> list int -> bool -> bytes
shift, rotate : bytes -> int -> bytes
countSetBits, findFirstSetBit : bytes -> int
toHex : bytes -> string
```

### `String`

```elm
module String exposing (..)

toBytes : string -> bytes
fromBytes : bytes -> string          -- errors on invalid UTF-8
fromInt : int -> string
concat : list string -> string
join : string -> list string -> string
```

### `List`

Over `list 'a`, built on `nullList`/`headList`/`tailList`/`mkCons`.

```elm
module List exposing (..)

singleton : 'a -> list 'a
repeat : int -> 'a -> list 'a
range : int -> int -> list int
head : list 'a -> option 'a
tail : list 'a -> option (list 'a)
isEmpty : list 'a -> bool
length : list 'a -> int
reverse : list 'a -> list 'a
append : list 'a -> list 'a -> list 'a
concat : list (list 'a) -> list 'a
map : ('a -> 'b) -> list 'a -> list 'b
indexedMap : (int -> 'a -> 'b) -> list 'a -> list 'b
filter : ('a -> bool) -> list 'a -> list 'a
filterMap : ('a -> option 'b) -> list 'a -> list 'b
foldl : ('a -> 'b -> 'b) -> 'b -> list 'a -> 'b
foldr : ('a -> 'b -> 'b) -> 'b -> list 'a -> 'b
any, all : ('a -> bool) -> list 'a -> bool
find : ('a -> bool) -> list 'a -> option 'a
member : Eq 'a => 'a -> list 'a -> bool
take, drop : int -> list 'a -> list 'a
at : int -> list 'a -> option 'a
map2 : ('a -> 'b -> 'c) -> list 'a -> list 'b -> list 'c
sort : Ord 'a => list 'a -> list 'a
sortBy : ('a -> 'a -> ordering) -> list 'a -> list 'a
sum : Num 'a => list 'a -> 'a
partition : ('a -> bool) -> list 'a -> (list 'a, list 'a)
toArray : list 'a -> array 'a
```

`List 'a` has no functions of its own: `lower` it (one `unListData`),
work on the `list`, `lift` the result. `List.map = Functor.map` at `list`.

`zip` returns a `list` of tuples, which are `Term` elements and therefore
illegal for `list` (elements must be `Storable`). `zip`/`unzip` are
omitted; `map2` covers the common case. A list of tuples (or of any
other `Term` value) is a `cons`, below.

### `Cons`

A Term-kind linked list for elements `list` cannot hold (functions,
tuples, little ADTs such as `Ast` nodes). Each cell is a UPLC `constr`
(`Nil` = `constr 0 []`, `Cons x xs` = `constr 1 [x, xs]`), so elements
may be of any kind. It is the list type of the macro `Ast` family
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
fromList : list 'a -> cons 'a                     -- 'a : Storable, from `list`
toList : cons 'a -> list 'a                       -- 'a : Storable, from `list`
```

`fromList`/`toList` carry the `Storable` kind bound that `list 'a`
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
snd : pair 'a 'b -> 'b
make : Data -> Data -> pair Data Data      -- only Data pairs can be built

module Array exposing (..)
fromList : list 'a -> array 'a
length : array 'a -> int
at : array 'a -> int -> 'a                 -- errors out of bounds
get : array 'a -> int -> option 'a
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

Structured, failing decoders for untrusted `Data`. `FromData.validateData`
for derived types is built from these.

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

`decoder` is a little alias of a function type (`Term` kind), so it has no
`Functor`/`Monad` impls (impls attach to nominal types); v1 uses
`andThen`. `Data.Encode` is the inverse: `int : int -> Data`, `bytes`,
`string`, `bool`, `list : ('a -> Data) -> list 'a -> Data`,
`constr : int -> list Data -> Data`, `map`.

## `Fuzz`

Exactly testing.md "Fuzzers": `Prng` is Big, choices are `Int`s, the
primitive is `choice`.

```elm
module Fuzz exposing (..)

import Prelude exposing (..)
import Builtin
import Functor exposing (Functor)
import Applicative exposing (Applicative)
import Monad exposing (Monad)
import Lift exposing (Lift)
import List

type Prng = Seeded Bytes (List Int) | Replayed Int (List Int)

type fuzzer 'a = Fuzzer (Prng -> option (Prng, 'a))

run : fuzzer 'a -> Prng -> option (Prng, 'a)
run (Fuzzer f) = f

-- Draw an integer in [0, bound]. The only primitive.
choice : int -> fuzzer int
choice bound =
    Fuzzer
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

impl Functor fuzzer where
    map f (Fuzzer g) =
        Fuzzer (\prng ->
            case g prng of
                None -> None
                Some (p, a) -> Some (p, f a))

impl Applicative fuzzer where
    pure a = Fuzzer (\prng -> Some (prng, a))
    apply ff fa = bind ff (\f -> map f fa)

impl Monad fuzzer where
    bind (Fuzzer g) k =
        Fuzzer (\prng ->
            case g prng of
                None -> None
                Some (p, a) -> run (k a) p)

constant : 'a -> fuzzer 'a
int : fuzzer int                         -- small-biased, full range possible
intBetween : int -> int -> fuzzer int
intAtLeast : int -> fuzzer int
bool : fuzzer bool
bytes : fuzzer bytes                     -- length 0..32
bytesBetween : int -> int -> fuzzer bytes
bytesExactly : int -> fuzzer bytes
option : fuzzer 'a -> fuzzer (option 'a)
listOf : fuzzer 'a -> fuzzer (list 'a)   -- length 0..20
listBetween : int -> int -> fuzzer 'a -> fuzzer (list 'a)
oneOf : list (fuzzer 'a) -> fuzzer 'a
frequency : list (int, fuzzer 'a) -> fuzzer 'a
suchThat : ('a -> bool) -> fuzzer 'a -> fuzzer 'a       -- gives up after 100 draws
data : fuzzer Data                        -- arbitrary well-formed Data, depth-bounded
tuple2 : fuzzer 'a -> fuzzer 'b -> fuzzer ('a, 'b)
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
        [] -> Builtin.error ()
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
(kind `Term`: `constr` trees with `string`/`int`/`bytes` leaves and
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

## `Debug`

```elm
module Debug exposing (trace, todo, fail)

trace : string -> 'a -> 'a     -- subject to trace level (silent / compact / verbose)
trace = Builtin.trace

fail : string -> 'a            -- traces msg and errors
fail msg = Builtin.trace msg (Builtin.error ())

todo : string -> 'a            -- traces "TODO: msg" and errors; warning at compile time
todo msg = fail (Builtin.appendString "TODO: " msg)
```

`fail`, `todo`, `trace` are keywords with their own expression nodes
(syntax.md) that lower to these functions, so they need no import.

## Open questions

1. **`Fuzz`/`Test` as default imports.** Elm does not default-import test
   modules. Alternative: `tests` blocks get their own implicit
   `import Fuzz` / `import Test exposing (assert, label)` only.
2. **`value` builtins** (`InsertCoin` .. `ScaleValue`) exist in
   `nash-plutus` but not in Plutus V3 mainnet; the table types them and
   `nash build` gates them on the target version.
