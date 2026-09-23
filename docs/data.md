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
option-based decoding.

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
| `ordering` | `Ordering` | rebuild | rebuild |

Container conversions preserve their element types and values. Convert native
integers explicitly with `List.map lift` before wrapping as a Big List.
String encoding uses `String.toBytes` / `String.fromBytes` rather than Lift.

The primitive and collection Lift impls are ordinary Nash functions calling
concrete typed Data builtins. The reflexive impl is identity. Other Lift
impls use normal pattern matching and construction. Optimizations operate
on actual builtin applications and preserve errors and evaluation order.

## Encoding and decoding

`Data.Encode` contains ordinary Nash functions for integers, bytes, UTF-8
strings, booleans, lists, maps and constructors. See [stdlib.md](stdlib.md#datadecode-dataencode)
for their signatures. Encoders accept Big/little outer representations where
applicable, and element encoders are explicit. `Data.tag` and `Data.fields`
assume constructor Data and fail on other shapes.

Labeled constructor fields encode flat: `type Datum = Datum { owner : Bytes,
deadline : Int }` has two positional fields under tag 0. Alias records use a
Data list instead.

`Data.Decode.decoder 'a` is the function alias `Data -> option 'a`, with
ordinary Functor, Applicative and Monad instances. Primitive decoders return
`None` for the wrong shape; string decoding also rejects malformed UTF-8.
`constr` checks a tag and keeps the original node; `field` selects a constructor
field. Missing/negative indices return `None`; extra fields are allowed.

```nash
import Data.Decode as Decode

type alias datum = { owner : bytes, deadline : int }

datumDecoder : Decode.decoder datum
datumDecoder = Decode.constr 0 <| do
    owner <- Decode.field 0 Decode.bytes
    deadline <- Decode.field 1 Decode.int
    pure { owner = owner, deadline = deadline }
```

Use ordinary `map`, `pure`, application and `do` instead of custom mapping or
binding helpers. `reject` always returns `None`; `oneOf` tries a `Cons` sequence
of decoders, stopping at the first success. Function values cannot inhabit a
native list. Similarly decoded maps use `Cons` of tuples so decoded components
can be arbitrary little values. Native `Decode.list` retains its Storable
constraint. Map encoding/decoding preserves duplicates and order.

The library is implemented in Nash. There is no compiler-generated decoder or
special codegen treatment of decoder combinators. A user-written decoder that
calls `fail` still aborts evaluation; `None` is the recoverable failure result.

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
