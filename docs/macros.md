# Macros and compile-time evaluation

Nash has syntax macros, as in Racket: a macro is an ordinary Nash function that
the compiler runs on the CEK machine during compilation. It receives source
syntax and returns source syntax. The compiler then checks the expanded module
in the ordinary strict way. `comptime` is separate: it evaluates a closed
expression to a UPLC constant, and it already works.

Macros are planned; [Plan 11](../plans/11-macros-comptime.md) tracks the work.
The comparison with Racket covers only syntax expansion before ordinary checking
and outer-first expansion. Nash keeps a fixed grammar and its own scope
representation.

## Compiler boundary

```text
parse
  -> resolve macro imports and names; track syntax scopes
  -> expand outer invocations; repeat on the emitted syntax
  -> ordinary canonicalization, kinds, inference, evidence
  -> nitpick
  -> Core, specialization, UPLC
```

The expander knows macro imports and syntax scopes. It does not infer types.
Ordinary unresolved names, type errors, and coverage errors are reported on the
expanded module. A module with no macro invocations keeps its current path.

| Component | Responsibility |
|---|---|
| `nash-macro` (new) | Syntax encoding and decoding, execution, output checks, hygiene |
| `nash-driver` | Macro imports, compiled macro programs, the expansion loop, final checking |
| `nash-codegen` | Compiles macro functions through the existing Build and program assembly |
| `nash-can`, `nash-constrain`, `nash-solve`, `nash-nitpick` | Ordinary checking of the expanded module |

There is no second inference engine and no lenient solver mode.

## What stays compiler-defined

`do`, `trace`, `assert`, `fail`, `todo`, and `comptime` are special forms.
`tests`, `test`, `prop`, `via`, generators, and the test runner stay compiler
features. Macros do not replace them. A new macro feature is added only when a
real use case needs it.

## Declaring a macro

A macro is an annotated top-level definition whose annotation line starts with
`macro`. The annotation is mandatory and fixes the macro's shape. A macro is an
ordinary Nash function over `Ast` values; its own module checks it like any other
definition.

```nash
module Unfold exposing (unfold10)

import Ast

macro unfold10 : cons Ast.expr -> Ast.expr
unfold10 args =
    ...
```

A macro is used only through an import. A call in its defining module is an
error. Macros have their own namespace next to values. Imports in a `tests`
block can supply macros to tests.

## Invocation shapes

| Shape | Source form | Signature |
|---|---|---|
| Call | `name!(a, b)` | `cons Ast.expr -> Ast.expr` |
| Case | `name! subject of` followed by `pattern -> body` arms | `Ast.caseInput -> Ast.expr` |
| Binding | `name! pattern = value` in a `let` or `do` block | `Ast.binding -> Ast.expr` |
| Declaration | `@name(args)` on the line before a declaration | `Ast.decl -> cons Ast.expr -> cons Ast.decl` |

- **Call.** The macro receives the argument expressions in order.
- **Case.** There is no space before `!`; the space after it separates a case from
  the `!(` call form. Write `name! (subject) of` for a parenthesized subject. The
  macro receives the subject and the arms; each arm has its own scope.
- **Binding.** The macro receives the pattern, the value, and the rest of the
  nearest block. The pattern is not in scope in its own value. A binding splits a
  recursive `let` group: later groups see earlier bindings, not the reverse.
- **Declaration.** The macro receives the declaration without the executing
  attribute, and the attribute arguments as syntax. It returns the declarations
  that replace the input; returning the input first keeps it. Retained attributes
  on returned declarations run next. A dropped declaration runs nothing more.

Qualified names work in every shape. A shape fixes only the input. Each macro
chooses its output: an ordinary `case`, integer dispatch, or any other syntax. The
compiler recognizes shapes, never library names such as `expect` or `dispatch`.

Macros cannot add keywords, operators, or token parsers.

## The `Ast` module

Base ships `Ast`. Every `Ast` type is a little ADT (representation `Term`): a
value is a UPLC `constr` tree with native `string`, `int`, and `bytes` leaves,
`option` slots, and `cons` child lists. A constructor is `constr i [fields]`, `i`
the declaration index. Nothing in the macro path is `Data`. The host tag table
in `nash-macro` and `Ast.nash` change together.

`Ast` is source syntax. It covers expressions, patterns, arms, types and written
annotations, declarations, and nested macro invocations, with origins and binding
identities. No node carries an inferred type, kind, representation, expanded
alias, or trait evidence. Written annotations stay syntax and are checked after
expansion.

The decoder checks tags, arities, field types, binding references, and origins,
and reports errors without panics.

## Quote and unquote

`quote (e)` builds the syntax of `e`. `~x` and `~(e)` insert a syntax value.
Macros use quote and unquote as needed. Quote lowers to calls of the `Ast`
builders, which are ordinary Nash functions. A macro can use them inside a quote:

```nash
macro dispatch : cons Ast.expr -> Ast.expr
dispatch args =
    case args of
        Cons n (Cons (Ast.EList branches) Nil) ->
            quote (~(Ast.integerDispatch n branches))
        _ ->
            fail "dispatch! expects (n, [branches])"
```

- A global written in a quote resolves in the macro's defining module, also a
  private helper that the caller cannot see.
- Each binder in a quote gets a fresh identity every time the quote is evaluated.
- Inserted syntax keeps its caller resolution and its origins.
- `~` outside a quote is an error. Nested quotes are an error until a use case
  needs them.
- Macro calls inside a quote stay syntax until a returned invocation runs.

Quote covers expressions first, then patterns, then declarations and impls.

## Hygiene and resolution

- Copied or generated binders get fresh identities, and their references change
  with them. Equal spellings never mean equal identities.
- Free references in caller syntax keep their caller binding.
- A supplied pattern and its body are freshened together. Separate arms and
  nested scopes stay separate.
- Globals written by the macro resolve in its defining module. Caller syntax keeps
  the caller's resolution, also for names that resolve after expansion.
- Caller dependencies are explicit macro inputs. A macro cannot look up a caller
  name by a generated string. Macro output cannot make a reference to a global
  that the macro's own checked code did not name.
- A generated local reference without a binder is an error. It never falls back
  to a global with the same spelling.

Names are resolved by scope before canonicalization. Generated declarations can
supply names that other code in the module uses, so ordinary name resolution
waits until expansion finishes.

## Source origins

Copied or moved caller syntax keeps its original locations, operands included.
Freshening does not change locations. The expander records the invocation chain
separately. New syntax with no caller location points to the invocation.
Diagnostics show the expansion chain.

## Expansion

- Outer invocations expand first. Only invocations in the emitted syntax expand
  next; children that a macro drops do not run.
- Declaration attributes run top to bottom, before expression expansion in the
  same round.
- Expansion repeats until no invocation remains. Then ordinary strict checking
  runs once on the expanded module.
- Parse errors stay errors, also in input that a macro drops.
- A macro sees only its inputs: not other declarations, the file system, or type
  information.

## Execution

Macros compile through the existing Build and closed program assembly and run on
the existing CEK machine. The result must contain only constructors and scalars;
closures, delays, builtins, and other constants are output errors. Each invocation
has an execution budget and output size limits. Expansion depth has a limit
(default 32); independent invocations do not share it. These limits are separate
from `comptime`. Compiled programs are cached for one build, keyed by the
defining module and its dependencies. There is no cache across builds.

## Errors

Macro errors name the macro and the invocation and show the expansion chain:

- wrong shape, same-module call, call of an ordinary value;
- `fail` in the macro, budget exhaustion, and other CEK failures, with traces;
- malformed output and invalid binding references, including a quoted reference
  to a declaration that is not a checked definition-site global;
- the depth limit.

When a macro stops at an error term, the last trace before it is the main error
text: `fail "msg"` traces `msg`, and a bare `fail` after `trace "x"` gives `x`.
Earlier traces stay under "Macro logs". With no trace, the text is "I could not
expand this macro." with the machine error as help.

Errors in generated code are ordinary name, type, trait, and coverage errors,
reported at the origins of the syntax. When a missing constraint points at an
annotation that a macro generated, the help says to write the declaration by hand.

## Integer dispatch (planned)

Integer dispatch is an AST and Core operation with a subject and an ordered list
of branches. Only macro output makes it; there is no parser syntax. The Base
macro `dispatch!(n, [branch0, branch1, ...])` is its client: the bracketed list is
syntax that expansion consumes, never a runtime list.

- The subject is native `int`; all branches have one result type. Dispatch adds no
  trait or representation constraint. Big callers decode with `Builtin.unIData`.
- It lowers to one direct UPLC `case`. The subject is evaluated once; only the
  selected branch runs. A negative or out-of-range subject fails as UPLC does. An
  empty branch list is allowed.
- No guards, fallback, decoding, filling, or runtime list. Ordinary integer
  literal `case` keeps its equality lowering.

## Deriving (planned)

`@derive(Eq, Ord, Show, Validate)` is a Nash declaration macro in Base (`Derive`).
Its arguments are trait selectors, not type queries. Unknown or duplicate
selectors give an invocation diagnostic. It applies to unions and to record
aliases, with or without parameters: a little type gets Eq, Ord, and Show; a Big
type gets Ord, Show, and Validate. Other aliases and value, trait, and impl
declarations are rejected.

- `Eq` compares constructors, then fields in declaration order, and stops at the
  first unequal field. Big types keep automatic structural Eq; requesting Eq for
  them is an error.
- `Ord` compares constructor order first, then fields in order. Its Eq superclass
  must exist, for example from the same derive.
- `Show` prints `Name`, `Name(value, value)`, or
  `Name { label = value, label = value }`. A record alias prints its alias name:
  `entry { first = True, second = False }`.
- `Validate` requires a Big type. For a union it checks the constructor tag, the
  exact field count, and every field through `Data.validate`, then rebuilds the
  constructor. A Big record alias is a Data list of its fields in order: it
  requires a list with the exact field count (constructor Data is rejected),
  validates every field, then builds the record. Blanket ToData/FromData stay
  unchanged.

Constraints come from the field syntax only: a field `'a` requires the trait for
`'a`; a field `'f 'a` requires it for the application. Parameters that no field
uses need nothing. Other types' bodies and impl contexts are not read, so a
phantom argument inside a concrete type can get a constraint it does not need.
All generated code goes through ordinary checking.

## `comptime`

```nash
table = comptime (buildTable 1000)

check x =
    x < comptime (Int.pow 2 64)
```

```ebnf
comptime_expr    = 'comptime' term ;
```

- `comptime e` has the type of `e`. After solving, that type must have
  representation `Const` or `Big` (`ComptimeNotConstant` otherwise): the result is
  spliced as a UPLC constant.
- `e` must be closed after its reachable dependencies are included. Top-level
  values, imports, and closed local dependencies are allowed; a captured runtime
  parameter is rejected.
- `e` may not contain `comptime`.
- `nash-codegen` compiles `e` and its dependencies to a standalone program and runs
  it on the CEK machine with V3 and `ExBudget::default()`. The result is a
  `Core::Lit`. A failure is `ComptimeFailed` with the machine error and its traces.
- Evaluation happens per use site during lowering to Core.
- Macro output can contain `comptime`, and macro definitions can use it. A macro
  parameter is not a closed constant for a nested `comptime`.
- A `tests` block may use `comptime`. A `prop` generator that is `comptime` is a
  constant and gives a warning.
