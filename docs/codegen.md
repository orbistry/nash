# Codegen — Core IR and lowering to UPLC

This document specifies the back end: the `Core` intermediate representation,
the passes that turn a type-checked Can AST into `Core`, the Core -> Core
optimizations, and the final lowering to a UPLC `Term`. Decisions here follow
[overview.md](overview.md); the runtime layout of each type is specified in
[representation.md](representation.md) and the `Data` type in
[data.md](data.md). Implementation plans: `plans/07-codegen.md` and
`plans/08-optimizer.md`.

## Purpose

Aiken generates UPLC from a stack of intermediate forms (`AirTree` ->
`Vec<Air>` -> `Term<Name>`) with types (`Rc<Type>`) attached to many nodes,
performs monomorphization by rewriting the tree in place, and defaults every
user type to `Data`. Nash does three things differently:

1. **One tree IR.** `Core` is a monomorphized, explicitly-typed lambda
   calculus. Every pass is `Core -> Core` until the last one, which is
   `Core -> Term<Name>`. There is no linearized instruction stream.
2. **Explicit kinds.** Every binder and every node carries a `Ty` whose base
   kind is `Big`, `Const` or `Term` (see [kinds.md](kinds.md)). Codegen never
   guesses a representation from a type name; it reads the kind.
3. **No `Data` by default.** A little type is never represented as `Data`.
   Conversions between representations are explicit `Cast` nodes that the
   front end inserted for `toData` / `fromData` / `validateData` / `lift` /
   `lower`. The optimizer can cancel adjacent inverse casts; it never inserts
   or removes a representation change on its own.

## The Core IR

```
Core ::= Var(name)
       | Lit(constant)                          -- a nash-plutus Constant, incl. Data
       | Lam(params, body)                      -- n-ary, curried on lowering
       | App(func, args)                        -- n-ary
       | Let(binder, value, body)               -- non-recursive, strict
       | LetRec(rec_binders, body)              -- functions only
       | Case(kind, scrutinee, branches, default)
       | Constr(tag, fields)                    -- UPLC `constr`
       | Field(record, index)                   -- projection out of a `constr`
       | Builtin(fn, args)                      -- saturated or partial builtin call
       | Cast(kind, from_ty, to_ty, arg)
       | Trace(msg, body)
       | Error
       | Delay(body) | Force(body)
```

Every binder is `Binder { name: Name, ty: Ty }` where `Name { text, unique }`
is globally unique after the hygiene pass (see `plans/08-optimizer.md`,
chunk 1). `Ty` is a fully monomorphic type with its base kind exposed:

```rust
pub enum Ty<'a> {
    Big(&'a BigTy<'a>),     // Int, Bytes, Data, List t, Map k v, Big ADT, Big record
    Const(&'a ConstTy<'a>), // int, bytes, string, bool, unit, list t, pair a b, array t, bls_*, value
    Term(&'a TermTy<'a>),   // little ADT, tuple, little record, function
}
```

`Ty::kind()` is the only thing most passes need. `BigTy` records enough
structure to generate `validateData` for the type (constructor count and
field types); `TermTy` records constructor arities so `Case` and `Field` can
be lowered; `ConstTy` maps one-to-one onto nash-plutus `typ::Type` so
literals can be built (`crates/nash-plutus/src/typ.rs`).

### Node semantics

| Node | Meaning | Lowers to |
|---|---|---|
| `Var` | local or top-level variable | `Term::Var` |
| `Lit` | a UPLC constant | `Term::Constant` |
| `Lam(ps, b)` | n-ary function | nested `Term::Lambda` |
| `App(f, as)` | n-ary application | nested `Term::Apply` |
| `Let(b, v, e)` | strict binding | `(\b -> e) v` |
| `LetRec` | recursive function group | self-application, see Recursion |
| `Case(Tag, s, bs, d)` | match on a `Term` constr tag; branch `i` binds the fields | `Term::Case` |
| `Case(Bool, s, [t, e], _)` | `if` | `force (ifThenElse s (delay t) (delay e))` |
| `Case(Int, s, bs, d)` | switch on integer literals | chain of `equalsInteger` + `ifThenElse` |
| `Case(Bytes, s, bs, d)` | switch on bytestring literals | chain of `equalsByteString` |
| `Case(List, s, [nil, cons], _)` | match on a `Const` list; `cons` binds head and tail | `chooseList` + `headList`/`tailList` |
| `Case(Data, s, bs, d)` | match on the `Data` tag; five branches `Constr\|Map\|List\|I\|B` | `chooseData` with delayed branches |
| `Constr(i, fs)` | build a UPLC constr | `Term::Constr` |
| `Field(r, i)` | project field `i` of a constr | `case r [\f0 .. fn -> fi]` |
| `Builtin(f, as)` | call builtin `f`; `as.len() <= f.arity()` | `force^k (builtin f)` applied to `as` |
| `Cast` | representation change | see Casts |
| `Trace(m, b)` | log `m` then evaluate `b` | `force (trace m (delay b))` |
| `Error` | abort | `Term::Error` |
| `Delay`/`Force` | explicit laziness | `Term::Delay` / `Term::Force` |

`Case` branch binders come from the node, not from nested lambdas: a `Tag`
branch is `(tag, &[Binder], body)`, a `List` cons branch binds `(head, tail)`,
a `Data` `Constr` branch binds `(tag: int, fields: list Data)`. Keeping the
binders in the node lets the decision-tree compiler and the optimizer treat
them uniformly without pattern-matching on lambda shapes.

`Builtin` carries the `DefaultFunction` from
`crates/nash-plutus/src/builtin/default_function.rs`; arity and force count
come from `DefaultFunction::arity()` and `DefaultFunction::force_count()`.
A `Builtin` with fewer arguments than its arity is a partial application and
lowers to the same term; a `Builtin` with zero arguments is the (forced)
builtin value itself. `Builtin` never has more arguments than its arity: the
front end wraps excess arguments in an outer `App`.

### Casts

```rust
pub enum CastKind {
    ToData,           // Big -> Data: identity at runtime
    FromDataShallow,  // Data -> Big: check the outermost node, then identity
    ValidateData,     // Data -> Big: full recursive check, then identity
    Lift,             // Const -> Big  (iData, bData, listData, mapData)
    Lower,            // Big -> Const  (unIData, unBData, unListData, unMapData)
}
```

`ToData` is erased during lowering. `FromDataShallow` and `ValidateData`
lower to a call of a compiler-generated checker function for the target
type (one per Big type and check depth, hoisted as top-level `LetRec`
bindings named `fromData#T` and `validateData#T`, see
[data.md](data.md)), followed by the value itself. `Lift`/`Lower` lower to the
single builtin that converts between the two representations (`iData`,
`bData`, `listData`, `mapData` and their inverses), or to nothing for the
built-in reflexive `Lift 'a 'a` on Big types. Only those `core/` impls
become `Cast` nodes; the other stdlib impls (`list` with element
conversion, `option`, `result`, `ordering`, see [data.md](data.md)) and
user-written impls are ordinary functions.

## Pipeline

```
Can AST + solved types + trait evidence
   │ 1. monomorphization worklist         (Can -> Core, one instance per key)
   │ 2. trait methods -> impl bodies      (folded into 1)
   │ 3. pattern matching -> decision trees
   │ 4. desugar do / records / tuples / lists   (folded into 1 and 3)
   │ 5. recursion rewrite                 (LetRec -> self-application)
   ▼
Core
   │ 6. Core -> Core optimization passes  (plans/08-optimizer.md)
   ▼
Core
   │ 7. Core -> Term<Name> -> Term<DeBruijn> -> Program
   ▼
UPLC
```

Phases 1–4 run in one traversal in `nash-codegen` (`Can -> Core`), phase 5
is a Core pass in `nash-codegen`, phase 6 lives in `nash-ir`, phase 7 in
`nash-codegen`.

### 1. Monomorphization worklist

Input: the Can `Module`s of the build (`crates/nash-ast/src/lib.rs`), the
solved annotation per top-level value (`nash_can::Annotations`, produced by
`nash_solve::run` in `crates/nash-solve/src/solve.rs:22`), and the per-use
instantiation types and evidence that [plans/03-traits.md](../plans/03-traits.md) adds to the solver
output (each `Expr::VarTopLevel` / `VarForeign` / `VarOperator` /
`Binop` occurrence gets its instantiated ground type and a slice of resolved
impls).

The worklist holds keys:

```rust
pub struct MonoKey<'a> {
    pub name: QualifiedName<'a>,
    pub type_args: &'a [Ty<'a>],         // ground, in `Annotation.free_vars` order
    pub evidence: &'a [Evidence<'a>],    // ground, one per context predicate
}
```

`Evidence` is `nash_ast::Evidence::{Impl { impl_, type_args, args }, Given, Super}`
from [plans/03-traits.md](../plans/03-traits.md) ("Contract with
plans/07-codegen.md"). Before a key is formed, every `Given` is replaced by
the evidence of the enclosing specialization and every `Super` is resolved
through the impl table, so a key holds only `Impl` trees; `Evidence`
derives `Hash`/`Eq` for this.

Starting from the roots (`main` for a validator, each test body for a test
module, the `comptime` subterm for compile-time evaluation), the driver pops a
key, instantiates the definition's body with the type substitution, rewrites
every trait-method use to the method of the impl named in the evidence
(phase 2), and pushes every new key it meets. Each key is instantiated once.
Instances are named `text#variant` where `variant` is the rendering of the
type arguments (Aiken: `get_generic_variant_name` in
`crates/aiken-lang/src/gen_uplc/builder.rs:178`), so `List.map#int#Int`
and `List.map#Int#Int` are distinct `Core` bindings.

Evidence is a function of the ground type arguments under coherence (Rust
orphan rules, see [traits.md](traits.md)), so the key could omit it; it is
kept because the instantiation needs it and because superclass evidence is
cheaper to carry than to re-derive.

Nash's ordering differs from Aiken's: Aiken builds the whole `AirTree` with
generic types and monomorphizes afterwards (`builder::monomorphize`,
`gen_uplc.rs` `hoist_functions_to_validator`). Nash instantiates while
building, so `Core` is never polymorphic.

### 2. Trait method calls

A use of a trait method `Ord.compare` at type `int` with evidence
`Impl { impl_: Ord int, type_args: [], args: [] }` becomes
`Var(compare#Ord#int)`, whose definition is the impl's method body
instantiated at `type_args` (or the trait's default method body with the
impl's evidence substituted). `args` supplies the evidence for the impl's
own context (`impl Eq 'a => Eq (list 'a)`), and a `Super` node (`lt` using
`Eq` through `Ord`'s superclass) resolves to the superclass impl through
the impl table. After this phase there are no dictionaries and no trait
names in `Core`.

### 3. Pattern matching

`Expr::Case`, `Expr::LetDestruct`, multi-clause function arguments and
lambda argument patterns all go through one compiler that produces a
Maranget decision tree, ported from Aiken's
`crates/aiken-lang/src/gen_uplc/decision_tree.rs`:

- Rows are built by `map_pattern_to_row`; variable and alias patterns are
  split out as assignments, tuple and record patterns are expanded in place
  (they are irrefutable), everything else becomes a column keyed by a
  `Path` (`Tuple(i)`, `Constr(i)`, `BigField(i)`, `ListHead(i)`,
  `ListTail(i)`, `DataConstrTag`, `DataConstrFields`, ...).
- `highest_occurrence` picks the column with the most non-wildcard tests
  before the first wildcard, and `do_build_tree` specializes on it,
  producing `Switch { path, cases, default }` and `ListSwitch` for list
  patterns of differing lengths.
- Leaves are **hoisted**: each right-hand side is emitted once as a
  `Let`-bound lambda over the variables its pattern binds and placed at the
  lowest common ancestor scope of its uses (`get_hoist_paths`,
  `hoist_by_path`). A leaf reached from one place is inlined by the
  optimizer.
- Accessor paths are **memoized**: the projection chain that reaches a
  `Path` (`unConstrData`, `sndPair`, `tailList`..., `headList`) is bound to
  a name once and reused by every test and leaf below it. This ports Aiken's
  `stick_break_set.rs` (`Builtins::new_from_path`,
  `TreeSet::diff_union_builtins`) with `Core` `Let`s instead of `AirTree`
  `let_assignment`s.

The `Switch` node lowers to the `Case` kind matching the scrutinee's `Ty`:

| Scrutinee kind / type | `Case` kind | Test |
|---|---|---|
| little ADT (`Term`) | `Tag` | UPLC `case` on the constr |
| `bool` | `Bool` | `ifThenElse` |
| `int`, `bytes` literals | `Int`, `Bytes` | equality chain |
| `list 'a` | `List` | `chooseList` |
| Big ADT | `Int` on `fstPair (unConstrData s)` | equality chain on the tag |
| `Data` | `Data` | `chooseData` |
| Big record, `List 'a`, `Map 'k 'v` | none (irrefutable) | projection only |

Exhaustiveness is checked earlier by `nash-nitpick` (Elm's
`Nitpick/PatternMatches`), so `default` is `None` for a complete match and the
tree never needs a compiler-generated fallthrough. When a match is not
exhaustive the front end has already reported an error.

### 4. Desugaring

Handled inline while building `Core`:

- `do` blocks are `Monad.bind` chains (see [syntax.md](syntax.md)); they
  reach codegen as ordinary calls and resolve through phase 2.
- Records: a Big record literal is `Builtin(ListData, [cons chain])`; a
  little record literal is `Constr(0, fields)`. Field access is
  `Field(r, i)` for little records and
  `Builtin(HeadList, [tail^i (unListData r)])` for Big records. Record update
  binds the base once and rebuilds every field. `Expr::Accessor` becomes a
  `Lam`.
- Tuples are `Constr(0, ...)` and `Field`.
- `Expr::List` is a `Const` list: a literal of constants is one `Lit`; a
  list with computed elements is a `mkCons` chain onto a `Lit` nil of the
  right element type. A `List 'a` literal (Big) is the `Const` list wrapped
  in `Builtin(ListData)`.
- `Expr::If` with several branches is nested `Case(Bool)`.
- Negation has no node of its own: canonicalization turns `-e` into a
  `Num.negate` method call ([plans/03-traits.md](../plans/03-traits.md)),
  which phase 2 resolves to the impl's body (`subtractInteger 0 e` for
  `int`).
- `Expr::Unit` is `Lit(Unit)`. `Expr::Str` is a `Const` `string` literal
  when the solved type is `string`, and a `bytes` literal when it is `bytes`
  (see literal defaulting in [traits.md](traits.md)).

### 5. Recursion

`Decls::DeclareRec` and `Expr::LetRec` are the only sources of recursion;
canonicalization already computed the SCCs.

**Self recursion** uses self-application, ported from Aiken's
`modify_self_calls` and `identify_recursive_static_params`
(`crates/aiken-lang/src/gen_uplc/builder.rs:256-408`) and the
`FunctionVariants::Recursive` lowering (`gen_uplc.rs:4607`):

1. Walk the body. A parameter is *static* if every self call passes it
   through unchanged and the function is never used other than as the head
   of a call (`calls == usages`). Otherwise it is *non-static*.
2. Rewrite each self call `f a1 .. an` to `(f f) [non-static args]`.
3. Emit
   ```
   f = \static.. -> (\f -> (f f) nonstatic..)          -- outer, all params
                       (\f nonstatic.. -> body')       -- inner, self-applying
   ```
   With no static params the outer wrapper collapses to
   `f = (\f -> f f) (\f nonstatic.. -> body')`, and a function with no
   parameters gets a `Delay`/`Force` pair so the self-application does not
   loop at definition time.

**Mutual recursion** (`DeclareRec` with `following` non-empty) uses a
combined dispatcher, ported from `modify_cyclic_calls`
(`builder.rs:410`) and the `FunctionVariants::Cyclic` lowering:

```
cycle = \cycle -> \select -> select (\a.. -> bodyA') (\b.. -> bodyB')
A x   ==>  cycle cycle (\a b -> a) x
B y   ==>  cycle cycle (\a b -> b) y
```

inside the bodies, and the same shape at outside call sites. `LetRec` keeps
`static_params: &[u16]` per binder so the optimizer's unused-parameter pass
does not undo the static lifting.

No Y combinator is ever emitted.

### 6. Optimizations

Specified in `plans/08-optimizer.md`:

- **Inline** single-use `Let`s and small lambdas (Aiken `inline_reducer`,
  `lambda_reducer` in `crates/uplc/src/optimize/shrinker.rs`).
- **Builtin force caching**: hoist `force (builtin f)` for every forced
  builtin to one binding at the program root, and curry constant first
  arguments (Aiken `builtin_force_reducer`, `builtin_curry_reducer`).
- **DCE + unused params**: drop unreferenced `Let`/`LetRec` bindings and
  parameters that no call site needs.
- **Case-of-known-constructor + constant folding**: `Case` on a `Constr`
  or `Lit` picks the branch; a closed `Builtin` application whose arguments
  are all `Lit` is evaluated on the nash-plutus CEK machine
  (`Program::eval`, `crates/nash-plutus/src/program.rs:38`) and replaced by
  the resulting constant when the builtin is error-safe on those arguments
  (Aiken `builtin_eval_reducer`, `is_error_safe`). Adjacent inverse casts
  (`unIData (iData x)`) cancel (Aiken `cast_data_reducer`).

The passes run to a fixed point on node count (Aiken
`optimize_repeatedly`, `crates/uplc/src/optimize.rs:9`).

### 7. Lowering to UPLC

`Core -> Term<Name>` is a direct structural translation using the
constructors in `crates/nash-plutus/src/term.rs` (`Term::lambda`,
`Term::apply`, `Term::constr`, `Term::case`, `Term::builtin`, ...) into a
nash-plutus `Arena`. `Name` is `crates/nash-plutus/src/binder/name.rs`
(`text` + `unique`); the `unique` comes straight from the `Core` `Name`.
A separate step converts `Term<Name>` to `Term<DeBruijn>`
(`crates/nash-plutus/src/binder/debruijn.rs`) for evaluation and flat
encoding; `Program::new(arena, Version::plutus_v3(arena), term)` wraps it.

n-ary `Lam`/`App` become nested unary terms. A `Builtin(f, args)` becomes
`force^{force_count} (builtin f)` applied to the arguments; the force-caching
pass has usually already replaced the head with a variable.

## How each Nash type lowers

| Nash type | Kind | Runtime value | Build | Take apart |
|---|---|---|---|---|
| `int` `bytes` `string` `bool` `unit` | Const | constant | `Lit` | builtins |
| `list 'a` (`'a` Storable) | Const | `list t` constant | `mkCons` / `Lit []` | `chooseList` `headList` `tailList` |
| `pair 'a 'b` (Big elements) | Const | `pair data data` | `mkPairData` | `fstPair` `sndPair` |
| `array 'a` | Const | `array t` | `listToArray` | `indexArray` `lengthOfArray` |
| `bls_g1` `bls_g2` `bls_mlr` `value` | Const | constant | builtins | builtins |
| `Int` | Big | `data (I n)` | `iData` | `unIData` |
| `Bytes` | Big | `data (B bs)` | `bData` | `unBData` |
| `Data` | Big | `data` | any | `chooseData` |
| `List 'a` | Big | `data (List xs)` | `listData` | `unListData` |
| `Map 'k 'v` | Big | `data (Map kvs)` | `mapData` | `unMapData` |
| Big ADT `type Foo = A .. \| B ..` | Big | `data (Constr i fields)` | `constrData i fields` | `unConstrData`, `fstPair`, `sndPair`, list indexing |
| Big labeled ctor `type Datum = Datum { owner : Bytes, deadline : Int }` | Big | `data (Constr i [owner, deadline])` | `constrData i fields` | same as a Big ADT |
| Big record `type alias Foo = {..}` | Big | `data (List fields)` | `listData` | `unListData`, list indexing |
| little ADT `type foo = ..` | Term | `constr i [fields]` | `Constr` | `Case(Tag)` |
| little labeled ctor `type step = Next { n : int, rest : step }` | Term | `constr i [n, rest]` | `Constr` | `Case(Tag)`, `Field` |
| tuple, little record | Term | `constr 0 [fields]` | `Constr(0)` | `Field`, `Case(Tag)` |
| function | Term | closure | `Lam` | `App` |

`constrData` takes an `int` tag and a `list data` of fields, so a Big ADT
value is `Builtin(ConstrData, [Lit i, fields])` where `fields` is a `Const`
list of the (already Data) field values.

**Labeled constructor fields** (`Datum { owner : Bytes, deadline : Int }`,
Aiken style) are positional fields with names (`CtorArgs::Labeled`,
[representation.md](representation.md)): the labels exist only at
compile time, canonicalization rewrites labeled patterns and constructor
calls to wire-order positional form, and the encoding is flat, `Constr i [owner, deadline]` for a
Big type and `constr i [owner, deadline]` for a little one. There is no
nested record. On a single-constructor type `.owner` access lowers to the
same field extraction a pattern `Datum { owner }` produces, through the
same memoized accessor path. Only `type alias` records lower to a `List`. Elm's `CtorOpts::Enum` and
`CtorOpts::Unbox` (`crates/nash-ast/src/lib.rs:118`) are ignored for Big
types because the Data layout is the on-chain ABI. For little ADTs `Enum`
changes nothing (a nullary constructor is `constr i []`); `Unbox` is not
applied in v1 (see Open questions).

## Case on a Big ADT

```elm
case datum of
    Datum { owner, deadline } -> deadline
```

produces, before optimization,

```
let p      = unConstrData datum          -- pair int (list data)
let tag    = fstPair p
case tag of
  0 -> let fields   = sndPair p
       let deadline = headList (tailList fields)
       deadline
```

The `Int` case on `tag` disappears when the type has one constructor (the
front end guarantees exhaustiveness, so a single-constructor match needs no
test). Field extraction is lazy in the sense that `headList`/`tailList`
chains are emitted at the leaf that needs them, shared through the memoized
accessor set; unused fields are never touched. The pair projection and the
`unConstrData` are shared across constructors of the same match.

## Case on a little ADT

```elm
case step of
    Done a   -> a
    Next n a -> a
```

lowers to one UPLC `case`:

```
case step [ (\a -> a), (\n a -> a) ]
```

Each branch is a lambda over the constructor's fields in declaration order,
even if the branch ignores them. `Field(r, i)` is `case r [\f0 .. fn -> fi]`
and is what tuple projection and little-record access compile to.

## `if` on `bool`

`Case(Bool, c, [t, e])` lowers to `force (ifThenElse c (delay t) (delay e))`.
The optimizer removes the `delay`/`force` pair around a branch that is a
variable, literal, lambda or builtin (it cannot fail and is cheap to
evaluate eagerly), which is the shape Aiken's shrinker also targets.

## Records

Big record access is `headList (tailList^i (unListData r))`; the memoized
accessor set shares the `unListData` and the `tailList` prefix across fields
read in one scope. Little record access is `Field`. On a single-constructor
type with labeled fields, `r.x` is not a record access: it lowers to the
constructor's field extraction (`sndPair (unConstrData r)` then list
indexing for Big, `Field` for little), exactly as the pattern
`Ctor { x }` does. Record update
`{ r | x = e }` becomes

```
let base = r
Constr(0, [Field base 0, e, Field base 2, ..])       -- little
listData [headList (unListData base), e, ..]         -- Big
```

## Runtime errors and traces

| Surface | Core | Notes |
|---|---|---|
| `fail "msg"` | `Trace(Lit "msg", Error)` | message subject to trace level |
| `fail` | `Error` | |
| `todo "msg"` | `Trace(Lit "TODO: msg", Error)` | also a compile warning |
| `trace "msg" e` | `Trace(Lit "msg", e)` | |
| `assert c` | `Case(Bool, c, [Lit (), Trace(msg, Error)])` | `msg` is the power-assert rendering built at compile time (see [testing.md](testing.md)) |

Trace levels are a build setting (`nash.jsonc`, [cli.md](cli.md)):

- `silent`: every user `Trace(m, b)` becomes `b`; `fail "msg"` becomes
  `Error`.
- `compact`: the message is replaced by `Module:line:col` of the
  originating expression.
- `verbose`: the message is kept verbatim.

Compiler-generated traces ("validateData: field 1 of Datum",
"incomplete pattern match", "validator returned false") are controlled by a
separate boolean switch, `compilerTraces`, so a user can ship verbose user
traces without the compiler's, or the reverse. In Aiken both are one
`TraceLevel` (`crates/aiken-lang/src/ast.rs:2305`, used in
`gen_uplc.rs:445`).

Trace strings are hoisted: each distinct message becomes one top-level
`Let` of a `string` constant so the program does not repeat it.

## Validators

For `validator module Foo exposing (main)`, `main`'s arguments become
lambdas in order and its body is the program body. The caller applies
UPLC constants to the program, so an argument of `main` may be of kind
`Big` (a `Data` constant, what the ledger passes) or `Const` (any other
UPLC constant, for parameterized scripts and tests). An argument of kind
`Term` (a function, a little ADT, a tuple) is a compile error, "nothing
outside the script can supply this", reported before codegen (see
[validators.md](validators.md) and [plans/09-validators-build.md](../plans/09-validators-build.md)).
No boundary conversion is inserted for either kind: a Big value *is* its
`Data`, and a Const value is the constant itself.
`main : Datum -> Redeemer -> Data -> unit` lowers to
`\datum redeemer ctx -> body`. Pattern matches inside `body` are what check
the shape; a `validateData` call is the user's choice.

The result type is free. Success is "evaluation did not error", so a `bool`
result is **not** checked; `assert` is the idiom for a condition. (Aiken
wraps the body in `wrap_validator_condition`, `builder.rs:1214`; Nash does
not.)

The program is `Program { version: 1.1.0, term }` for Plutus V3.

## Tests

Each `test` / `prop` in the `tests` block compiles to its own `Program`. The
module's `Core` bindings are built and optimized once (monomorphized from
the union of all test roots), and each test program is assembled from the
bindings reachable from its own body, so shared code is compiled once and
DCE is per program. A `test` compiles to one program of type `unit`:
success is no error. A `prop` compiles to two programs following the
runner protocol in [testing.md](testing.md):

```
draw : Prng -> option (Prng, list string)
run  : Prng -> option Prng
```

`draw` threads the PRNG through the `via` generators and returns the shown
values; `run` draws the same values, evaluates the body with them in scope,
and returns the next PRNG. The drawn values never cross the program
boundary (they may be of any kind), so the body is compiled together with
the generators, and `nash-test` only ever applies a `Prng` as `Data`.

## Comptime hook

`comptime e` reaches codegen as a marked subterm. After monomorphization
the subterm is closed (it may reference top-level bindings, which are
included), so it is lowered on its own, evaluated with `Program::eval`, and
the resulting `Term::Constant` becomes a `Lit` in the enclosing `Core`.
Evaluation errors and non-constant results are compile errors. Constant
folding uses the same function on any closed `Builtin` subterm, so the
comptime hook is not a special path. Macros (see [macros.md](macros.md))
use the same CEK machine but not the constant rule: the `Ast` family is
`Term` kind, so the host applies the macro program to a `Term::Constr`
tree and reads the output `Ast` from the result `Value`, never through
`Data`.

## Interactions

- **Kinds** ([kinds.md](kinds.md)): `Ty::kind()` decides every
  representation choice; codegen never inspects casing.
- **Traits** ([traits.md](traits.md)): evidence drives phase 2; literal
  traits (`FromInt` ...) resolve to `Lit` or to a `Lift` cast.
- **Data** ([data.md](data.md)): `Cast` lowering and the checker
  functions.
- **Nitpick**: exhaustiveness is assumed; decision trees have no
  fallthrough of their own.
- **Testing** ([testing.md](testing.md)): power-assert messages are built by
  the front end and reach codegen as string literals.
- **nash-plutus**: `Term`, `Constant`, `PlutusData`, `DefaultFunction`,
  `Program::eval`, flat encoding.

## Open questions

1. **`Unbox` for little ADTs.** Elm unboxes single-constructor,
   single-field types. Doing the same for a little ADT saves a `constr`
   allocation and a `case` per access. Deferred to after v1.
2. **nash-plutus lacks a `Term` pretty printer and `Name -> DeBruijn`
   conversion.** The syn module only parses. `plans/07-codegen.md` chunk 2
   adds both to nash-plutus (ports of Aiken `crates/uplc/src/pretty.rs` and
   `crates/uplc/src/debruijn.rs`).
