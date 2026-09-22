# Data — the `Data` type, encoders and decoders

`Data` is the Plutus `Data` value: the on-chain wire format for datums,
redeemers and the script context. In Nash it is an ordinary Big type with
five constructors that can be pattern matched, plus a small set of traits
and a stdlib decoder library on top. Decisions follow
[overview.md](overview.md); the representation of every other type is in
[representation.md](representation.md); builtin lowering is in
[codegen.md](codegen.md).

## The type

```elm
type Data
    = Constr (pair int (list Data))
    | Map (list (pair Data Data))
    | List (list Data)
    | I int
    | B bytes
```

`Data` is Big (it *is* a `Data` constant at runtime) and is special-cased by
the compiler: its constructors are not `Constr i [...]` nodes but the five
Data node shapes themselves, and its fields are `Const` types (`pair int (list Data)`, `int`,
`bytes`, `list Data`, `list (pair Data Data)`) rather than Big ones, which
is what the builtins `unConstrData`, `unMapData`, `unListData`, `unIData`,
`unBData` return. This is the one Big ADT whose fields are not Big. Every
other Big value (an `Int`, a `Datum`, a `List Int`) is also a `Data` value
at runtime; source codecs preserve its wire shape.

Mapping to nash-plutus (`crates/nash-plutus/src/data.rs:10`):

| Nash constructor | `PlutusData` | build | take apart |
|---|---|---|---|
| `Constr payload` | `Constr { tag, fields }` | `constrData` | `unConstrData`; explicit pair pattern uses native `case` |
| `Map kvs` | `Map(..)` | `mapData` | `unMapData` |
| `List xs` | `List(..)` | `listData` | `unListData` |
| `I n` | `Integer(..)` | `iData` | `unIData` |
| `B bs` | `ByteString(..)` | `bData` | `unBData` |

There is no `data` little type.

## Constructing

```elm
d : Data
d = Builtin.constrData 0 [ I 1, B #"cafe" ]
```

`Constr : pair int (list Data) -> Data` rewraps a decoded pair. To build
from a separate tag and field list, call `Builtin.constrData`. The call lowers to
`constrData 0 (mkCons (iData 1) (mkCons (bData #"cafe") []))`; when every
argument is a literal the optimizer folds the whole thing to one `Data`
constant. `I`, `B`, `List`, `Map` lower to `iData`, `bData`, `listData`,
`mapData`.

## Pattern matching

```elm
case d of
    Constr pair(0, [ owner, deadline ]) -> ...
    Constr pair(_, _)                   -> ...
    I n                          -> ...
    _                            -> ...
```

A match on a `Data` scrutinee produces a `Core` `Case(Data, ...)` node with
up to five branches, one per constructor, and a default. It lowers to

```
force (chooseData d
    (delay constrBranch) (delay mapBranch) (delay listBranch)
    (delay iBranch) (delay bBranch))
```

The five Data shapes are selected by `chooseData`; only the selected delayed
branch is forced. Constructors that no clause
mentions share the default branch. Inside a
branch the fields are bound with the matching `un*Data` builtin and the
rest of the pattern is an ordinary `Const` match:

- `Constr payload`: `payload = unConstrData d`. An explicit
  `Constr pair(tag, fields)` pattern destructures it with native `case`.
  A literal tag becomes an `int` switch
  (`equalsInteger`); a list pattern on `fields` is a `list Data` match
  (native `case`, with cons at branch 0 receiving head and tail, and nil
  at branch 1).
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
usually inspected when a full `validate` is too expensive.

## Traits

```elm
trait ToData ('a : Big) where
    toData : 'a -> Data

impl ToData ('a : Big) where
    toData = Primitive.coerce

trait FromData ('a : Big) where
    fromData     : Data -> 'a     -- unchecked identity

impl FromData ('a : Big) where
    fromData = Primitive.coerce

trait Validate ('a : Big) where
    validate : Data -> 'a     -- required recursive validation

trait Lift 'small 'big where
    lift  : 'small -> 'big
    lower : 'big -> 'small
```

`ToData` and `FromData` apply to Big types. Core supplies an ordinary blanket
impl for each trait, covering every Big type, including user ADTs, nominal
record aliases, lists and maps. The impl bodies define the coercion methods directly;
collection elements need no conversion or validation constraints. Neither
conversion trait needs derivation, and concrete impls would overlap the
blanket impls. `Validate` is separate and opt-in: core provides impls for
`Int`, `Bytes`, `Data`, `List 'a` and `Map 'k 'v`; user types need a source
`Validate` impl. `@derive(Validate)` remains future macro work.

`fromData` uses `Primitive.coerce`, an unchecked identity. It checks
neither the outer Data shape nor nested fields. Malformed data fails only
if a later operation needs the expected shape; a value that is never inspected
can pass through unchanged. `Validate.validate` is the required method of a separate trait:
core impls check the shape and recursively validate collection elements.
`Data` itself accepts every Data shape. `Data.Decode` supplies non-failing
result-based decoding.

`toData` uses `Primitive.coerce`: every Big value already has its Data
representation. Conversion preserves that value and its wire encoding without
traversing or rebuilding it. Concrete `ToData` impls would overlap the blanket
impl and are rejected.

For example, validation is an ordinary source impl:

```elm
impl Validate Int where
    validate value =
        case value of
            I _ -> Primitive.coerce value
            _ -> fail
```

`Bytes` validation similarly checks its Data shape before coercing the
original value. List and map validation retains recursive source checks.
The actual builtin `iData` has type `int -> Int`; the existing constructor
`I` has type `int -> Data`. Both emit the same UPLC Data shape.

`Primitive.coerce : 'a -> 'b` is an explicit compiler intrinsic, not a Plutus
builtin. Its two type variables accept any value types independently,
including functions, with no representation constraint. It performs no
validation and no runtime representation change. It is therefore the caller's
responsibility to use the result with a compatible runtime representation.
The real Plutus builtin inventory stays unchanged.

## `Lift` between representations

`Lift 'small 'big` connects a `Const` type to the Big type with the same
content. The impls shipped in `crates/nash-driver/base/` are the table of
[representation.md](representation.md) (which is authoritative):

| `'small` | `'big` | `lift` | `lower` |
|---|---|---|---|
| `int` | `Int` | `iData` | `unIData` |
| `bytes` | `Bytes` | `bData` | `unBData` |
| `bool` | `Bool` | `case c [Constr 0 [], Constr 1 []]` | tag compare |
| `unit` | `Unit` | `Constr 0 []` | `()` |
| `list ('a : Big)` | `List 'a` | `listData` | `unListData` |
| `list (pair 'k 'v)` with `'k 'v : Big` | `Map 'k 'v` | `mapData` | `unMapData` |
| `'a` for every type (built-in reflexive impl) | `'a` | identity | identity |
| `option ('a : Big)` | `Option 'a` | `case`, rebuild | `unConstrData`, rebuild |
| `result ('e : Big) ('a : Big)` | `Result 'e 'a` | as `option` | as `option` |
| `ordering` | `Ordering` | rebuild | rebuild |

Container conversions preserve their element types and values. Convert native
integers explicitly with `List.map lift` before wrapping as a Big List.
String encoding uses `String.toBytes` / `String.fromBytes` rather than Lift.

The primitive and collection Lift impls are ordinary Nash functions calling
concrete typed Data builtins. The reflexive impl is identity. Other Lift
impls use normal pattern matching and construction. Optimizations operate
on actual builtin applications and preserve errors and evaluation order.

## Encoding: `Data.Encode`

Encoders are plain functions to `Data`. The module is small because
every Big type already has `ToData` through the blanket impl:

```elm
module Data.Encode exposing (int, bytes, list, map, constr, bool)

int : int -> Data
int = I

bytes : bytes -> Data
bytes = B

list : list Data -> Data
list = List

map : list (pair Data Data) -> Data
map = Map

constr : int -> list Data -> Data
constr = Builtin.constrData

bool : bool -> Data
bool b = Builtin.constrData (if b then 1 else 0) []
```

A little ADT is encoded by writing its `toData`-like function by hand or
via a `Lift` impl to its Big twin.

Labeled constructor fields encode flat. `type Datum = Datum { owner :
Bytes, deadline : Int }` is one constructor with two positional fields
whose labels exist only at compile time, so a value is
`Constr 0 [B owner, I deadline]`, never `Constr 0 [List [..]]`. The
future derived codecs reconstruct the same encoding and check arity
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
    Constr pair(_, fs) -> at i (run item) fs
    _ -> Err (Failure "expected Constr"))

tag : decoder int                                -- the Constr tag
constr : int -> decoder 'a -> decoder 'a         -- require tag, decode fields as List
constr t item = Decoder (\d -> case d of
    Constr pair(t', fs) -> if t == t' then run item (List fs)
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

`list` inherits the datatype context of `list`: `'a` must be Storable, so
`list int`, `list Int` and `list Data` decode, but a `list (option int)`
decoder fails the `Storable` representation predicate. Decoding a `List` into a little container is done
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

Unlike unchecked `fromData`, a decoder validates and converts to little types as it goes
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
| `impl ToData` / `impl FromData` / `impl Validate` for a non-Big type | representation superclass check |
| `Constr` pattern with a Big field type (e.g. `Constr pair(0, [x : Int])`) | type check (fields of `Data` are `Const`) |
| `fromData d` where the node shape is wrong | no check; a later operation requiring that shape can fail |
| `validate d` where the node shape is wrong | runtime failure in source validation |
| `validate d` on a recursive type with a cycle in the data | cannot happen; `Data` is a finite tree |
| `lift` at a pair with no impl | trait resolution error |
| non-exhaustive `case` on `Data` | nitpick error |

## Interactions

- **Kinds and representation** ([kinds.md](kinds.md)): `Data` has kind
  `Type` and representation `Big`; its constructor fields
  are `Const`.
- **Traits** ([traits.md](traits.md)): `ToData`, `FromData`, `Validate`, `Lift` are
  ordinary traits with stdlib impls; deriving is a macro.
- **Codegen** ([codegen.md](codegen.md)): typed builtins, Data patterns,
  the `Case(Data)` lowering.
- **Validators** ([validators.md](validators.md)): `main` arguments are Big
  or Const; for the Big ones, `Data` patterns and `validate` are how their
  shape is checked.
- **Macros** ([macros.md](macros.md)): `@derive(Validate)`; conversion traits need no derivation.
  A `field "owner"` form of `Data.Decode.field` that resolves the label
  through an alias in scope would be a macro, not a library function.
