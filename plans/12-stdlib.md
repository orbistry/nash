# Plan 12: `nash/base` standard library

Goal: write `crates/nash-driver/base/` in Nash per [docs/stdlib.md](../docs/stdlib.md) and
wire it into the compiler: embedded package, default imports, the
synthetic `Builtin` module, the trait modules and twin types, the type
modules, decoders, `Fuzz`, `Test`, `Ast`, `Cardano.*`.

Prerequisites, by chunk (each chunk's Nash must type-check with the
compiler features available when it lands):

| Chunk | Needs |
|---|---|
| 1 skeleton, 2 default imports, 3 `Builtin` | nothing beyond today's pipeline |
| 4 kinds-aware twin types | plans/02 (kinds) |
| 5 trait modules, operators, `Lift`, `ToData`, `FromData`, `Validate` | plans/03 (traits; chunk 12 there is this chunk's file list) |
| 6 type modules, 7 `Data`/`Map` modules | plans/03; `Data` patterns from data.md |
| 8 `Fuzz`, 9 `Test` | plans/03, plans/10 (tests block, sequencing `do`, `Prng` protocol, runner) |
| 10 `Ast`, `Derive` | plans/11 chunk 4 (tags) and chunk 10 |
| 11 `Cardano.*` | chunk 7 |

Module layout is docs/stdlib.md "Layout": one file per trait, one module
per type pair named by the uppercase name, functions on the little twin
only. `Prelude` is the `infix` table, its helper functions, and the tuple
impls. There is no Big `String` and no `Data.List`-style module family.

Crates touched: `nash-can`, `nash-driver`, `nash-cli`,
`nash-codegen` (builtin lowering), `crates/nash-driver/base/`.

References:

- Elm: `elm/compiler/src/Elm/Compiler/Imports.hs` (`defaults`),
  `Canonicalize/Environment/Foreign.hs` (`createInitialEnv`),
  `Elm/Kernel.hs` (how Elm binds native code; we bind builtins by table
  instead).
- Aiken: `crates/aiken-lang/src/builtins.rs` (`from_default_function`,
  `prelude`, the builtin type table), `crates/aiken-project/src/lib.rs`
  (stdlib is a normal dependency; we embed instead), Aiken stdlib
  `aiken-lang/stdlib` for API shape (`list`, `option`, `cbor`, `fuzz`).
- Current code: `crates/nash-can/src/environment/foreign.rs:18`
  (`create_initial_env`, the `List` pre-seed at :34),
  `crates/nash-driver/src/project.rs:70` (`discover_modules`),
  `crates/nash-driver/src/source.rs:224` (`OverlaySource`),
  `crates/nash-driver/src/compile.rs:86` (`build_sync` interface map),
  `crates/nash-plutus/src/builtin/default_function.rs`,
  `SPEC.md` and `nash-can/src/defaults.rs`.

Conventions: type variables `'a`; lowercase bare type names are little,
uppercase Big.

---

Plan 10 implements the minimum `Fuzz` and `Test` modules required by its runner.
Chunks 8 and 9 here extend and verify those modules; they must not duplicate or
replace the tested PRNG, replay, label, and assertion protocols.

## Chunk 1: package skeleton and embedding — complete

- [x] Store the foundation Nash sources in `crates/nash-driver/base/src/`.
- [x] Embed them through `nash-driver/build.rs`; no separate Rust crate or Nash package manifest.
- [x] Discover bundled modules automatically, with stable `nash-base:///Module.nash` URIs and compiler-owned `nash/base` identity.
- [x] Read sources offline, independently of checkout and current directory; reject virtual-source writes.
- [x] Reserve bundled module names and package identity against application/dependency replacement.

Base changes ship with a compiler release. Applications do not list Base as
a dependency or install its sources. The language server uses the same driver
source provider.

## Chunk 2: default imports — complete for shipped modules

- [x] Share `nash-can::defaults::MODULES` between dependency discovery, canonical scope, and diagnostic localization.
- [x] Expose Prelude operators/helpers, traits and methods, primitive names, and little constructors in application modules.
- [x] Keep Big twin constructors qualified; make `Test.label` unqualified only in tests blocks.
- [x] Preserve original source imports; formatting must never serialize implicit imports.
- [x] Compile all bundled modules and an application without imports or dependencies through the in-process driver.

Base modules import explicitly to avoid bootstrap cycles. Extend the catalog
when later chunks add new modules; do not install placeholder interfaces.

## Chunk 3: synthetic `Primitive` and `Builtin` — complete

- [x] `Primitive` owns compiler-known types, constructors, representation traits, and unchecked `coerce`.
- [x] `Builtin` owns only actual Plutus Core builtin functions from `BUILTINS`.
- [x] Builtin signatures use primitive type identities; codegen dispatches `Primitive.coerce` separately.
- [x] Test the strict Builtin inventory and rejection of `Builtin.coerce`.

`Primitive.coerce` is runtime identity with independently polymorphic input
and output. It checks neither representation nor shape. The normal Nash
blanket impls of `ToData` and `FromData` use it; `Validate` remains opt-in.

## Chunk 4: compiler-known types and the twin type modules

**Files**

- `crates/nash-driver/base/src/Bool.nash`, `Unit.nash`, `Option.nash`, `Result.nash`, `Ordering.nash` (new; type declarations only, functions in chunk 6, impls in chunk 5)
- `crates/nash-can/src/environment/foreign.rs` (`make_union_ctor` special case for `Primitive.bool`; the `List` pre-seed at `foreign.rs:34` is gone once plans/02 chunk 3 seeds from `PRIMITIVES`)
- `crates/nash-can/src/environment/defaults.rs` (add the five modules)

**Change**

With kinds (plans/02) available: every compiler-known type comes from
`nash_ast::primitives::PRIMITIVES` with home `Builtin` (docs/stdlib.md
"Compiler-known types"); this chunk adds no second table. Declare the
twins in their own modules, `type Bool = False | True`
in `Bool.nash`, `type Unit = Unit` in `Unit.nash`,
`type option 'a = Some 'a | None` and `type Option 'a = Some 'a | None` in
`Option.nash`, and likewise `Result`, `Ordering` (representation.md
"Prelude twins", constructor order is load-bearing); move the
`Basics.Bool` special case (`foreign.rs:349`) to `Primitive.bool`.

**Code**

`crates/nash-driver/base/src/Option.nash` (chunk 4 version):

```elm
module Option exposing (Option(..), type option(..))

type option 'a = Some 'a | None
type Option 'a = Some 'a | None
```

Inside `Option.nash` both constructor sets are in scope unqualified, so the
module's own code (chunk 6) writes `Option.Some` for the Big one, exactly
as users do; a bare `Some` inside the module is the little constructor.
Canonicalization treats a module's own Big twin constructors as
qualified-only when a little constructor of the same name is declared in
the same module (`Env.ctors` keeps the little one, `Env.q_ctors` the Big
one). User modules may not declare two constructors with one name; only
`nash/base` twin modules may, and only for a little/Big pair
(`Context.package` check).

The type seed is plans/02 chunk 3's (`primitives::PRIMITIVES`, homed by
`primitives::builtin_home()`, `Data`/`Int`/`Bytes`/`List`/`Map` Big and
`int`/`bytes`/`string`/`bool`/`unit`/`list`/`pair`/`array`/`bls_*`/`value`
Const). What this chunk adds to that seed: `bool` gets constructors
`False`, `True`, `unit` gets `()`, and `Data` gets `Constr`, `Map`, `List`,
`I`, `B` with the little field types from data.md. `foreign.rs`
`make_union_ctor`:

```rust
if home.name == "Builtin" && union_name == "bool" {
    return Ctor::Bool { home, union: can_union, index: ctor.index };
}
```

Big `Bool` is an ordinary union (`Constr 0`/`Constr 1`).

**Elm/Aiken reference**

`Canonicalize/Environment/Foreign.hs` `toCtor` (Bool special case);
`Elm/Compiler/Type/Extract.hs` has nothing to port here. Aiken
`builtins.rs` `prelude` (how `Option`, `Ordering`, `Bool` are declared as
compiler-known ADTs).

**Tests**

- `core` compiles (the in-process Base compilation tests via the driver test `test_core_compiles`, kept green from here on).
- nash-can: `if` on `bool` uses `Ctor::Bool`; `type t = A | B` little and `type T = A | B` Big in one user module is a dup-ctor error; `Some 1` and `Option.Some (lift 1)` from the defaults resolve to the little and the Big constructor.
- kinds: `list (option int)` has kind `Type` but fails `Storable` storage formation; `list Int` and `list int` are fine.

**Done when** `test_core_compiles` passes and the little/Big pairs are
usable side by side in one user module.

---

## Chunk 5: trait modules, operators, `Lift`, `ToData`, `FromData`, `Validate`

**Files**

- `crates/nash-driver/base/src/Eq.nash`, `Ord.nash`, `Show.nash`, `Num.nash`, `Integral.nash`, `Semigroup.nash`, `Monoid.nash`, `Functor.nash`, `Applicative.nash`, `Monad.nash`, `Lift.nash`, `Data.nash`, `Literal.nash` (new; the file list of plans/03 chunk 12)
- `crates/nash-driver/base/src/Prelude.nash` (the `infix` table and the tuple impls)
- `crates/nash-driver/base/src/Bool.nash` (`not`, `and`, `or`, `xor`)
- `crates/nash-driver/base/src/Debug.nash` (new)
- `crates/nash-driver/base/src/Option.nash`, `Result.nash`, `Ordering.nash` (their `Eq`/`Functor`/`Applicative`/`Monad`/`Lift` impls)
- `crates/nash-can/src/environment/foreign.rs` (lazy `and`/`or` special case marker)
- `crates/nash-codegen/src/special.rs` (plans/07: `Bool.and`/`or` delay the second argument)

**Change**

Write each trait of traits.md "Core trait hierarchy" in its own module
with the impls for compiler-known types listed in docs/stdlib.md "Trait
modules"; `Lift.nash` holds representation.md's impl table (the reflexive
`Big 'a => Lift 'a 'a` is compiler-provided and not written); `Prelude`
gets the `infix` table, the operator helper functions, and the tuple
impls; `Bool` gets the `bool` functions; `Debug` gets `trace`, `todo`,
`fail` over `Builtin.trace` and `error`. Impls for the twin types go in
the twin's module.

**Code**

`crates/nash-driver/base/src/Debug.nash`:

```elm
module Debug exposing (trace, todo, failWith)

import Builtin

trace : string -> 'a -> 'a
trace = Builtin.trace

failWith : string -> 'a
failWith msg =
    trace msg
    fail

todo : string -> 'a
todo msg = failWith (Builtin.appendString "TODO: " msg)
```

Failure uses Nash syntax, and identity is an ordinary Nash function. The
Builtin table contains only actual UPLC DefaultFunction operations.

Import order that type-checks (no module imports `Prelude`; each trait
module imports only `Builtin` and its superclass module):

```
Builtin
  └─ Eq ─ Ord          Show      Num ─ Integral      Semigroup ─ Monoid
     Functor ─ Applicative ─ Monad
     Lift             Data (ToData/FromData/Validate)          Literal
Bool, Unit                              (types only; `Bool` functions use `if`)
Prelude                                 (imports every trait module and Bool)
Option, Result, Ordering                (import Prelude and the trait modules they impl)
List, Int, Bytes, String, Map, ...      (import Prelude for operators; chunks 6–7)
```

`crates/nash-driver/base/src/Functor.nash` carries `impl Functor list` with a local
recursive `mapList`; chunk 6's `List.map` is `Functor.map` specialized at
`list`, so `List` imports `Functor`, never the other way round.

`crates/nash-driver/base/src/Prelude.nash` is docs/stdlib.md "Prelude" verbatim: the `infix`
block (`infix non 4 (==) = eq`, `infix left 6 (+) = add`,
`infix left 7 (/) = div`, `infix right 5 (::) = prepend`, ...), `identity`,
`always`, `applyForward`, `applyBackward`, `composeLeft`, `composeRight`,
`prepend = Builtin.mkCons` (kept here because `List` imports `Prelude`;
named `prepend` because `Cons` is the `Cons` module's constructor), and
`impl (Eq 'a, Eq 'b) => Eq ('a, 'b)` and friends up to 4-tuples for
`Eq`, `Ord`, `Show` (tuples count as defined in `nash/base` for the orphan
rule).

Lazy `and`/`or`: codegen recognizes `VarForeign { home: Bool, name: "and" | "or" }`
in call position with two arguments and emits `if a then b else False`
directly (the `if` is already lazy). Partial applications of `and` fall
back to the strict function.

**Elm/Aiken reference**

Elm `crates/nash-driver/base/src/Basics.elm` for the operator table, precedences, and
`&&`/`||` (Elm's compiler special-cases them in `Optimize/Expression.hs`).
Aiken `builtins.rs` `prelude` for `Ordering`, `Option`, and the
`ToData`-like `Data` conversions (`builtins::data`).

**Tests**

- `crates/nash-driver/base/src/Prelude.nash` `tests` block (runs after plans/10): `1 + 2 == 3`, `compare 1 2 == LT`, `lift 1 == (1 : Int)`, `lower (lift "a" : Bytes) == "a"`, `Some 1 == Some 1` and `Option.Some (lift 1) == Option.Some (lift 1)`, `[1,2] ++ [3] == [1,2,3]`, `fail` raises (`test "fail fails" fail = do fail "x"`).
- nash-can/nash-solve: `1 + 2` resolves to `Num int`; `(1 : Int) + 2` resolves `Num Int` with the literal at `Int` via `FromInt Int`; `lift [1, 2] : List Int` resolves `Lift (list int) (List Int)` through `Lift int Int`; `lift ([] : list Int) : List Int` resolves the element through the reflexive impl.
- codegen: `False && fail "x"` evaluates to `False` (laziness).

**Done when** the in-process Base compilation tests and the user-facing operator tests pass.

---

## Chunk 6: type modules

**Files**

- `crates/nash-driver/base/src/Int.nash`, `Bytes.nash`, `String.nash`, `List.nash`, `Cons.nash`, `Pair.nash`, `Array.nash`, `Option.nash`, `Result.nash`, `Ordering.nash`, `Bool.nash`

**Change**

Write the APIs listed in docs/stdlib.md "Little-type modules" and "Twin
modules". Every function takes the little twin (`list 'a`, `option 'a`,
`int`, ...); the Big twins (`List 'a`, `Int`, `Option 'a`, ...) get no
functions, only the impls from chunk 5. `String.nash` is the little
`string` module; there is no Big `String`. `Cons.nash` is the Term-representation
linked list `type cons 'a = Nil | Cons 'a (cons 'a)` (docs/stdlib.md
"`Cons`") that chunk 10's `Ast` and plans/11 depend on; its `fromList`
and `toList` carry the `Storable` bound of `list`. All functions are
total unless documented (`Array.at`, `Option.unwrap`).

**Code** (`crates/nash-driver/base/src/List.nash` excerpt, the shape everything else follows)

```elm
module List exposing (..)

import Prelude exposing (..)
import Builtin exposing (chooseList, headList, tailList, nullList)
import Functor

map : ('a -> 'b) -> list 'a -> list 'b
map = Functor.map

foldr : ('a -> 'b -> 'b) -> 'b -> list 'a -> 'b
foldr f acc xs =
    chooseList xs acc (f (headList xs) (foldr f acc (tailList xs)))

foldl : ('a -> 'b -> 'b) -> 'b -> list 'a -> 'b
foldl f acc xs =
    chooseList xs acc (foldl f (f (headList xs) acc) (tailList xs))

length : list 'a -> int
length = foldl (\_ n -> n + 1) 0

filter : ('a -> bool) -> list 'a -> list 'a
filter p =
    foldr (\x acc -> if p x then x :: acc else acc) []

member : Eq 'a => 'a -> list 'a -> bool
member x = any (\y -> x == y)

sortBy : ('a -> 'a -> ordering) -> list 'a -> list 'a
sortBy cmp xs =
    case xs of
        [] -> []
        pivot :: rest ->
            let
                (smaller, larger) = partition (\y -> cmp y pivot == LT) rest
            in
            sortBy cmp smaller ++ (pivot :: sortBy cmp larger)
```

`chooseList xs acc (...)` is strict in its branches; the recursive calls
are guarded because `chooseList`'s builtin type in the table is strict but
codegen wraps `chooseList` branches in delays when both branches are
present (plans/07 special case, same mechanism as `and`/`or`). Until that
lands, write `foldr` with `if nullList xs then acc else ...`; the `if` is
lazy today. This chunk uses the `if` form; plans/07 may rewrite.

**Elm/Aiken reference**

Elm `crates/nash-driver/base/src/List.elm` for names and argument order. Aiken
`stdlib/lib/aiken/collection/list.ak` for what is worth having on-chain
(no `zip` on `list`, `at` returns `option`).

**Tests**

Each module has a `tests` block: `List.reverse [1,2,3] == [3,2,1]`,
`List.sort [3,1,2] == [1,2,3]`, `Bytes.slice 1 2 "abcd" == "bc"`,
`Int.pow 2 10 == 1024`, `Option.withDefault 0 None == 0`, and one `prop`
per module (`List.reverse (List.reverse xs) == xs` with `xs via listOf int`
once chunk 8 lands; before that the `prop`s are written but `nash test`
only runs `test`s).

**Done when** the in-process Base compilation tests passes and the `test`s pass under
the in-process Base test runner.

---

## Chunk 7: `Data` and `Map` modules

**Files**

- `crates/nash-driver/base/src/Data.nash` (`serialise`, `tag`, `fields` added to chunk 5's traits), `Data/Decode.nash`, `Data/Encode.nash`, `Map.nash`

**Change**

Functions over the `Data` type, the decoder/encoder combinators, and
`Map` (the one module whose functions take a Big type, because its little
form `list (pair 'k 'v)` is not nominal). `Data`, `Data.Decode` and
`Data.Encode` are about `Data` only; there are no `Data.List`-style Big
counterparts of the type modules. Needs `Data` patterns (data.md).

**Code** (`crates/nash-driver/base/src/Data/Decode.nash` excerpt)

```elm
module Data.Decode exposing (..)

import Builtin
import List

type alias decoder 'a = Data -> option 'a

int : decoder int
int d =
    case d of
        I n -> Some (lower n)
        _ -> None

bytes : decoder bytes
bytes d =
    case d of
        B b -> Some (lower b)
        _ -> None

list : decoder 'a -> decoder (list 'a)
list item d =
    case d of
        List xs -> traverse item (lower xs)
        _ -> None

constr : int -> decoder 'a -> decoder 'a
constr tag inner d =
    case d of
        Constr t fields -> if lower t == tag then inner d else None
        _ -> None

field : int -> decoder 'a -> decoder 'a
field i inner d =
    case d of
        Constr _ fields ->
            case List.at i (lower fields) of
                Some f -> inner f
                None -> None
        _ -> None

andThen : ('a -> decoder 'b) -> decoder 'a -> decoder 'b
andThen f dec d =
    case dec d of
        Some a -> f a d
        None -> None

traverse : decoder 'a -> list Data -> option (list 'a)
traverse dec xs =
    List.foldr
        (\x acc ->
            case (dec x, acc) of
                (Some a, Some rest) -> Some (a :: rest)
                _ -> None)
        (Some [])
        xs
```

`crates/nash-driver/base/src/Map.nash` works on `Map 'k 'v` through `lower`/`lift`
(`Lift (list (pair 'k 'v)) (Map 'k 'v)`, one `unMapData`/`mapData` each):

```elm
module Map exposing (..)

import Prelude exposing (..)
import Builtin
import Eq exposing (Eq)
import Lift exposing (Lift)
import List
import Option
import Pair

get : Eq 'k => 'k -> Map 'k 'v -> option 'v
get k m =
    Option.map Pair.snd (List.find (\p -> Pair.fst p == k) (lower m))
```

`lower xs : list Int` on a `List Int` goes through
`Lift 'a 'b => Lift (list 'a) (List 'b)` with the reflexive element impl
and costs one `unListData` after the optimizer drops the identity map.

**Elm/Aiken reference**

Elm `crates/nash-driver/base/src/Dict.elm` for `Map` API names (insert/get/remove/keys/values).
Aiken `stdlib/lib/aiken/collection/dict.ak` (association list semantics),
`stdlib/lib/aiken/cbor.ak` for diagnostics. Elm `Json.Decode` for the
combinator shapes (`field`, `andThen`, `oneOf`, `succeed`, `fail`).

**Tests**

`tests` blocks: `Decode.run (Decode.constr 0 (Decode.field 0 Decode.int)) (Encode.constr 0 [Encode.int 5]) == Some 5`;
`Decode.run Decode.int (Encode.bytes "x") == None`;
`Map.get (lift 1) (Map.fromList [Pair.make (lift 1) (lift "a")]) == Some (lift "a")`;
a `prop` that `Encode` then `Decode` is identity for `int`, `bytes`, `list int`.

**Done when** the in-process Base compilation tests passes and the decode tests pass.

---

## Chunk 8: `Fuzz`

**Files**

- `crates/nash-driver/base/src/Fuzz.nash`
- `crates/nash-test/src/prng.rs` (plans/10 chunk 5: `Prng::from_seed`, `from_choices`, `to_data`, `from_data`)

**Change**

docs/testing.md "Fuzzers", verbatim: the **Big** `Prng` ADT that the
runner builds as `PlutusData`, the **little** `fuzzer 'a` wrapper with
`Functor`/`Applicative`/`Monad` impls, `choice` as the single primitive
over `u64` integer choices (not Aiken's bytes), and the generators listed
in docs/stdlib.md "`Fuzz`" built on `choice`.

**Code** (`crates/nash-driver/base/src/Fuzz.nash` excerpt; the type and impl definitions are
docs/testing.md's)

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
constant = pure

intBetween : int -> int -> fuzzer int
intBetween lo hi =
    if hi <= lo then constant lo else map (\n -> lo + n) (choice (hi - lo))

-- width first so small choices give small magnitudes (testing.md "Shrinking")
int : fuzzer int
int =
    do
        width <- choice 2
        case width of
            0 -> choice 255
            1 -> intBetween -32768 32767
            _ -> intBetween -9223372036854775808 9223372036854775807

listOf : fuzzer 'a -> fuzzer (list 'a)
listOf = listBetween 0 20

-- one `choice 1` continue bit per element; `0` stops
listBetween : int -> int -> fuzzer 'a -> fuzzer (list 'a)
listBetween lo hi item =
    let
        go n =
            if n >= hi then constant []
            else if n < lo then more n
            else
                do
                    continue <- choice 1
                    if continue == 0 then constant [] else more n
        more n =
            do
                x <- item
                xs <- go (n + 1)
                pure (x :: xs)
    in
    go 0
```

`Seeded`/`Replayed` field representations are Big (`Bytes`, `List Int`, `Int`), so
`choice` lowers them to work and lifts them back; the runner reads the
returned `Prng` with `unwrap_constr`. The runner protocol (`draw`/`run`
programs, `Prng::from_seed`, `Prng::from_choices`, replay returning `None`
when the sequence runs out or a choice exceeds its bound) is
docs/testing.md "How the runner drives a property" and plans/10 chunks
4–7.

**Elm/Aiken reference**

Aiken `stdlib/lib/aiken/fuzz.ak` (`rand`, `int`, `list`, `bool`,
`bytearray`) and `crates/aiken-lang/src/test_framework.rs` `Prng`
(constructor tags: `Seeded = 0`, `Replayed = 1`; `Some = 0`, `None = 1`
must match the little `option` layout in plans/04). Nash differs in the
choice element type: `Int`, not bytes (testing.md "Open questions").

**Tests**

- `tests` block: `run (choice 10) (Seeded (lift "seed") (lift []))` is `Some`; `run (choice 10) (Replayed (lift 0) (lift []))` is `None`; `run (choice 10) (Replayed (lift 1) (lift [lift 11]))` is `None` (over bound); `intBetween 3 3` is `3`.
- `prop "intBetween in range"`: `let lo via int; n via intBetween 0 1000` then `intBetween lo (lo + n)` sampled through `Fuzz.run` stays in range.
- Rust (plans/10 chunk 6): a shrink test that a failing `listOf int` counterexample shrinks to `[0]` or `[]`.

**Done when** the in-process Base test runner runs the props with the plans/10 runner.

---

## Chunk 9: `Test`

**Files**

- `crates/nash-driver/base/src/Test.nash`
- `crates/nash-codegen/src/test.rs` (plans/10 chunks 3–4: power-assert rewrite, `draw`/`run` programs)

**Change**

`label` and `assertFailed` from docs/stdlib.md "`Test`". There is no test
monad: a test body is a sequencing `do` block that desugars to plain `let`
(docs/testing.md "Test body"), `label : string -> unit` compiles to a
`\0label\0` trace, and `assert` is a keyword whose power-assert rewrite
(plans/10 chunk 3) ends in `Test.assertFailed`, which traces one
`\0assert\0` payload line per captured operand and then errors. This
module provides the runtime side.

**Code**

```elm
module Test exposing (label, assertFailed)

import Builtin

label : string -> unit
label s = Builtin.trace (Builtin.appendString "\u{0}label\u{0}" s) ()

-- Target of the power-assert rewrite; payload lines in order, then the error.
assertFailed : list string -> 'a
assertFailed msgs =
    case msgs of
        [] -> fail
        m :: rest -> Builtin.trace m (\() -> assertFailed rest) ()
```

Codegen for `Builtin.trace` delays its second argument (plans/07), which
is what makes the traces fire before the error.

**Elm/Aiken reference**

Aiken `crates/aiken-lang/src/test_framework.rs` `Assertion` (operand
capture for `==`, `!=`, `<`, etc.); Elm `elm-explorations/test` `Expect`
for naming only.

**Tests**

`tests` block: `test "labels are traces" = do label "a"` passes and the
runner's label table shows `a`; `test "assert False fails" fail = do assert False`;
`test "assertFailed traces then errors" fail = do assertFailed ["x", "y"]`
with the trace log `["x", "y"]`.

**Done when** `nash test` on a user project reports labels and
power-assert output through this module.

---

## Chunk 10: `Ast` and `Derive`

**Files**

- `crates/nash-driver/base/src/Ast.nash`
- `crates/nash-driver/base/src/Derive.nash`
- `core/tests/DeriveTests.nash`

**Change**

The `Ast` types and builders from docs/macros.md, matching
`crates/nash-macro/src/tags.rs` (plans/11 chunk 4; constructor tags are
declaration indices, so the two files change together); `Derive` from
plans/11 chunk 10. Both are plain Nash. Every `Ast` type is a little ADT
with native `string`/`int`/`bytes` fields, `option` slots, and `cons`
child lists (chunk 6); `Ast` imports `Prelude`, `Cons`, `String`, and the
trait modules. No `Data`, `Lift`, or Big type appears.

**Code** (`crates/nash-driver/base/src/Ast.nash` builders excerpt)

```elm
expr : exprNode -> expr
expr node = Expr { span = None, typ = None } node

name : string -> name
name s = Local s

var : name -> expr
var n = expr (Var n)

int : int -> expr
int n = expr (IntLit n)

call : expr -> cons expr -> expr
call f args = expr (Call f args)

tuple : cons expr -> expr
tuple es = expr (Tuple es)

and : cons expr -> expr
and es =
    case es of
        Nil -> expr (Var (Global builtinModule "True"))
        Cons e rest -> Cons.foldl (\b acc -> expr (BinOp (Global boolModule "and") acc b)) e rest

builtinModule : modname
builtinModule = { package = Some "nash/base", name = "Builtin" }

boolModule : modname
boolModule = { package = Some "nash/base", name = "Bool" }

exprName : expr -> option string
exprName (Expr _ node) =
    case node of
        Var (Raw s) -> Some s
        Var (Global _ s) -> Some s
        _ -> None
```

**Elm/Aiken reference**

None; see plans/11.

**Tests**

`core/tests/DeriveTests.nash` per plans/11 chunk 10; `Ast.nash` `tests`:
`exprName (var (raw "Eq")) == Some "Eq"`, `and Nil` is the `True` node,
`Cons.length (Cons (int 1) Nil) == 1`.

**Done when** plans/11 chunk 12's `decl_macro_derive_eq` snapshot passes.

---

## Chunk 11: `Cardano.*`

**Files**

- `crates/nash-driver/base/src/Cardano/Tx.nash`, `Cardano/Address.nash`, `Cardano/Value.nash`, `Cardano/Time.nash`
- `core/tests/golden/*.cbor` (real V3 script contexts)
- `core/tests/CardanoTests.nash`

**Change**

Big ADTs for the V3 `ScriptContext` per docs/stdlib.md, `Lift value Value`,
interval helpers. Golden tests decode real contexts with
`Validate.validate` and check a few fields.

**Code** (`crates/nash-driver/base/src/Cardano/Value.nash` excerpt)

```elm
module Cardano.Value exposing (..)

import Builtin

type alias Value = Map Bytes (Map Bytes Int)

impl Lift value Value where
    lift = Builtin.unValueData << toData
    lower = fromData << Builtin.valueData

lovelace : Value -> int
lovelace v = Builtin.lookupCoin "" "" (lift v)

quantityOf : bytes -> bytes -> Value -> int
quantityOf policy name v = Builtin.lookupCoin policy name (lift v)
```

Note the naming: `lift : value -> Value` here goes from Const to Big,
matching `Lift 'small 'big`'s direction (little is small).

**Elm/Aiken reference**

Aiken `stdlib/lib/cardano/transaction.ak`, `cardano/address.ak`,
`cardano/assets.ak` for field order and constructor tags (they match the
ledger). Plutus `plutus-ledger-api` `V3/Contexts.hs` is the source of
truth for the encoding.

**Tests**

`core/tests/CardanoTests.nash`: `validate` on each golden context is
`Some`; `Tx.inputs` length matches; `lovelace` of the first output
matches the fixture.

**Done when** all fixtures decode and the in-process Base compilation tests stays green.

---

## Test harness

- `crates/nash-driver/tests/bundled_base.rs` compiles every embedded module and import-free applications through the driver library, without invoking the CLI.
- `crates/nash-driver/tests/testing_base.rs` executes the Base fuzzer and test protocol through codegen and the evaluator.
- `crates/nash-driver/tests/vesting.rs` compiles validators against bundled Base and executes serialized UPLC.
- `nash-can` tests preserve source imports and enforce the real-only Builtin inventory.
- CI runs these with `cargo test`; unit and integration tests do not spawn the Nash CLI.

## Open questions

Same as docs/stdlib.md (`Fuzz`/`Test` as default imports; `value`
builtins gated by target version). Neither blocks a chunk.
