# Nash — Language Overview

Nash is a purely functional language with Elm/Haskell syntax that compiles to
Untyped Plutus Core (UPLC) and runs on the Plutus CEK machine. It is the
successor to Aiken: same on-chain target, but a real functional language with
type classes, higher-kinded types, layout-sensitive syntax, macros, and
compile-time evaluation.

This document is the single source of truth for design decisions. Component
docs in this folder expand on each area; `plans/` holds the implementation
plans in execution order.

## Development policy

Nash has no compatibility commitments to earlier syntax, APIs, or cache formats.
Implement the current design directly and remove superseded paths. Cache files
are disposable; do not add version migrations or older-format readers.

Source coordinates and indentation use `usize`, matching source byte offsets.
Valid Rust strings have at most `isize::MAX` bytes, so one-based positions and
EOF fit without a separate parser input-size failure. This makes `Region`
32 bytes on a 64-bit host. Protocol boundaries such as LSP check their narrower
coordinate limits explicitly; they must not truncate positions.

## Status

Done (ported from the Elm compiler, Haskell -> Rust):

- `nash-parse` — recursive-descent parser with Elm's full error hierarchy
- `nash-can` — canonicalization (name resolution, SCC ordering, interfaces)
- `nash-constrain` + `nash-solve` — HM inference (Elm's rank-based solver)
- `nash-driver` / `nash-config` / `nash-cli` — build graph, `nash.jsonc`, `nash check`
- `nash-plutus` — complete UPLC: terms, flat codec, CEK machine, cost models

The front end also implements Haskell 98 kinds, representation predicates,
traits, retained evidence and the approved Plan 03 shipping core. Remaining
work is tracked in [SPEC.md](../SPEC.md), including runtime/codegen, optimizer
implementation, default imports, Fuzz and the full validator example.

## Decisions at a glance

| Area | Decision |
|---|---|
| Type classes | `trait` / `impl` keywords, Haskell layout. Superclasses, default methods, multi-param traits. Rust orphan rules. Full monomorphization (no dictionaries). |
| Higher-kinded types | Yes. Haskell 98 kinds (`Type`, `k -> k`) inferred by unification with an occurs check; no kind polymorphism, no kind syntax. |
| Type variables | OCaml style: `'a`, `'b`. Bare lowercase names in type position are *little types*. |
| Representation | Every ground type has a representation decided by its head constructor: `Big` (Plutus `Data`), `Const` (UPLC builtin constant), or `Term` (UPLC `constr` term). Casing of the type name picks it: `Int` is Big, `int` is Const. Representation is **not** part of the kind: it is enforced by compiler-owned predicates `Big`/`Const`/`Term`/`Storable`/`Little`/`Apply` and inferred datatype contexts, resolved by the trait machinery. `('a : Storable)` is sugar for the context entry `Storable 'a`. |
| Records | Nominal only (via `type alias`). Row polymorphism removed. Record update syntax kept. Big record = `Data.List` of fields; little record = `constr 0`. |
| Tuples | Always `Term` (`constr 0 [..]`), fields of any representation (each has kind `Type`). No Big tuple. |
| Named ctor fields | `type Datum = Datum { owner : Bytes, deadline : Int }` is a constructor with *labeled fields* (Aiken style), not a nested record: encoded flat (`Constr 0 [B, I]` / `constr 0 [..]`), fields follow the enclosing type's representation rule, `.field` access allowed on single-constructor types, record-style construction `Datum { owner = o, deadline = d }` and pattern `Datum { owner, deadline }` allowed; record update only on alias records in v1. Anonymous record *types* are not allowed anywhere else; use `type alias`. |
| Prelude twins | `bool`/`Bool`, `unit`/`Unit`, `option`/`Option`, `result`/`Result`, `ordering`/`Ordering`. Little constructors are exposed unqualified by the prelude (`True`, `Some`, `Ok`, `LT`...); Big twins only qualified (`Bool.True`, `Option.Some`). `if` takes `bool`. |
| `Data` | Big type with constructors `Constr tag fields | Map kvs | List xs | I n | B bs`; pattern-matchable. Decoder combinators built on top in the stdlib. |
| Big <-> little | Explicit. `ToData`/`FromData` on Big types (`toData`, `fromData` shallow, `validateData` full). `Lift 'small 'big` multi-param trait (`lift`/`lower`) between reprs; built-in reflexive `impl Big 'a => Lift 'a 'a` (exempt from head rules) so `Lift 'a 'b => Lift (list 'a) (List 'b)` covers `list Int`. `validateData : Data -> 'a` traps on mismatch; `Data.Decode` is the non-failing path. No implicit coercion. |
| Literals | Polymorphic via `FromInt` / `FromString` / `FromBytes` traits. Ambiguous literals default to little (`int`, `string`, `bytes`). |
| Operators | Trait methods (`Num`, `Integral`, `Eq`, `Ord`, `Semigroup`, ...). Elm's `number`/`comparable`/`appendable` supertypes removed. |
| Operator sections | Whole `(+)` plus partial `(> 5)` / `(5 >)`, canonicalized to hygienic lambdas; `(-x)` stays negation. |
| Dropped from Elm | `Float`, `Char`, row polymorphism, magic supertypes, ports/effects. |
| Validators | `validator module Foo exposing (main)`. `main` required, signature free (args must have Big or Const representation; Term arguments are an error), all args become lambdas. Success = evaluation does not error; return value ignored. No blueprint. |
| Tests | `tests` block at end of module with its own imports. `test "name" =`, `prop "name" =` with `let x via gen in`. Bodies are sequencing blocks (`e : unit` ⇒ `let () = e in ..`; `x <- e` ⇒ `let x = e`; no test monad). Power-assert `assert`. `fail` / `fail once`, `within (cpu N, mem M)`, `label`. |
| Property testing | Aiken design: `type Prng = Seeded Bytes (List Int) \| Replayed Int (List Int)` (Big, built by the runner as Data), choice-sequence shrinking in Rust, `fuzzer 'a` little type with Functor/Applicative/Monad. Each prop compiles to `draw`/`run` programs. |
| `do` notation | Layout `do` block, `x <- e` desugars to `Monad.bind`. |
| Macros | Procedural. Input: typed AST; output: surface AST. `@derive(Eq)` on declarations, `name!(args)` in expressions. Hygienic. Run on the CEK machine. Expand-then-recheck loop per module. The `Ast` family has Term representation (little ADTs with `string`/`int`/`bytes` fields; child lists as core `cons 'a = Nil \| Cons 'a (cons 'a)`); the host builds input as a `Term::Constr` tree and reads output from the CEK result value. |
| Comptime | `comptime expr` evaluates on the CEK machine at compile time; result must be a UPLC constant (`Const` or `Big`). |
| Deriving | Implemented as macros (`@derive(Eq, Ord, Show, ToData, FromData)`). |
| IR | Single tree IR (`Core`): monomorphized lambda calculus with explicit reprs. Core -> Core optimization passes. Core -> UPLC `Term`. |
| Pattern matching | Maranget decision trees, hoisted leaves, memoized accessors. Exhaustiveness from Elm's `Nitpick/PatternMatches`. |
| Recursion | Self-application with static-parameter lifting; mutual recursion via a combined dispatcher. No Y combinator. |
| Runtime errors | `fail`, `todo`, `trace`, `assert`. Trace levels silent / compact / verbose; compiler-generated traces separate switch. |
| Diagnostics | Elm's `Reporting/*` prose ported into miette `Diagnostic`s. One `nash-report` crate. |
| Stdlib | `nash/core` package in-repo (`core/`), implicit default imports like Elm's `core`. `Builtin` module exposes raw UPLC builtins. One module per type pair named by the uppercase name (`List`, `Int`, ...); functions operate on the little twin; Big twins only carry `Lift`/`ToData`/`FromData`. No Big `String`. |
| Exposing little types | `exposing (type option(..), map)` — the `type` prefix marks a lowercase type in exposing/import lists. |
| Target | Plutus V3, latest builtins (`case`/`constr`, bitwise, BLS, arrays, ledger `Value`). |
| CLI v1 | `nash check`, `nash build`, `nash test`, `nash fmt`, `nash docs`, `nash lsp`. |
| Optimizations | Inline single-use lets / small lambdas; builtin force caching; DCE + unused params; case-of-known-constructor + constant folding (via CEK). |

## Kinds and representation in one page

Kinds are Haskell 98: `Type` and `k1 -> k2`, inferred by unification.
Representation is a separate, head-determined property enforced by
predicates:

- `Big` — a Plutus `Data` value. Uppercase names: `Int`, `Bytes`, `List 'a`,
  `Map 'k 'v`, `Data`, user `type Foo`, `type alias Foo`.
- `Const` — a UPLC builtin constant. Lowercase builtins: `int`, `bytes`,
  `string`, `bool`, `unit`, `list 'a`, `pair 'a 'b`, `array 'a`, `bls_*`,
  `value`.
- `Term` — a UPLC `constr` term or a function. Lowercase user ADTs, tuples,
  little records, `'a -> 'b`.

Every constructor carries an inferred **datatype context** its arguments
must satisfy; forming the type anywhere (annotation, inference, impl
specialization) instantiates it:

| Constructor | Context |
|---|---|
| `List`, `Map` | `Big` arguments |
| Big ADT / Big record | fields `Big` |
| `list`, `array`, `pair` | `Storable` = `Big` or `Const` (never `Term`); only `mkPairData` constructs a pair |
| little ADT / tuple / little record / `->` | none |
| `type wrap 'f 'a = Wrap ('f 'a)` | `Apply 'f 'a`, reduced when `'f` is known |

`Data` is always Big; there is no `data`. `self 'f = Self ('f 'f)` is an
infinite kind, as in Haskell. Details: [kinds.md](kinds.md).

## Syntax in one page

Missing and unreachable match cases are checked with Maranget's pattern-matrix
algorithm, following Elm's `Nitpick/PatternMatches.hs`. After type solving,
exhaustiveness checking reports missing-pattern examples and usefulness
checking reports redundant branches. This is Plan 05 work; diagnostic prose
and rendering belong to Plan 06.

Literal patterns use conversion and equality traits, so they may coexist
with constructors of a user-defined type. Coverage checking treats their
values as opaque: literals do not establish constructor coverage, and it
does not evaluate trait methods to prove that distinct patterns are equal.
An identical literal or complete structural coverage can make a later
literal branch redundant. Other unknown overlaps remain potentially useful.

```elm
validator module Vesting exposing (main)

import Cardano.Tx exposing (Tx, Output)

-- Big ADT: Data Constr, fields Big
type Datum = Datum { owner : Bytes, deadline : Int }

-- little ADT: UPLC constr, fields any representation
type step 'a = Done 'a | Next int 'a

-- alias, little record: constr 0 [fields]
type alias acc = { total : int, seen : list Int }

trait Eq 'a => Ord 'a where
    compare : 'a -> 'a -> ordering

    lt : 'a -> 'a -> bool
    lt a b = compare a b == LT

impl Ord int where
    compare a b =
        if Builtin.lessThanInteger a b then LT
        else if Builtin.equalsInteger a b then EQ
        else GT

-- Big Eq is automatic and cannot be overridden.
@derive(Show, ToData, FromData)
type Redeemer = Claim | Cancel

main : Datum -> Redeemer -> Data -> unit
main datum redeemer ctx =
    case redeemer of
        Claim -> assert (lower datum.deadline < currentSlot ctx)
        Cancel -> assert (signedBy ctx datum.owner)

tests
    import Fuzz exposing (int, listOf)

    test "lt is strict" = do
        assert (not (lt 1 1))

    prop "compare is antisymmetric" =
        let
            a via int
            b via int
        in
        do
            label (if a < b then "lt" else "ge")
            assert (compare a b == invert (compare b a))

    prop "division by zero fails" fail =
        let x via int in
        do
            assert (x / 0 == 0)
```

## Pipeline

```
source ─parse─> Source AST ─canonicalize─> Can AST ─constrain/solve─> types + trait evidence
   ▲                                                                           │
   └──────────── macro expansion (typed AST in, source AST out) ◄──────────────┘
                                                                               │
                                          exhaustiveness (Nitpick) ◄───────────┘
                                                                               │
                                   Core IR (monomorphized, explicit reprs) ◄───┘
                                                                               │
                                          Core -> Core optimizations           │
                                                                               │
                                          UPLC Term ─> flat / CBOR / CEK (tests, comptime)
```

## Crate map (target)

```
crates/
  nash-region          spans
  nash-source          surface AST                       (extend: traits, tests, do, attrs, macros)
  nash-parse           parser                            (extend: same)
  nash-ast             canonical AST                     (extend: kinds, traits, evidence slots)
  nash-can             canonicalization + Haskell 98 kinds + datatype contexts
  nash-constrain       type and predicate constraint generation
  nash-solve           type/trait solving + kind contracts + defaulting + evidence
  nash-nitpick         exhaustiveness (new)
  nash-report          diagnostics: Elm prose -> miette  (new)
  nash-ir              Core IR + Core->Core passes       (new)
  nash-codegen         Can AST -> Core -> UPLC           (new)
  nash-test            test runner, fuzz driver, shrinker(new)
  nash-macro           expansion loop, Ast reification   (new)
  nash-fmt             formatter                         (new)
  nash-docs            doc generator                     (new)
  nash-plutus          UPLC runtime
  nash-config / nash-driver / nash-cli / nash-language-server
core/                  the `nash/core` package (Nash source)
```

## Component docs

- [syntax.md](syntax.md) — full surface syntax and grammar changes
- [kinds.md](kinds.md) — Haskell 98 kinds, representation predicates, datatype contexts
- [traits.md](traits.md) — traits, impls, resolution, monomorphization
- [representation.md](representation.md) — runtime layout of every type, Big/little bridging
- [data.md](data.md) — `Data` type, patterns, encoders/decoders
- [validators.md](validators.md) — validator modules, `main`, build outputs
- [testing.md](testing.md) — tests block, props, fuzzers, shrinking, power-assert
- [macros.md](macros.md) — procedural macros, `@derive`, `comptime`
- [codegen.md](codegen.md) — Core IR, lowering, pattern compilation, recursion, optimizations
- [diagnostics.md](diagnostics.md) — error reporting architecture
- [stdlib.md](stdlib.md) — `nash/core` layout, prelude, `Builtin`
- [cli.md](cli.md) — commands and outputs
