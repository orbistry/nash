# Macros and compile-time evaluation

Procedural macros are planned in Plan 11; the `comptime` expression already
works. Both use Nash code on the CEK machine inside the compiler. Macros transform *typed*
canonical AST into *surface* AST that is spliced back into the module and
re-checked. `comptime` evaluates a closed expression to a UPLC constant.

Decisions here follow [overview.md](overview.md): typed input, surface
output, hygienic, `@derive(..)` on declarations, `name!(args)` in
expressions, expand-then-recheck loop per module, deriving implemented as
macros.

## Implementation sequence and testing migration

Plan 11 now starts with a small complete expression-macro path: resolve an
imported macro, transport typed syntax, execute ordinary Nash on CEK, decode
hygienic output, and strictly recheck it. AST builders suffice for this first
slice; quote/splice and deriving follow. Binding/case-shaped inputs then establish
scope-preserving composition before library property macros are introduced.

The accepted direction is to move property preparation/callback generation and
power-assert expansion from dedicated codegen into macros. This is separate work
tracked in Plan 11 chunk 15. Public syntax, generated private-root discovery,
interleaved generation semantics, optional Show evidence, and operand source
origins must be defined before their respective migration steps. The generic
macro shapes below do not by themselves solve those contracts. Runtime generation,
replay, shrinking, execution budgets and reporting remain library/runner work.

## Concepts

| Term | Meaning |
|---|---|
| Macro | A top-level function declared with `macro`, with a declared structured input/output shape (including the binding/case forms required below). |
| Invocation | `@name(args)` before a declaration, or `name!(args)` in an expression. |
| Reification | Turning compiler AST into `Ast.*` values (little ADTs, so they are UPLC `constr` terms at runtime) and back. |
| Expansion round | One pass: find every invocation in a module, run each macro, splice results. |
| Hygiene | Fresh binding identities prevent accidental capture; caller dependencies are explicit inputs. |
| Comptime | `comptime e`: evaluate `e` on the CEK machine at compile time, splice the resulting constant. |

## Declaring a macro

A macro is an annotated top-level definition whose annotation line starts
with `macro`. The annotation is mandatory. It fixes the macro's shape.

```elm
module Derive exposing (derive)

import Ast exposing (type decl, type expr)
import Cons exposing (type cons(..))

macro derive : decl -> cons expr -> cons decl
derive decl traits =
    Cons decl (Cons.map (deriveOne decl) traits)
```

Every `Ast` type is a little ADT (representation `Term`). Argument lists, result
lists, and the child lists inside `Ast` nodes are `cons 'a`, the
Term linked list from the core `Cons` module
(`type cons 'a = Nil | Cons 'a (cons 'a)`, [stdlib.md](stdlib.md)); `list`
cannot hold Term elements. No `Data`, `lift`, or `lower` appears anywhere
in macro code.

The two shapes:

```elm
-- declaration macro: the decorated declaration, then the attribute arguments
macro name : Ast.decl -> cons Ast.expr -> cons Ast.decl

-- expression macro: the call arguments
macro name : cons Ast.expr -> Ast.expr
```

These are the original expression/declaration interfaces. Plan 11 chunk 14
requires additional binding/case-shaped interfaces; extend shape validation and
its diagnostics together rather than restricting the system to these two forms.

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
@derive(Ord, Show, Validate)
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
  are not in any scope. `Eq` is passed as `Ast.expr` whose node is
  `Var (Raw "Eq")` with `typ = None`. The macro interprets them.
- The macro receives the decorated declaration as a **typed** `Ast.decl`
  (see "What the macro sees").
- The result `cons Ast.decl` **replaces** the decorated declaration. To
  keep the original, return it as the first element. `derive` returns
  `Cons decl (Cons impl1 (Cons impl2 ...))`.

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

Expression macro arguments are typed syntax trees, not evaluated values. A
macro can inspect an integer or list literal, but a variable such as `n` or
`xs` does not reveal its runtime value. This makes literal-driven unrolling
possible without adding a runtime loop.

For example, `tail` requires an integer literal followed by an expression and
constructs that many calls to the primitive `Builtin.tailList`:

```elm
module TailMacro exposing (tail)

import Ast exposing (type expr(..), type exprNode(..))
import Builtin
import Cons exposing (type cons(..))

macro tail : cons Ast.expr -> Ast.expr
tail arguments =
    case arguments of
        Cons (Expr _ (IntLit count)) (Cons value Nil) ->
            repeatTail count value

        _ ->
            fail "tail: expected an integer literal and an expression"

repeatTail : int -> Ast.expr -> Ast.expr
repeatTail count value =
    if count < 0 then
        fail "tail: count cannot be negative"
    else if count == 0 then
        value
    else
        repeatTail (count - 1) (quote (~value |> Builtin.tailList))
```

A macro can likewise unroll a predicate over a list literal:

```elm
module PredicateMacros exposing (predicateAll)

import Ast exposing (type expr(..), type exprNode(..))
import Cons exposing (type cons(..))

macro predicateAll : cons Ast.expr -> Ast.expr
predicateAll arguments =
    case arguments of
        Cons predicate (Cons (Expr _ (ListLit items)) Nil) ->
            Ast.and
                (Cons.map
                    (\item -> Ast.call predicate (Cons.singleton item))
                    items
                )

        _ ->
            fail "predicateAll: expected a predicate and a list literal"
```

With partial operator sections, the call:

```elm
predicateAll!((> 5), [10, 12, 15])
```

constructs the equivalent of:

```elm
(> 5) 10 && (> 5) 12 && (> 5) 15
```

Normal beta reduction can simplify that further to:

```elm
(10 > 5) && (12 > 5) && (15 > 5)
```

The literal list is gone from the generated program, so there is no runtime
fold or list traversal. The empty list expands to `True` through `Ast.and
Nil`. `predicateAll!((> 5), xs)` is rejected because a runtime list cannot be
unrolled by inspecting its syntax. The ordinary runtime version remains:

```elm
predicateAll p xs =
    List.foldr (\x acc -> p x && acc) True xs
```

### Explicit positional integer dispatch (planned)

Plan 11 chunk 13 adds an explicit `IntegerDispatch` AST operation and an ordinary
library macro:

```nash
dispatch!(n, [branch0, branch1, branch2])
```

The macro requires two syntax arguments, extracts branch expressions from the
second argument's `ListLit` node, and returns
`Ast.integerDispatch subject branches`. It never constructs a runtime list and
is not recognized specially by the compiler. A runtime list variable is rejected
by the macro. The compiler type-checks the resulting operation: native `int`
subject, a common branch result type, and no dispatch-specific trait constraints.
The syntax list must not impose a surviving Storable constraint on the expanded
branches; Term and function results are supported through the ordinary macro
lenient-typing/recheck contract.

The operation lowers directly to `(case subject branch0 branch1 branch2)`.
The subject is evaluated once; only its selected branch executes. Negative or
out-of-range indices fail. An empty branch sequence evaluates its subject then
fails, with an unconstrained result type. There are no inserted guards, fallback,
decoding, branch-count heuristics, or runtime thunk wrappers. To dispatch on Big
Int, the caller explicitly supplies `Builtin.unIData x`; to handle out-of-range
values, the caller explicitly writes a guard.

The AST operation survives reification, hygiene, strict typing, and Core lowering;
it is distinct from ordinary literal `case` and from Core's equality-chain
`CaseKind::Int`. This feature is pending, not available source functionality yet.

## What the macro sees: the `Ast` module

`nash/base` ships an `Ast` module. Every type in it is a **little** ADT
(representation `Term`): a value is a UPLC `constr` tree whose leaves are `string`,
`int`, and `bytes` constants, exactly the layout of any user little type
([representation.md](representation.md)). The compiler builds that tree
directly as `nash_plutus::Term::Constr` nodes, applies the macro program
to it, and walks the resulting `constr` tree back into surface AST; no
`Data` encoding is involved. One family of types serves both input
(typed) and output (surface). Type information lives in `option` slots
that are `Some` when resolved on input and are ignored on output. Provisional
unresolved syntax retains names, lexical references and origins with absent type
metadata; it must not be erased into a payload-free hole. Child lists are
`cons` ([stdlib.md](stdlib.md) "`Cons`") because `list` elements must be
`Storable` and `Ast` nodes are `Term`.

```elm
module Ast exposing (..)

import Cons exposing (type cons(..))

type alias span = { startRow : int, startCol : int, endRow : int, endCol : int }

-- How a name resolves after splicing.
type name
    = Local string            -- hygienic: renamed per expansion (see Hygiene)
    | Raw string              -- spliced verbatim, resolves at the invocation site
    | Global modname string   -- fully qualified, resolves regardless of imports

type alias modname = { package : option string, name : string }

-- Closed inferred kinds; there are no kind variables or bounds on input.
type kind = Type | Arrow kind kind

-- Runtime representations are independent of kinds.
type repr = Big | Const | Term

-- Source annotation sugar; Little is written as an ordinary constraint.
type reprAnnotation = Repr repr | Storable

type alias meta = { span : option span, typ : option typ }

type expr = Expr meta exprNode

type exprNode
    = IntLit int
    | StrLit string
    | BytesLit bytes
    | Var name
    | Op name                          -- operator used as a value: (+)
    | ListLit (cons expr)
    | Negate expr
    | BinOp name expr expr             -- resolved to the operator's function name
    | Lambda (cons pattern) expr
    | Call expr (cons expr)
    | If expr expr expr
    | Let (cons def) expr
    | Case expr (cons arm)
    | Accessor string
    | Access expr string
    | Update name (cons fieldAssign)
    | Record (cons fieldAssign)
    | UnitLit
    | Tuple (cons expr)                -- length >= 2
    | MacroCall name (cons expr)       -- output may contain new invocations
    | Comptime expr
    | IntegerDispatch expr (cons expr) -- explicit native integer UPLC case

type def
    = Define name (cons pattern) expr (option typ)
    | Destruct pattern expr

type alias arm = { pattern : pattern, body : expr }
type alias fieldAssign = { field : string, value : expr }

type pattern = Pattern meta patternNode

type patternNode
    = PAny
    | PVar name
    | PRecord (cons name)
    | PAlias pattern name
    | PUnit
    | PTuple (cons pattern)
    | PCtor name (cons pattern)
    | PList (cons pattern)
    | PCons pattern pattern
    | PInt int
    | PStr string
    | PBytes bytes

type typ
    = TVar string
    | TCon name (cons typ)
    | TFun typ typ
    | TRecord (cons field)
    | TTuple (cons typ)
    | TUnit

type alias field = { name : string, typ : typ }
type alias param = { name : string, kind : option kind, repr : option reprAnnotation }
type alias constraint = { traitName : name, args : cons typ }

type decl
    = Value { name : name, args : cons pattern, body : expr, annotation : option typ }
    | Union { name : name, params : cons param, kind : option kind, representation : option repr, ctors : cons ctor }
    | Alias { name : name, params : cons param, kind : option kind, representation : option repr, typ : typ }
    | Trait traitDef
    | Impl implDef
    | Infix { op : string, assoc : assoc, prec : int, function : name }

type alias ctor = { name : name, args : cons typ }
type assoc = LeftAssoc | RightAssoc | NonAssoc

type alias traitDef =
    { name : name
    , params : cons param
    , supers : cons constraint
    , methods : cons method
    }

type alias method = { name : name, typ : typ, default : option expr }

type alias implDef =
    { traitName : name
    , args : cons typ
    , context : cons constraint
    , defs : cons def
    }
```

Naming: type names avoid the keywords `type`, `module`, `trait`, `impl`
(`typ`, `modname`, `traitDef`, `implDef`, field `traitName`), and literal
constructors carry a `Lit` suffix so they do not collide with the
compiler-known `Data` constructors (`List`, `I`, `B`) that are in scope
everywhere. Little records (`meta`, `arm`, `span`, ...) are `constr 0`
terms; labeled constructors (`Value {..}`) are flat `constr i` terms, and
`case decl of Union { name, params, ctors } -> ...` is the pattern sugar
that reads them.

Input conventions (reification, `nash-macro`):

- Input `expr` and `pattern` nodes retain source spans and available solved
  types. Resolved nodes have `typ = Some`; provisional unresolved nodes may have
  `typ = None` rather than invented concrete type information. `Union`/`Alias` and their parameters carry closed inferred
  `kind = Some`. `representation` is `Some` where known, independently of
  the kind. A transparent alias uses its substituted body. Parameter
  `repr` preserves the source representation annotation, if present.
- Local variables and their binders retain lexical identity. Copying bound
  syntax freshens binders and their uses together; free caller references retain
  their original identity. A raw spelling alone cannot encode this relationship.
- Top-level, foreign, constructor, and operator references are
  `Global modname name`. `BinOp` carries the operator's *function* name.
- `if` chains are nested `If`. `let` with several definitions is one `Let`
  with a `cons def` in source order. Elm's `LetRec`/`LetDestruct` fold
  into that list.
- Partial operator sections have already been canonicalized to `Lambda` with
  a `BinOp` body; macros do not receive a section-specific node.
- `do` blocks arrive desugared (`Call (Var (Global Monad "bind")) ...`).
- `Alias` types are fully expanded in `typ` slots (`Ast.typ` has no alias
  node); the alias *declaration* is still visible as `Decl.Alias`.

Output conventions (the walk back to surface AST):

- Preserve original spans on supplied syntax. Generated syntax without an
  original caller location uses the invocation region. Attach expansion history
  separately, as specified under Source locations and expansion origins.
- `typ` is ignored.
- `name` decides resolution as described under Hygiene.
- Inferred `kind` and `representation` slots are ignored. Kinds and
  datatype contexts are inferred again after splicing; the name's casing
  imposes the normal representation rules. Parameter `repr` emits source
  representation annotation sugar; it never emits a kind annotation.
- The result must be a pure constructor tree: a lambda, delayed term, or
  partially applied builtin where a node is expected is `MacroBadOutput`.

`Ast` also exports builders so macro code does not spell every `meta`:

```elm
expr : exprNode -> expr                    -- span = None, typ = None
pat : patternNode -> pattern
name : string -> name                      -- Local
raw : string -> name                       -- Raw
var : name -> expr
int : int -> expr
str : string -> expr
call : expr -> cons expr -> expr
lambda : cons pattern -> expr -> expr
case_ : expr -> cons arm -> expr
integerDispatch : expr -> cons expr -> expr -- positional branches, native int subject
tuple : cons expr -> expr
arm : pattern -> expr -> arm
pvar : name -> pattern
pctor : name -> cons pattern -> pattern
ptuple : cons pattern -> pattern
wildcard : pattern
tcon : name -> cons typ -> typ
tvar : string -> typ
constraint : name -> cons typ -> constraint
impl : name -> cons typ -> cons constraint -> cons def -> decl
def : name -> cons pattern -> expr -> def
and : cons expr -> expr                    -- folds with (&&), `True` when empty
nameText : name -> string
exprName : expr -> option string           -- `Some "Eq"` for `Var (Raw "Eq")`
```

## Required reusable structured invocation forms

Plan 11 chunk 14 requires these capabilities, without defining particular library
macros:

| Form | Structured input |
|---|---|
| `name!(a, b)` | Expression arguments |
| `name! pattern = value` | Pattern, value, and remaining lexical body |
| `name! subject of` followed by normal arms | Subject and ordered pattern/body arms |

These shapes remain required in milestone B, before the property migration.
For example, a binding macro can use `expect! Some x = value`, and a case-shaped
macro can use:

```nash
dispatch! subject of
    0 -> first
    1 -> second
```

The case form does not itself select the explicit positional IntegerDispatch
operation. That operation also retains its call-shaped interface
`dispatch!(n, [branch0, branch1])`; library arm/fallback semantics remain to be
specified. Another case-shaped macro could use:

```nash
decodeIf! valueData of
    Box _ -> body
    Something a b -> otherBody
```

The parser reuses existing grammar components and transports structured AST; it
never recognizes these illustrative macro names. The binding form needs a defined
remaining-scope boundary in let/do contexts. Its expansion can put subsequent code
inside the supplied pattern's scope. Case-shaped inputs preserve independent arm
bindings and order. No raw token parser or arbitrary syntax extension is required.

Earlier expression/declaration signature sketches must be extended for these
input shapes. Their exact payload encoding is implementation work. In particular,
case-shaped macro input must not be constrained as an ordinary runtime case before
expansion: an expansion may introduce the conversion between subject and pattern.
Preserve available provisional types and scopes; strictly check the final AST,
including exhaustiveness/redundancy. Test this generically, not by macro name.

These are capability requirements, not standard-library definitions. Decoding,
validation depth, failure/fallback, dispatch-arm rules, pattern conveniences, and
assertion reporting are not decided here. Builders already let macros generate
ordinary cases/functions and the explicit IntegerDispatch node. Quote extensions
are optional shorthand rather than a prerequisite.

## `quote` and splices

Building trees by hand is verbose. `quote (e)` is parser sugar that turns
an expression into the `Ast.expr` value that builds it. `~x` inside a quote
splices an `Ast.expr` value; `~(e)` splices the result of an expression of
type `Ast.expr`.

```elm
eqField : Ast.name -> Ast.name -> Ast.expr
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
  `==` above becomes `BinOp (Global {nash/base} Eq "eq")` (the method
  the `Prelude` infix binds to). The invocation site does not need to
  import `Prelude` or `Eq` items for the spliced code to work.
- Binders introduced inside the quote (`\x ->`, `let y =`, pattern
  variables) become `Local`, so they are hygienic.
- A splice may appear anywhere an expression may appear inside the quote.
  Splicing into pattern, type, or argument-list positions is not supported
  in v1; use builders (`Ast.call f args`, `Ast.pvar`).
- A `quote` may not contain a `quote`. A splice outside a quote is a parse
  error (`SpliceOutsideQuote`).
- `quote` has type `Ast.expr`. It is only useful in modules that import
  `Ast`; using it elsewhere is a normal "unknown type" error.

`quote type (t)`, `quote pattern (p)`, and `quote decl (d)` are deferred
shorthand extensions, not prerequisites for constructing those AST values.

## Source locations and expansion origins

**Settled contract (6 October 2026):** preserve original locations when supplied
syntax is moved or copied, down to individual operands. Freshening bindings does
not change their source origins. Attach invocation provenance and nested expansion
history separately. Generated syntax without an original caller location points
to the macro invocation. Diagnostics retain access to the expansion chain.

The concrete encoding is still to be chosen. Older output-span sketches must not
be implemented as unconditional replacement of supplied spans by invocation spans.
This is a syntax-metadata preservation rule, not a requirement to share storage
with comments.

## Hygiene

**Settled binding contract (6 October 2026):** freshen each copied or generated
binder and all references bound by it, preserving lexical scope. Free caller
references keep their original binding identities. Equal display names do not
imply equal identities. In particular, a supplied pattern and body must be
freshened together so the body still refers to that pattern's bindings.

The name modes below describe resolution intent. Their historical string-based
encoding is illustrative and must be revised to carry this identity contract;
fresh names alone are not a scope-tracking algorithm. The concrete encoding and
explicit caller-visible binding API remain design work.

**Settled resolution contract (6 October 2026):**

| Name source | Resolution |
|---|---|
| Macro-written global | Macro defining module; preserve the resolved global identity |
| Supplied caller reference | Original caller binding/global identity |
| Copied or generated local binder | Fresh identity with consistently updated bound references |
| Supplied pattern and remaining body | Preserve their binding relationship while freshening together |
| Named generated declaration or trait method | Explicit declaration API, distinct from reference lookup |

Arbitrary caller-scope lookup by string and implicit anaphoric bindings are
deferred. Caller dependencies must be passed explicitly. Historical `Raw` and
`Ast.raw` signatures in the AST/builder sketches are not an initial public lookup
API: separate unresolved input spelling from bindings, globals, and declaration
names when finalizing that encoding. Reading an attribute argument's spelling
is not permission to resolve a generated reference in the caller's scope.

The gensym pass runs on the decoded surface AST before canonicalization,
see plan chunk "Hygiene".

Moving an input pattern/body pair into a generated arm must preserve that
pattern's bound references and all caller free references. Separate clauses and
nested scopes remain separate even when names have the same spelling. Preserve
binding identity or consistently freshen the binder and its references; a global
text replacement is not sufficient hygiene for combining input fragments.

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
        check_main_parameters(can, types)
        nitpick.check(can)       -- exhaustiveness and redundancy on final output
        break
    round += 1
    if round > limit:
        error MacroExpansionLimit { sites: uses }
    for each use in uses (declaration attributes first, source order):
        program = compiled_macro(use.macro)            -- from the defining module's build output
        input   = reify(use, can, types)               -- Term::Constr tree
        result  = cek.run(program applied to input, budget)
        match result:
            Err(machine_error, logs) -> error MacroFailed { site: use.region, message: last(logs) }
            Ok(value)                -> output = unreify(value)        -- constr tree -> surface AST
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
- **Strict mode** runs on the fully expanded module, followed by ordinary
  exhaustiveness/redundancy checking. Provisional rounds do not reject refutable
  lambda patterns carried as macro syntax. A refutable lambda left in runtime
  output is still rejected. This is generic phase ordering, not a macro-name
  exception. Final recanonicalization and solving rebuild ordinary recursive
  dependencies and types from the expanded definitions.
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
        _ -> fail "derive: expected a trait name such as Eq, Ord, Show, Validate"
```

`fail msg` traces `msg` and errors. The runner reports the last trace
line at the invocation site:

```
-- MACRO FAILED --------------------------------------------- src/Foo.nash

The macro `derive` failed while expanding this attribute:

3| @derive(Eq, Ordered)
   ^^^^^^^^^^^^^^^^^^^^
It said:

    derive: expected a trait name such as Eq, Ord, Show, Validate
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
| `MacroBadOutput` | result term is not a well-formed `Ast` constructor tree (wrong tag or arity, or a lambda/delay/builtin where a node was expected) |
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
- After solving, that type must have representation `Const` or `Big`. A
  `Term` representation (functions, little ADTs, tuples) is an error (`ComptimeNotConstant`)
  because the result must be representable as a UPLC constant. This is
  the one place the two mechanisms differ: a macro result is an `Ast`
  `constr` tree that the compiler walks, while a `comptime` result is
  spliced as a constant into the program.
- `e` must be **closed after including its reachable dependencies**. Top-level
  values, imports, and closed local dependencies are allowed. Capturing an
  enclosing runtime parameter is rejected. Preserve the existing dependency
  closure behavior rather than rejecting all local bindings syntactically.
- `e` may not contain `comptime` (they are nested constants anyway) or an
  unexpanded macro invocation (expansion runs first, so this cannot
  happen).
- Evaluation: `nash-codegen` compiles `e` plus its dependencies to a
  standalone program and runs it on the CEK machine. The current implementation
  uses V3 and `ExBudget::default()`. Configurable `comptimeBudget` and a
  `--comptime-budget` flag are not implemented and are not prerequisites for the
  first macro slice; any addition needs an explicit configuration contract.
- The resulting `Constant` is spliced as a `Core::Lit` node. Failures
  are `ComptimeFailed` with the machine error and the last trace line.
- Current comptime failures include captured traces. A `--trace-comptime` flag
  for successful evaluation is not implemented.
- Comptime happens per use site, during lowering to Core. Two identical
  `comptime` expressions evaluate twice (v1; content-hash caching is an
  open question).
- A top-level `x = comptime (...)` is the same expression form; it is not
  a separate declaration kind.

Interaction with tests: a `tests` block may use `comptime`. A `prop` whose
generator is `comptime` is a constant and is a warning.

## Deriving

`@derive(Eq, Ord, Show, Validate)` is the declaration macro
`Derive.derive` from `nash/base`, exposed by the default imports. It
dispatches on the trait name and appends one `impl` per trait after the
original declaration.

Requested traits must satisfy the declared type's Haskell 98 kinds and
representation predicates. Eq
derivation applies to little types. Big types already receive structural Eq
from the compiler; deriving must not emit an Eq override for them. Generated
impls go through the same checks as handwritten impls, including rejection
of Big Eq overrides. For a Big Redeemer, request Ord/Show/Validate as
needed and use the automatic Eq instance. `ToData` and `FromData` need no derivation:
core supplies ordinary blanket impls for every Big type. A macro must not
emit concrete conversion impls because they would overlap those blanket
impls. `Validate` is the separate opt-in validation trait.

Sketch of the `Eq` derivation in Nash:

```elm
module Derive exposing (derive)

import Ast exposing (type decl(..), type expr, type name(..), type pattern)
import Cons exposing (type cons(..))
import String

macro derive : decl -> cons expr -> cons decl
derive decl traits =
    Cons decl (Cons.map (deriveOne decl) traits)

deriveOne : decl -> expr -> decl
deriveOne decl arg =
    case Ast.exprName arg of
        Some "Eq" -> deriveEq decl
        Some "Ord" -> deriveOrd decl
        Some "Show" -> deriveShow decl
        Some "Validate" -> deriveValidate decl
        Some other -> fail ("derive: no derivation for " ++ other)
        None -> fail "derive: expected a trait name such as Eq, Ord, Show, Validate"

deriveEq : decl -> decl
deriveEq decl =
    case decl of
        Union { name, params, ctors } ->
            let
                self =
                    Ast.tcon name (Cons.map (\p -> Ast.tvar p.name) params)

                context =
                    Cons.map (\p -> Ast.constraint (Raw "Eq") (Cons.singleton (Ast.tvar p.name))) params

                names prefix ctor =
                    Cons.indexedMap (\i _ -> Ast.name (prefix ++ String.fromInt i)) ctor.args

                ctorArm ctor =
                    let
                        xs = names "x" ctor
                        ys = names "y" ctor
                        fieldsEqual =
                            Cons.map2 (\x y -> quote (~(Ast.var x) == ~(Ast.var y))) xs ys
                    in
                    Ast.arm
                        (Ast.ptuple (Cons (Ast.pctor ctor.name (Cons.map Ast.pvar xs))
                                    (Cons.singleton (Ast.pctor ctor.name (Cons.map Ast.pvar ys)))))
                        (Ast.and fieldsEqual)

                fallthrough =
                    Ast.arm Ast.wildcard (quote False)

                a = Ast.name "a"
                b = Ast.name "b"

                body =
                    Ast.case_ (Ast.tuple (Cons (Ast.var a) (Cons.singleton (Ast.var b))))
                        (Cons.append (Cons.map ctorArm ctors) (Cons.singleton fallthrough))
            in
            Ast.impl (Raw "Eq") (Cons.singleton self) context
                (Cons.singleton
                    (Ast.def (Raw "eq") (Cons (Ast.pvar a) (Cons.singleton (Ast.pvar b))) body))

        _ ->
            fail "derive(Eq): only `type` declarations can derive Eq"
```

Notes on the sketch:

- `ctor.name` from the input is `Global`, so the generated patterns match
  the right constructors even if the user's module renames them on import.
- The historical `Raw` uses in this sketch must be replaced: the Eq trait
  reference resolves in the deriving macro module, while `eq` is an explicit
  declaration of that trait method. Neither needs arbitrary caller lookup.
- `x0`, `y0`, `a`, `b` are `Local` and get renamed, so a field named `a`
  cannot interfere.
- `Union { name, params, ctors }` is the labeled-constructor pattern sugar
  (representation.md); the unmentioned `kind` and `representation` fields
  become `_`.
- Everything is a little value: `cons` lists are walked with `Cons.map`,
  `Cons.map2`, `Cons.indexedMap`, `Cons.append`; there is no `Data`
  conversion at any point.
- The `fallthrough` arm is omitted for single-constructor types by the
  real implementation to avoid a redundant-pattern warning.
- `Validate` derivations check `union.representation == Some Big` and fail
  otherwise. Future derivation preserves constructor tags and field order:
  `validate` generates ordinary
  source cases that check tags, exact arity, and all fields recursively before
  rebuilding typed constructors. Validation never delegates to unchecked
  `fromData`; no hidden compiler-generated checkers are needed. `Show` and
  `Ord` work on any representation. `Ord` derives `compare` by
  constructor index then lexicographic fields.

## Interactions with other components

- **Parser** (plans/01 for `@attr`, `name!()`, `comptime`; plan 11 for
  `macro`, `quote`, `~`).
- **Canonicalization**: lenient mode; `quote` desugaring; `Global` name
  resolution via the interface table; macro declarations recorded in the
  interface (`InterfaceMacro { name, shape }`) so importers can check
  `MacroWrongKind` without the body.
- **Solver**: lenient mode for predicates; per-node type map for the
  encoder (the existing `nash_solve::SolvedTypes`).
- **Kinds and representation**: closed `Union.kind` and separate
  `Union.representation` in the reified AST; `comptime` requires a
  representation that can be returned as a UPLC constant.
- **Codegen**: compiles macro programs and comptime programs; consumes
  `Core::Lit` splices.
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
