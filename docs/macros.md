# Macros and compile-time evaluation

Nash has procedural macros and a `comptime` expression form. Both run Nash
code on the CEK machine inside the compiler. Macros transform *typed*
canonical AST into *surface* AST that is spliced back into the module and
re-checked. `comptime` evaluates a closed expression to a UPLC constant.

Decisions here follow [overview.md](overview.md): typed input, surface
output, hygienic, `@derive(..)` on declarations, `name!(args)` in
expressions, expand-then-recheck loop per module, deriving implemented as
macros.

## Concepts

| Term | Meaning |
|---|---|
| Macro | A top-level function declared with `macro`, of one of two fixed shapes (declaration macro or expression macro). |
| Invocation | `@name(args)` before a declaration, or `name!(args)` in an expression. |
| Reification | Turning compiler AST into `Ast.*` values (Big types, so they are `Data` at runtime) and back. |
| Expansion round | One pass: find every invocation in a module, run each macro, splice results. |
| Hygiene | Binders created by a macro cannot capture or be captured by user names unless the macro asks for it with `Ast.raw`. |
| Comptime | `comptime e`: evaluate `e` on the CEK machine at compile time, splice the resulting constant. |

## Declaring a macro

A macro is an annotated top-level definition whose annotation line starts
with `macro`. The annotation is mandatory. It fixes the macro's shape.

```elm
module Derive exposing (derive)

import Ast exposing (Decl, Expr)

macro derive : Decl -> List Expr -> List Decl
derive decl traits =
    lift (List.map (deriveOne decl) (lower traits))
```

Macro arguments and results are Big lists (`List Ast.Expr`), the same
type as the lists inside `Ast` nodes. Macro code `lower`s them to
`list` at entry and `lift`s the result (stdlib.md).

The two shapes:

```elm
-- declaration macro: the decorated declaration, then the attribute arguments
macro name : Ast.Decl -> List Ast.Expr -> List Ast.Decl

-- expression macro: the call arguments
macro name : List Ast.Expr -> Ast.Expr
```

Any other annotation on a `macro` line is an error (`MacroBadShape`).

Rules:

- A macro lives in an ordinary module and is exported like any value.
- A macro cannot be invoked in the module that defines it. The defining
  module must be fully compiled to UPLC before the macro can run. Using it
  in the same module is an error (`MacroSameModule`).
- A macro body is ordinary Nash. It may call other functions, use `case`,
  `let`, traits, and other modules' macros (transitively, those modules are
  compiled first because they are imports).
- Macros are compiled to standalone UPLC programs by `nash-codegen` and
  cached per build. See [plans/11-macros-comptime.md](../plans/11-macros-comptime.md).

### Grammar

```ebnf
macro_decl      = 'macro' lower_var ':' type_expr definition ;
definition      = lower_var { pattern } '=' expression ;   (* same name, next fresh line *)
```

## Invoking a macro

### Declaration attributes

```elm
@derive(Eq, Ord, Show, ToData, FromData)
type Redeemer = Claim | Cancel

@inline
double x = x + x
```

- Attributes precede a declaration. Several attributes may stack; they run
  in source order, each receiving the output of the previous one for the
  same declaration.
- `name` resolves like a value: it must be in scope (unqualified or
  qualified, `@Derive.derive(Eq)` is valid). It must resolve to a
  declaration macro.
- The arguments are parsed as expressions but **not** type checked. They
  are not in any scope. `Eq` is passed as `Ast.Expr` whose node is
  `Var (Raw "Eq")` with `typ = None`. The macro interprets them.
- The macro receives the decorated declaration as a **typed** `Ast.Decl`
  (see "What the macro sees").
- The result `List Ast.Decl` **replaces** the decorated declaration. To
  keep the original, return it as the first element. `derive` returns
  `[decl, impl1, impl2, ...]`.

```ebnf
attribute        = '@' ( lower_var | qualified_var ) [ '(' [ expression { ',' expression } ] ')' ] ;
declaration      = { attribute } ( value_decl | type_decl | alias_decl | trait_decl | impl_decl ) ;
```

Attributes on a `tests` block, on an `import`, or on an `infix` are errors.

### Expression macros

```elm
total = sum!(a, b, c)

check = assertEq!(lhs, rhs)
```

- `name!(args)` is a `term`. It binds like a call.
- The arguments **are** type checked in the surrounding scope before the
  macro runs. The macro sees each argument with its inferred type.
- The invocation itself has a fresh type during the pre-expansion pass.
  After splicing, the module is re-checked and the real type is inferred.

```ebnf
macro_call       = ( lower_var | qualified_var ) '!' '(' [ expression { ',' expression } ] ')' ;
term             = ... | macro_call | comptime_expr | quote_expr | splice ;
```

## What the macro sees: the `Ast` module

`nash/core` ships an `Ast` module. Every type in it is Big, so a value is a
`Data` constant at runtime and can be handed to and from the CEK machine
without conversion. One family of types serves both input (typed) and
output (surface). Type information lives in optional slots that are
`Some` on input and are ignored on output. Text (names, string literals,
labels) is `Bytes` holding UTF-8: there is no Big `String`, and
`impl Lift string Bytes` (`encodeUtf8`/`decodeUtf8`) converts to and from
the little `string`.

```elm
module Ast exposing (..)

type alias Span = { startRow : Int, startCol : Int, endRow : Int, endCol : Int }

-- How a name resolves after splicing.
type Name
    = Local Bytes             -- hygienic: renamed per expansion (see Hygiene)
    | Raw Bytes               -- spliced verbatim, resolves at the invocation site
    | Global Module Bytes     -- fully qualified, resolves regardless of imports

type alias Module = { package : Option Bytes, name : Bytes }

type Kind
    = Big
    | Const
    | Term
    | Storable                -- Big or Const, for `list`/`array` elements
    | Any
    | Arrow Kind Kind
    | KindVar Bytes

type alias Meta = { span : Option Span, typ : Option Type }

type Expr = Expr Meta ExprNode

type ExprNode
    = Int Int
    | Str Bytes                        -- UTF-8
    | Bytes Bytes
    | Var Name
    | Op Name                          -- operator used as a value: (+)
    | List (List Expr)
    | Negate Expr
    | BinOp Name Expr Expr             -- resolved to the operator's function name
    | Lambda (List Pattern) Expr
    | Call Expr (List Expr)
    | If Expr Expr Expr
    | Let (List Def) Expr
    | Case Expr (List Arm)
    | Accessor Bytes
    | Access Expr Bytes
    | Update Name (List FieldAssign)
    | Record (List FieldAssign)
    | Unit
    | Tuple (List Expr)                -- length >= 2
    | MacroCall Name (List Expr)       -- output may contain new invocations
    | Comptime Expr

type Def
    = Define Name (List Pattern) Expr (Option Type)
    | Destruct Pattern Expr

type alias Arm = { pattern : Pattern, body : Expr }
type alias FieldAssign = { field : Bytes, value : Expr }

type Pattern = Pattern Meta PatternNode

type PatternNode
    = PAny
    | PVar Name
    | PRecord (List Name)
    | PAlias Pattern Name
    | PUnit
    | PTuple (List Pattern)
    | PCtor Name (List Pattern)
    | PList (List Pattern)
    | PCons Pattern Pattern
    | PInt Int
    | PStr Bytes
    | PBytes Bytes

type Type
    = TVar Bytes
    | TCon Name (List Type)
    | TFun Type Type
    | TRecord (List Field)
    | TTuple (List Type)
    | TUnit

type alias Field = { name : Bytes, typ : Type }
type alias Param = { name : Bytes, kind : Option Kind }
type alias Constraint = { trait : Name, args : List Type }

type Decl
    = Value { name : Name, args : List Pattern, body : Expr, annotation : Option Type }
    | Union { name : Name, params : List Param, kind : Option Kind, ctors : List Ctor }
    | Alias { name : Name, params : List Param, kind : Option Kind, typ : Type }
    | Trait Trait
    | Impl Impl
    | Infix { op : Bytes, assoc : Assoc, prec : Int, function : Name }

type alias Ctor = { name : Name, args : List Type }
type Assoc = LeftAssoc | RightAssoc | NonAssoc

type alias Trait =
    { name : Name
    , params : List Param
    , supers : List Constraint
    , methods : List Method
    }

type alias Method = { name : Name, typ : Type, default : Option Expr }

type alias Impl =
    { trait : Name
    , args : List Type
    , context : List Constraint
    , defs : List Def
    }
```

Input conventions (encoder, `nash-macro`):

- Every `Expr` and `Pattern` has `span = Some` and `typ = Some` (the
  solved type). `Union`/`Alias` carry `kind = Some`.
- Local variables and their binders are `Raw "x"`. Copying an input
  subtree into output keeps it resolving as the user wrote it.
- Top-level, foreign, constructor, and operator references are
  `Global module name`. `BinOp` carries the operator's *function* name.
- `if` chains are nested `If`. `let` with several definitions is one `Let`
  with a `List Def` in source order. Elm's `LetRec`/`LetDestruct` fold
  into that list.
- `do` blocks arrive desugared (`Call (Var (Global Monad "bind")) ...`).
- `Alias` types are fully expanded in `typ` slots (`Ast.Type` has no alias
  node); the alias *declaration* is still visible as `Decl.Alias`.

Output conventions (decoder):

- `span` is ignored. Every spliced node gets the invocation site's region
  so diagnostics point at the `@derive(..)` or `name!(..)`.
- `typ` is ignored.
- `Name` decides resolution as described under Hygiene.
- `Union.kind`/`Alias.kind` are ignored; the name's casing decides.

`Ast` also exports builders so macro code does not spell every `Meta`.
Builders take little values (`string`, `list`, `int`) and `lift` them
into the Big node fields, so macro code works on little types and only
touches `lift`/`lower` at the macro's own boundary:

```elm
expr : ExprNode -> Expr                    -- span = None, typ = None
pat : PatternNode -> Pattern
name : string -> Name                      -- Local
raw : string -> Name                       -- Raw
var : Name -> Expr
int : int -> Expr
str : string -> Expr
call : Expr -> list Expr -> Expr
lambda : list Pattern -> Expr -> Expr
case_ : Expr -> list Arm -> Expr
tuple : list Expr -> Expr
arm : Pattern -> Expr -> Arm
pvar : Name -> Pattern
pctor : Name -> list Pattern -> Pattern
ptuple : list Pattern -> Pattern
wildcard : Pattern
tcon : Name -> list Type -> Type
tvar : string -> Type
constraint : Name -> list Type -> Constraint
impl : Name -> list Type -> list Constraint -> list Def -> Decl
def : Name -> list Pattern -> Expr -> Def
and : list Expr -> Expr                    -- folds with (&&), `True` when empty
nameText : Name -> string
exprName : Expr -> option string           -- `Some "Eq"` for `Var (Raw "Eq")`
```

## `quote` and splices

Building trees by hand is verbose. `quote (e)` is parser sugar that turns
an expression into the `Ast.Expr` value that builds it. `~x` inside a quote
splices an `Ast.Expr` value; `~(e)` splices the result of an expression of
type `Ast.Expr`.

```elm
eqField : Ast.Name -> Ast.Name -> Ast.Expr
eqField x y =
    quote (~(Ast.var x) == ~(Ast.var y))
```

```ebnf
quote_expr       = 'quote' '(' expression ')' ;
splice           = '~' lower_var | '~' '(' expression ')' ;
```

Semantics:

- `quote` is resolved during canonicalization of the *macro's* module.
  Free names inside the quote resolve there and become `Global` names.
  `==` above becomes `BinOp (Global {nash/core} Eq "eq")` (the method
  the `Prelude` infix binds to). The invocation site does not need to
  import `Prelude` or `Eq` items for the spliced code to work.
- Binders introduced inside the quote (`\x ->`, `let y =`, pattern
  variables) become `Local`, so they are hygienic.
- A splice may appear anywhere an expression may appear inside the quote.
  Splicing into pattern, type, or argument-list positions is not supported
  in v1; use builders (`Ast.call f args`, `Ast.pvar`).
- A `quote` may not contain a `quote`. A splice outside a quote is a parse
  error (`SpliceOutsideQuote`).
- `quote` has type `Ast.Expr`. It is only useful in modules that import
  `Ast`; using it elsewhere is a normal "unknown type" error.

`quote type (t)`, `quote pattern (p)`, and `quote decl (d)` are planned
later chunks with the same shape.

## Hygiene

Names in macro output carry a resolution mode (`Ast.Name`). The decoder
applies it when splicing:

| Name | Binder position | Reference position |
|---|---|---|
| `Local s` | Renamed to `s·N` where `N` is unique to this expansion | Renamed to the same `s·N`; a `Local s` reference with no `Local s` binder in the output is an error (`MacroUnboundLocal`) |
| `Raw s` | Spliced as `s` | Spliced as `s`; resolves in the invocation module's scope like user code |
| `Global m s` | Not allowed as a binder (`MacroGlobalBinder`) | Resolves to `m.s` through the build's interface table, ignoring the invocation module's imports and aliases |

Consequences:

- A macro that writes `\x -> ... x ...` with `Ast.name "x"` can never
  capture a user's `x`, and user code passed in as `Raw "x"` can never be
  captured by it.
- A macro that wants to bind a name the user will refer to (an
  anaphoric macro) uses `Ast.raw`.
- Top-level declarations emitted by a declaration macro use `Raw` names
  for anything the user must be able to call (`impl` method names, the
  generated function in `@memoize`), and `Local` names for helpers. A
  `Local` top-level name is renamed and is not exported.
- The renamed form `x·1` contains a character that is not a valid
  identifier character, so it cannot collide with source text. Diagnostics
  print `x` and mention the macro when a renamed name appears.

The gensym pass runs on the decoded surface AST before canonicalization,
see plan chunk "Hygiene".

## Expansion algorithm

Per module, in `nash-driver`:

```
round = 0
loop
    surface  = parse(source) if round == 0 else spliced
    can      = canonicalize(surface, mode = Lenient)
    types    = constrain + solve(can, mode = Lenient)
    uses     = collect_macro_uses(can)
    if uses is empty:
        can, types = canonicalize + solve(surface, mode = Strict)   -- normal errors
        break
    round += 1
    if round > limit:
        error MacroExpansionLimit { sites: uses }
    for each use in uses (declaration attributes first, source order):
        program = compiled_macro(use.macro)            -- from the defining module's build output
        input   = encode(use, can, types)              -- PlutusData
        result  = cek.run(program applied to input, budget)
        match result:
            Err(machine_error, logs) -> error MacroFailed { site: use.region, message: last(logs) }
            Ok(constant)             -> output = decode(constant)      -- surface AST
        output = gensym(output, round, use)
    spliced  = splice(surface, uses, outputs)
```

Details:

- **Lenient mode** exists because macro output may introduce names and
  impls that user code already refers to (`@derive(Eq)` on `Foo` and
  `a == b` on `Foo` in the same module). In lenient mode:
  - an unresolved unqualified or qualified variable, constructor, or type
    canonicalizes to a placeholder (`Expr::Hole`, `Pattern::Hole`,
    `Type::Hole`) instead of an error;
  - unresolved trait predicates are dropped instead of reported: the
    solver runs with `Solver.mode = Mode::Lenient` (traits.md "Lenient
    mode", plans/03 chunk 6), which detaches any predicate that would
    have raised `nash_constrain::Error::MissingImpl` and records no
    evidence for it;
  - a `MacroCall` gets a fresh flexible type; an attributed declaration is
    checked as written.
  Unification errors are still real errors and stop the module (they do
  not depend on missing declarations, because holes unify with anything).
- **Strict mode** is the normal Elm behaviour and runs exactly once, on
  the fully expanded module.
- **Order**: attributes on one declaration run top to bottom, each seeing
  the previous one's output for that declaration (re-encoded without
  types; a later attribute that needs types sees `None`). Expression
  macros run after declaration macros in the same round. Nested
  `name!(...)` inside macro arguments is expanded in a later round
  (inner-first is not needed because the inner call is just a typed
  `MacroCall` node in the outer macro's input).
- **Fixed point**: an output that still contains invocations triggers
  another round. The limit is 32 (configurable, `macroExpansionLimit` in
  `nash.jsonc`). Exceeding it reports every remaining site.
- **Budget**: each macro run gets a CEK budget (`macroBudget`, default
  10x the mainnet transaction budget). Exhaustion is `MacroFailed` with
  the budget message.
- **Caching**: the compiled macro program is keyed by the defining
  module's content hash. Expansion results are not cached across builds
  in v1.
- **Determinism**: gensym counters are per module and per round, so the
  same source always expands to the same names.

### What a macro can observe

Only its arguments. Macros have no access to the module's other
declarations, the file system, or the build. A macro that needs the
definition of another type asks the user to pass it (`@derive` receives
only the decorated declaration). This keeps expansion a pure function of
the inputs, which is what makes caching and the LSP tractable.

## Errors

Errors raised by the macro itself:

```elm
deriveOne decl trait =
    case Ast.exprName trait of
        Some "Eq" -> deriveEq decl
        _ -> fail "derive: expected a trait name such as Eq, Ord, Show, ToData, FromData"
```

`fail msg` traces `msg` and errors. The runner reports the last trace
line at the invocation site:

```
-- MACRO FAILED --------------------------------------------- src/Foo.nash

The macro `derive` failed while expanding this attribute:

3| @derive(Eq, Ordered)
   ^^^^^^^^^^^^^^^^^^^^
It said:

    derive: expected a trait name such as Eq, Ord, Show, ToData, FromData
```

A macro that errors without a trace (a raw `error` term, a builtin
failure) reports the CEK machine error text.

Compiler-detected errors:

| Error | When |
|---|---|
| `MacroBadShape` | annotation is not one of the two shapes |
| `MacroSameModule` | invocation in the defining module |
| `MacroNotAMacro` | `@name` / `name!` resolves to a plain value |
| `MacroWrongKind` | declaration macro used as `name!(..)` or vice versa |
| `MacroFailed` | CEK error during expansion |
| `MacroBadOutput` | result `Data` does not decode to the `Ast` type |
| `MacroUnboundLocal` | `Local` reference without binder in the output |
| `MacroGlobalBinder` | `Global` name in binder position |
| `MacroExpansionLimit` | more than `macroExpansionLimit` rounds |
| `SpliceOutsideQuote`, `NestedQuote` | parse errors |

Errors from the strict pass over expanded code point at the invocation
site and include the macro name and a pretty-printed excerpt of the
generated declaration (`nash-fmt` renders it), because the user cannot
see that code otherwise.

## `comptime`

```elm
table = comptime (buildTable 1000)

check x =
    x < comptime (Int.pow 2 64)

x = comptime (expensive 1000)
```

```ebnf
comptime_expr    = 'comptime' term ;
```

Semantics:

- `comptime e` has the type of `e`.
- After solving, the kind of that type must be `Const` or `Big`. A `Term`
  kind (functions, little ADTs, tuples) is an error (`ComptimeNotConstant`)
  because the result must be representable as a UPLC constant.
- `e` must be **closed**: it may not mention local variables of the
  enclosing function (lambda parameters, `let` bindings, pattern
  variables). Top-level values and imports are allowed. Violation is
  `ComptimeNotClosed` naming the variable.
- `e` may not contain `comptime` (they are nested constants anyway) or an
  unexpanded macro invocation (expansion runs first, so this cannot
  happen).
- Evaluation: `nash-codegen` compiles `e` plus its dependencies to a
  standalone program and runs it on the CEK machine with the
  `comptimeBudget` (default 10x the mainnet transaction budget, settable
  in `nash.jsonc` and by `--comptime-budget`).
- The resulting `Constant` is spliced as a `Core::Const` node. Failures
  are `ComptimeFailed` with the machine error and the last trace line.
- Traces emitted during comptime are printed at compile time only with
  `--trace-comptime`.
- Comptime happens per use site, during lowering to Core. Two identical
  `comptime` expressions evaluate twice (v1; content-hash caching is an
  open question).
- A top-level `x = comptime (...)` is the same expression form; it is not
  a separate declaration kind.

Interaction with tests: a `tests` block may use `comptime`. A `prop` whose
generator is `comptime` is a constant and is a warning.

## Deriving

`@derive(Eq, Ord, Show, ToData, FromData)` is the declaration macro
`Derive.derive` from `nash/core`, exposed by the default imports. It
dispatches on the trait name and appends one `impl` per trait after the
original declaration.

Sketch of the `Eq` derivation in Nash:

```elm
module Derive exposing (derive)

import Ast exposing (Decl(..), Expr, Name(..), Pattern)
import List
import String

macro derive : Decl -> List Expr -> List Decl
derive decl traits =
    lift (decl :: List.map (deriveOne decl) (lower traits))

deriveOne : Decl -> Expr -> Decl
deriveOne decl trait =
    case Ast.exprName trait of
        Some "Eq" -> deriveEq decl
        Some "Ord" -> deriveOrd decl
        Some "Show" -> deriveShow decl
        Some "ToData" -> deriveToData decl
        Some "FromData" -> deriveFromData decl
        Some other -> fail ("derive: no derivation for " ++ other)
        None -> fail "derive: expected a trait name such as Eq, Ord, Show, ToData, FromData"

deriveEq : Decl -> Decl
deriveEq decl =
    case decl of
        Union union ->
            let
                -- input lists are Big; lower them once
                params = lower union.params
                ctors = lower union.ctors

                self =
                    Ast.tcon union.name (List.map (\p -> Ast.tvar (lower p.name)) params)

                context =
                    List.map (\p -> Ast.constraint (Raw "Eq") [ Ast.tvar (lower p.name) ]) params

                names prefix ctor =
                    List.indexedMap (\i _ -> Ast.name (prefix ++ String.fromInt i)) (lower ctor.args)

                ctorArm ctor =
                    let
                        xs = names "x" ctor
                        ys = names "y" ctor
                        fieldsEqual =
                            List.map2 (\x y -> quote (~(Ast.var x) == ~(Ast.var y))) xs ys
                    in
                    Ast.arm
                        (Ast.ptuple [ Ast.pctor ctor.name (List.map Ast.pvar xs)
                                    , Ast.pctor ctor.name (List.map Ast.pvar ys) ])
                        (Ast.and fieldsEqual)

                fallthrough =
                    Ast.arm Ast.wildcard (quote False)

                a = Ast.name "a"
                b = Ast.name "b"

                body =
                    Ast.case_ (Ast.tuple [ Ast.var a, Ast.var b ])
                        (List.map ctorArm ctors ++ [ fallthrough ])
            in
            Ast.impl (Raw "Eq") [ self ] context
                [ Ast.def (Raw "eq") [ Ast.pvar a, Ast.pvar b ] body ]

        _ ->
            fail "derive(Eq): only `type` declarations can derive Eq"
```

Notes on the sketch:

- `ctor.name` from the input is `Global`, so the generated patterns match
  the right constructors even if the user's module renames them on import.
- The trait name and the method name are `Raw`: `Eq` must resolve at the
  invocation site (it is in the prelude), and `eq` is the method the trait
  declares. `Raw "Eq"` is a `string` literal at type `Bytes` via
  `FromString Bytes`.
- `x0`, `y0`, `a`, `b` are `Local` and get renamed, so a field named `a`
  cannot interfere.
- `lower` on `List Param` / `List Ctor` / `List Type` uses the
  `Lift (list 'a) (List 'b)` impl with the reflexive element impl, so it
  is one `unListData`. Builders `lift` their list arguments back.
- The `fallthrough` arm is omitted for single-constructor types by the
  real implementation to avoid a redundant-pattern warning.
- `ToData`/`FromData` derivations check `union.kind == Some Big` and fail
  otherwise; `Show` and `Ord` work on any kind. `Ord` derives `compare` by
  constructor index then lexicographic fields.

## Interactions with other components

- **Parser** (plans/01 for `@attr`, `name!()`, `comptime`; plan 11 for
  `macro`, `quote`, `~`).
- **Canonicalization**: lenient mode; `quote` desugaring; `Global` name
  resolution via the interface table; macro declarations recorded in the
  interface (`InterfaceMacro { name, shape }`) so importers can check
  `MacroWrongKind` without the body.
- **Solver**: lenient mode for predicates; per-node type map for the
  encoder (`NodeTypes`).
- **Kinds**: `comptime` kind check; `Union.kind` in the reified AST.
- **Codegen**: compiles macro programs and comptime programs; consumes
  `Core::Const` splices.
- **Driver**: owns the expansion loop; macro programs are part of a
  module's build artifact so dependents can run them.
- **Diagnostics**: `nash-report` renders the errors above; generated code
  excerpts use `nash-fmt`.
- **LSP**: hover on `@derive(..)` shows the generated declarations
  (pretty-printed); go-to-definition on a generated impl points at the
  attribute.
- **Formatter**: `nash-fmt` prints attributes, `macro`, `quote`, `~`,
  `comptime` and never sees expanded code.

## Open questions

1. **Attribute arguments are untyped.** `@derive(Eq)` cannot be typed
   because `Eq` is a trait, not a value. Should attribute arguments be
   restricted to a small grammar (names, literals, nested calls) instead
   of full expressions?
2. **Comptime caching** by content hash of the closed Core term: worth it
   for large tables, not needed for v1.
3. **Expression macro result type in lenient mode** is a fresh flexible
   variable. If defaulting picks a type for an argument literal that the
   macro then changes (`sum!(1, 2)` where the macro expands to a `Bytes`
   operation), the strict pass reports a normal type error at the site.
   Acceptable, but the message should mention the macro.
4. **Splices in non-expression positions** (`~..args` for argument
   lists, pattern and type splices) are deferred; builders cover them.
