# Data — the `Data` type, casts, encoders and decoders

`Data` is the Plutus `Data` value: the on-chain wire format for datums,
redeemers and the script context. In Nash it is an ordinary Big type with
five constructors that can be pattern matched, plus a small set of traits
and a stdlib decoder library on top. Decisions follow
[overview.md](overview.md); the representation of every other type is in
[representation.md](representation.md); the lowering of casts is in
[codegen.md](codegen.md).

## The type

```elm
type Data
    = Constr int (list Data)
    | Map (list (pair Data Data))
    | List (list Data)
    | I int
    | B bytes
```

`Data` is Big (it *is* a `Data` constant at runtime) and is special-cased by
the compiler: its constructors are not `Constr i [...]` nodes but the five
Data node shapes themselves, and its fields are `Const` types (`int`,
`bytes`, `list Data`, `list (pair Data Data)`) rather than Big ones, which
is what the builtins `unConstrData`, `unMapData`, `unListData`, `unIData`,
`unBData` return. This is the one Big ADT whose fields are not Big. Every
other Big value (an `Int`, a `Datum`, a `List Int`) is also a `Data` value
at runtime; `toData` on it is the identity.

Mapping to nash-plutus (`crates/nash-plutus/src/data.rs:10`):

| Nash constructor | `PlutusData` | build | take apart |
|---|---|---|---|
| `Constr tag fields` | `Constr { tag, fields }` | `constrData` | `unConstrData` then `fstPair` / `sndPair` |
| `Map kvs` | `Map(..)` | `mapData` | `unMapData` |
| `List xs` | `List(..)` | `listData` | `unListData` |
| `I n` | `Integer(..)` | `iData` | `unIData` |
| `B bs` | `ByteString(..)` | `bData` | `unBData` |

There is no `data` little type.

## Constructing

```elm
d : Data
d = Constr 0 [ I 1, B #"cafe" ]
```

The constructor application lowers to
`constrData 0 (mkCons (iData 1) (mkCons (bData #"cafe") []))`; when every
argument is a literal the optimizer folds the whole thing to one `Data`
constant. `I`, `B`, `List`, `Map` lower to `iData`, `bData`, `listData`,
`mapData`.

## Pattern matching

```elm
case d of
    Constr 0 [ owner, deadline ] -> ...
    Constr _ _                   -> ...
    I n                          -> ...
    _                            -> ...
```

A match on a `Data` scrutinee produces a `Core` `Case(Data, ...)` node with
up to five branches, one per constructor, and a default. It lowers to

```
force (chooseData d (delay constrBranch)
                    (delay mapBranch)
                    (delay listBranch)
                    (delay iBranch)
                    (delay bBranch))
```

Constructors that no clause mentions share the default branch. Inside a
branch the fields are bound with the matching `un*Data` builtin and the
rest of the pattern is an ordinary `Const` match:

- `Constr tag fields`: `let p = unConstrData d`, `tag = fstPair p`,
  `fields = sndPair p`. A literal tag becomes an `int` switch
  (`equalsInteger`); a list pattern on `fields` is a `list Data` match
  (`chooseList` / `headList` / `tailList`, with `nullList` for the exact
  length of a `[a, b]` pattern).
- `List xs`: `xs = unListData d`, then a `list Data` match.
- `Map kvs`: `kvs = unMapData d`, elements are `pair Data Data`
  (`fstPair`/`sndPair`).
- `I n`, `B bs`: `unIData`, `unBData`.

The decision-tree compiler treats `Data` columns like any other: the
`chooseData` test is the switch and the `un*Data` projections are memoized
accessor paths, so a match that tests `Constr` twice with different tags
unwraps once.

Pattern matching on `Data` is the primitive everything else in this
document is built from. It is also how `Data` arguments to `main` are
usually inspected when a full `validateData` is too expensive.

## Traits

```elm
trait ToData 'a where
    toData : 'a -> Data

trait FromData 'a where
    fromData     : Data -> 'a     -- shallow check
    validateData : Data -> 'a     -- full check

trait Lift 'small 'big where
    lift  : 'small -> 'big
    lower : 'big -> 'small
```

`ToData` and `FromData` are only implementable for Big types (kind
`Big`); the impl is derived by `@derive(ToData, FromData)` (see
[macros.md](macros.md)) and the stdlib provides impls for `Int`, `Bytes`,
`Data`, `List 'a`, `Map 'k 'v`. Because every Big value is already `Data`,
all three methods are identity **at runtime** and differ only in what they
check:

| Method | Checks | Fails |
|---|---|---|
| `toData` | nothing | never |
| `fromData` | the outermost node only | trace + `error` |
| `validateData` | the whole value recursively | trace + `error` |

`fromData` and `validateData` **fail** (they do not return `option`)
because a validator's success is "did not error" and because a failing
check is the common case for a redeemer or datum. Non-failing variants live
in `Data.Decode` (`run` returns a `result`).

The shallow check for each shape of Big type:

| Big type | `fromData` checks | `validateData` additionally checks |
|---|---|---|
| ADT with `n` constructors (labeled fields included) | node is `Constr`, `0 <= tag < n`, field count matches the constructor's arity | every field, recursively, in declaration order |
| record alias with `n` fields | node is `List` of length `n` | every field |
| `Int` | node is `I` | nothing more |
| `Bytes` | node is `B` | nothing more |
| `List 'a` | node is `List` | every element is a valid `'a` |
| `Map 'k 'v` | node is `Map` | every key is a valid `'k`, every value a valid `'v` |
| `Data` | nothing | nothing |

### Generated code shape

`fromData` and `validateData` reach codegen as `Cast(FromDataShallow)` and
`Cast(ValidateData)` nodes and lower to a call of a compiler-generated
checker for the target type, then the value itself:

```
fromData#Datum = \d ->
    let p = unConstrData d
    case fstPair p of
      0 -> let fs = sndPair p
           if nullList (tailList (tailList fs)) then d else fail
      _ -> fail
```

```
validateData#Datum = \d ->
    let p = unConstrData d
    case fstPair p of
      0 -> let fs = sndPair p
           let _  = validateData#Bytes (headList fs)
           let _  = validateData#Int   (headList (tailList fs))
           if nullList (tailList (tailList fs)) then d else fail
      _ -> fail
```

Checkers are generated once per Big type, hoisted to top-level `LetRec`
bindings (recursive types produce recursive checkers), and shared by every
use in the program. `validateData#List#Int` is a loop over `unListData d`
calling `validateData#Int`; `validateData#Data` is the identity and is
inlined away. The `fail` carries a compiler-generated trace
(`"validateData: Datum field 1"`) governed by the `compilerTraces` switch
([codegen.md](codegen.md), Runtime errors).

`chooseData` is what makes the shape tests cheap: a checker for `Int` is
`force (chooseData d (delay fail) (delay fail) (delay fail) (delay d) (delay fail))`.
This mirrors Aiken's `softcast_data_to_type_otherwise`
(`crates/aiken-lang/src/gen_uplc/builder.rs:589`) and `unknown_data_to_type`
(`builder.rs:529`), with two differences: Nash never converts to a little
type inside the checker (the result stays `Data`), and shallow vs full is
chosen by the caller, not by an `ExpectLevel` threaded through the tree.

## `Lift` between representations

`Lift 'small 'big` connects a `Const` type to the Big type with the same
content. The impls shipped in `core/` are the table of
[representation.md](representation.md) (which is authoritative):

| `'small` | `'big` | `lift` | `lower` |
|---|---|---|---|
| `int` | `Int` | `iData` | `unIData` |
| `bytes` | `Bytes` | `bData` | `unBData` |
| `bool` | `Bool` | `ifThenElse c (Constr 1 []) (Constr 0 [])` | tag compare |
| `unit` | `Unit` | `Constr 0 []` | `()` |
| `list 'a` given `Lift 'a 'b` | `List 'b` | map `lift` over the elements, then `listData` | `unListData`, then map `lower` |
| `list (pair 'k 'v)` with `'k 'v : Big` | `Map 'k 'v` | `mapData` | `unMapData` |
| `'a` for every `'a : Big` (built-in reflexive impl) | `'a` | identity | identity |
| `option 'a` given `Lift 'a 'b` | `Option 'b` | `case`, rebuild | `unConstrData`, rebuild |
| `result 'e 'a` given `Lift 'e 'f`, `Lift 'a 'b` | `Result 'f 'b` | as `option` | as `option` |
| `ordering` | `Ordering` | rebuild | rebuild |

So `list int` lifts to `List Int` through `Lift int Int` on each element,
and `list Int` lifts to `List Int` through the reflexive impl, where the
element map is the identity and the optimizer reduces the whole `lift` to
`listData`. `string` has no Big partner (`Bytes` via `encodeUtf8` is a user
function, not a lift).

The `int`, `bytes`, `list`/`Map` and reflexive impls lower to `Cast(Lift)`
/ `Cast(Lower)` nodes and then to one builtin (or nothing, for the
reflexive impl). The others are ordinary Nash functions in `core/`, whose
`case` is compiled like any other. `lower (lift x)` and `lift (lower d)`
cancel in the optimizer (Aiken `cast_data_reducer`). A user-written
`impl Lift myLittle MyBig` for a user pair is an ordinary function too.

## Encoding: `Data.Encode`

Encoders are plain functions to `Data`. The module is small because
`toData` already covers every Big type:

```elm
module Data.Encode exposing (int, bytes, list, map, constr, bool)

int : int -> Data
int = lift

bytes : bytes -> Data
bytes = lift

list : list Data -> Data
list = List

map : list (pair Data Data) -> Data
map = Map

constr : int -> list Data -> Data
constr = Constr

bool : bool -> Data
bool b = Constr (if b then 1 else 0) []
```

A little ADT is encoded by writing its `toData`-like function by hand or
via a `Lift` impl to its Big twin.

Labeled constructor fields encode flat. `type Datum = Datum { owner :
Bytes, deadline : Int }` is one constructor with two positional fields
whose labels exist only at compile time, so a value is
`Constr 0 [B owner, I deadline]`, never `Constr 0 [List [..]]`. The
derived `ToData` is the identity and the derived `FromData` checks arity
`2` under tag `0`. A little labeled constructor is `constr i [..]` the
same way. Only `type alias` records are a `List` of fields.

## Decoding: `Data.Decode`

Elm-JSON-style combinators written in Nash on top of `Data` patterns. A
decoder is a function, hence a little type:

```elm
module Data.Decode exposing
    ( decoder, error, run, expect
    , data, int, bytes, list, map, pair
    , field, index, constr, tag
    , succeed, fail, map1, map2, map3, andThen, oneOf
    )

type error
    = Failure string
    | At int error

type result 'e 'a = Ok 'a | Err 'e              -- from the prelude; Ok is tag 0

type decoder 'a = Decoder (Data -> result error 'a)

run : decoder 'a -> Data -> result error 'a
run (Decoder f) d = f d

expect : decoder 'a -> Data -> 'a
expect dec d =
    case run dec d of
        Ok a -> a
        Err e -> fail (describe e)

-- primitives

data : decoder Data
data = Decoder Ok

int : decoder int
int = Decoder (\d -> case d of
    I n -> Ok n
    _ -> Err (Failure "expected I"))

bytes : decoder bytes
bytes = Decoder (\d -> case d of
    B b -> Ok b
    _ -> Err (Failure "expected B"))

list : decoder 'a -> decoder (list 'a)
list item = Decoder (\d -> case d of
    List xs -> traverse item xs
    _ -> Err (Failure "expected List"))

map : decoder 'k -> decoder 'v -> decoder (list (pair 'k 'v))
pair : decoder 'a -> decoder 'b -> decoder (pair 'a 'b)

-- structure

field : int -> decoder 'a -> decoder 'a          -- i-th element of a List
field i item = Decoder (\d -> case d of
    List xs -> at i (run item) xs
    _ -> Err (Failure "expected List"))

index : int -> decoder 'a -> decoder 'a          -- i-th field of a Constr
index i item = Decoder (\d -> case d of
    Constr _ fs -> at i (run item) fs
    _ -> Err (Failure "expected Constr"))

tag : decoder int                                -- the Constr tag
constr : int -> decoder 'a -> decoder 'a         -- require tag, decode fields as List
constr t item = Decoder (\d -> case d of
    Constr t' fs -> if t == t' then run item (List fs)
                    else Err (Failure "wrong tag")
    _ -> Err (Failure "expected Constr"))

-- combinators

succeed : 'a -> decoder 'a
fail : string -> decoder 'a
map1 : ('a -> 'b) -> decoder 'a -> decoder 'b
map2 : ('a -> 'b -> 'c) -> decoder 'a -> decoder 'b -> decoder 'c
map3 : ...
andThen : ('a -> decoder 'b) -> decoder 'a -> decoder 'b
oneOf : decoder 'a -> decoder 'a -> decoder 'a   -- binary; chain for more

impl Functor decoder where ...
impl Monad decoder where ...                     -- enables `do`
```

`list` inherits the kind constraint of `list`: `'a` must be Storable, so
`list int`, `list Int` and `list Data` decode, but a `list (option int)`
decoder is a kind error. Decoding a `List` into a little container is done
with `andThen` and a fold over `list Data`.

Example, a decoder for the `Datum` of [overview.md](overview.md) that
returns a little record:

```elm
type alias datum = { owner : bytes, deadline : int }

datumDecoder : decoder datum
datumDecoder =
    constr 0
        (map2 (\o d -> { owner = o, deadline = d })
            (field 0 bytes)
            (field 1 int))
```

and with `do`:

```elm
datumDecoder =
    constr 0 <| do
        o <- field 0 bytes
        d <- field 1 int
        succeed { owner = o, deadline = d }
```

Compared with `fromData`, a decoder converts to little types as it goes
and reports *where* it failed (`At 1 (Failure "expected I")`). Compared
with a hand-written `case`, it composes.

### Cost and fusion

A decoder as written above allocates a `result` constr per step and calls
through `Decoder` closures. After monomorphization and inlining most of the
closures disappear, but the `Ok`/`Err` allocations remain. The compiler
**may** later recognize decoder combinators (by their stdlib names, after
monomorphization) and fuse a whole decoder into one decision tree with
`chooseData` tests and memoized accessors, which is what the `case` version
compiles to. This is an optimization, not a semantic change: `run` on a
fused decoder returns the same `result`. Nothing in the language depends on
it, so the stdlib is written first and the fusion pass is scheduled after
`plans/08-optimizer.md`.

## Error cases

| Situation | Where reported |
|---|---|
| `impl ToData` / `impl FromData` for a non-Big type | kinds check |
| `Constr` pattern with a Big field type (e.g. `Constr 0 [x : Int]`) | type check (fields of `Data` are `Const`) |
| `fromData d` where the node shape is wrong | runtime `error` with compiler trace |
| `validateData d` on a recursive type with a cycle in the data | cannot happen; `Data` is a finite tree |
| `lift` at a pair with no impl | trait resolution error |
| non-exhaustive `case` on `Data` | nitpick error |

## Interactions

- **Kinds** ([kinds.md](kinds.md)): `Data : Big`; its constructor fields
  are `Const`.
- **Traits** ([traits.md](traits.md)): `ToData`, `FromData`, `Lift` are
  ordinary traits with stdlib impls; deriving is a macro.
- **Codegen** ([codegen.md](codegen.md)): `Cast` nodes, checker generation,
  the `Case(Data)` lowering.
- **Validators** ([validators.md](validators.md)): `main` arguments are Big
  or Const; for the Big ones, `Data` patterns and `fromData` are how their
  shape is checked.
- **Macros** ([macros.md](macros.md)): `@derive(ToData, FromData)`.
  A `field "owner"` form of `Data.Decode.field` that resolves the label
  through an alias in scope would be a macro, not a library function.
