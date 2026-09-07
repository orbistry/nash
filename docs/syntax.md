# Nash — Surface Syntax

This document is the complete surface syntax of Nash after every planned
change in [overview.md](overview.md). It is the contract for `nash-source`
and `nash-parse`, and the only home of the grammar (the EBNF section at the
end replaces the one that used to live in `SPEC.md`). Implementation order
is in `plans/01-syntax.md`.

Nash keeps Elm's layout-sensitive recursive-descent grammar and error
hierarchy. Everything below that is not marked *new* or *removed* behaves
exactly as in Elm.

## Lexical structure

### Identifiers and casing

| Form | Meaning |
|---|---|
| `foo`, `fooBar`, `foo_1` | value, field, method, *little type* in type position |
| `Foo`, `Foo.Bar` | constructor, module, *Big type* in type position |
| `'a`, `'msg` | type variable (*new*, OCaml style) |
| `_` | wildcard pattern |
| `_foo` | error: underscore-prefixed names are not variables (Elm rule kept) |

Casing of a type name selects its representation (see `kinds.md`): `Int`,
`List 'a`, `Data` are `Big`; `int`, `list 'a`, `bool`, and lowercase user
ADTs such as `option 'a` are little. Type variables are always written with a
leading `'`. A bare lowercase name in type position is therefore never a
variable; it is a reference to a little type. Builtin types (`int`, `bytes`,
`List`, `Data`, ...) are seeded by the compiler into `Builtin`; there is no
body-less `type int` declaration form.

### Literals

| Literal | Type (defaulted) | Notes |
|---|---|---|
| `42`, `0xFF` | `int` via `FromInt` | `1.5` is an error (`Number::Dot`); no floats |
| `"hi"`, `"""..."""` | `string` via `FromString` | Elm escapes; `\u{..}` kept |
| `#"ff00"` (*new*) | `bytes` via `FromBytes` | even number of hex digits, decoded at parse time |

Char literals are removed. A `'` in expression or pattern position is a
syntax error; in type position it starts a type variable.

### Comments

`--` line comments, `{- -}` nested block comments, `{-| -}` doc comments.
Tabs are rejected everywhere (Elm rule).

### Operators

Operator characters are `+ - / * = . < > : & | ^ ? % ! $`. The reserved
operators `.`, `|`, `->`, `=`, `:`, `=>`, `<-` cannot be user operators
(`=>` and `<-` are *new* reservations). `!` directly after an identifier and
directly before `(` is a macro call, not an operator (see Macros).

### Reserved words

```
if then else case of let in do
type alias module import exposing as where
trait impl
comptime assert fail todo trace
tests validator
```

`alias` is reserved only in the position after `type` (as in Elm). The words
`infix left right non test prop via once within cpu mem label` are
contextual: they are keywords only in the positions shown below and remain
valid variable names elsewhere. `port` and `effect` are removed.

## Modules

```elm
module Vesting exposing (main, Datum(..))
validator module Vesting exposing (main)
```

A validator module (*new*) is an ordinary module whose header starts with
`validator`. `nash build` requires it to expose `main`. The `port` and
`effect` headers are removed.

Exposing lists, in headers and in imports, are Elm's with one addition
(*new*): a little type is written with a `type` prefix so it cannot be
confused with a value of the same name:

```elm
module Prelude exposing (type option(..), type step, Data(..), map)
import Prelude exposing (type option(..), map)
```

`type` must be followed by a lowercase name; `type Foo` is an error. `(..)`
after it exposes the constructors, exactly as for Big types.

A module is: optional header, imports, infix declarations, declarations,
then an optional `tests` block which must be last. Anything after the last
item that is not end of file is a `Module::BadEnd` error.

## Declarations

Every declaration may be preceded by a doc comment and then by any number
of attributes, each on its own line at column 1:

```elm
{-| A redeemer. -}
@derive(Eq, Show, ToData, FromData)
type Redeemer = Claim | Cancel
```

### Values

Unchanged from Elm except that annotations may carry constraints:

```elm
max : Ord 'a => 'a -> 'a -> 'a
max a b = if lt a b then b else a

both : (Eq 'a, Show 'b) => 'a -> 'b -> string
```

A context is one constraint or a parenthesised comma list, followed by
`=>`. A constraint is a trait name applied to one or more type terms
(multi-parameter traits: `Lift 'small 'big`). Constraints are allowed on
value annotations, let-bound annotations, trait method signatures, trait
heads (superclasses) and impl heads. They are not types: `(Eq 'a => 'a)`
inside a type is an error.

### Types and aliases

```elm
type Datum = Datum { owner : Bytes, deadline : Int }   -- Big ADT, labeled fields
type step 'a = Done 'a | Next int 'a                    -- little ADT
type alias acc = { total : int, seen : list Int }       -- little record
type cell ('a : Storable) = Cell (list 'a)              -- representation-annotated binder
```

Changes from Elm:

- The declared name may be lowercase (little type) or uppercase (Big type).
  `alias` cannot be a type name.
- Type parameters are `'a` binders. A binder may carry a representation
  annotation `('a : Storable)` on `type`, `type alias` and `trait`
  parameters and on type variables inside annotations. The names are
  exactly `Big`, `Const`, `Term`, `Storable`; the parser rejects any other
  name. The annotation is sugar for the context entry `Storable 'a`
  ([kinds.md](kinds.md)). There is no syntax for kinds themselves; kinds
  are inferred.
- Constructors are still uppercase, for both Big and little types.
- A constructor may take *labeled fields* (*new*, Aiken style): `Datum {
  owner : Bytes, deadline : Int }`. This is not a constructor holding an
  anonymous record: the fields are encoded flat (`Constr 0 [B, I]` for a Big
  type, `constr 0 [..]` for a little one) and follow the enclosing type's
  representation rule. A constructor has either positional arguments or one `{ ... }`
  block, never both. `datum.owner` works on single-constructor types with
  labeled fields. Construction is `Datum { owner = o, deadline = d }`; the
  record update form `Datum { d | deadline = 0 }` is not supported in v1
  (update exists only on alias records).
- Anonymous record types `{ x : int }` are legal syntax only as the direct
  body of `type alias`; canonicalization rejects them anywhere else, since
  records are nominal (see `kinds.md`).
- Record extension `{ r | x : int }` is removed from types.
- A record alias whose name is lowercase is a little record (`constr 0`);
  uppercase is a Big record (`Data.List` of fields).

### Type expressions

```elm
'a -> 'b                     -- function
List 'a                      -- Big application
list (option int)            -- little application
'f 'a                        -- type-variable application (higher kinded)
(int, Bytes)                 -- tuple (always little)
()                           -- unit
Cardano.Tx.Output            -- qualified
```

A type application head is a Big name, a little name, a qualified name, or a
type variable (*new*: `'f 'a`). Arguments are type terms as in Elm.

### Traits

```elm
trait Eq 'a => Ord 'a where
    compare : 'a -> 'a -> ordering

    lt : 'a -> 'a -> bool
    lt a b = compare a b == LT

trait Functor 'f where
    map : ('a -> 'b) -> 'f 'a -> 'f 'b

trait (Ord 'k, ToData 'k) => Key 'k where
    hash : 'k -> Bytes
```

Head: optional superclass context, `=>`, trait name, one or more binders
(plain `'a` or representation-annotated), `where`. Body: a layout block of methods. A
method is a signature `name : type`; a signature may be followed by a default
definition with the same name (arguments and `=`, exactly like a let
definition after its annotation). A definition without a preceding signature
is an error. The body may be empty (marker trait): `trait Storable 'a where`
followed by the next declaration.

### Impls

```elm
impl Ord int where
    compare = Builtin.compareInteger

impl Eq 'a => Eq (list 'a) where
    eq xs ys = ...

impl Lift int Int where
    lift = ...
    lower = ...
```

Head: optional context, `=>`, trait name, one or more type terms, `where`.
Body: a layout block of value definitions (no annotations; the trait gives
them). Orphan rules and method completeness are checked in canonicalization,
not in the parser.

### Infix

Unchanged: `infix left 6 (|>) = apR`, only before other declarations.

### Attributes

```elm
@derive(Eq, Ord)
@inline
@cost(cpu 10, "note")
```

An attribute is `@` followed by a lowercase name and an optional
parenthesised, comma-separated list of expression arguments. `@derive` is
not special in the parser; every attribute is a procedural macro invocation
resolved in `nash-macro`. Attributes may precede `type`, `type alias`,
values, traits and impls. They are stored on the declaration.

## Expressions

Elm's expression language is kept: literals, variables, application,
operators and whole-operator sections such as `(+)`, `if`, `case`, `let`,
lambdas, lists, tuples, records, field access `r.x`, accessors `.x`, and
record update `{ r | x = 1 }`. Additions:

### Operator sections

An operator in parentheses is a function. Nash also supports partial
operator sections on either side:

```elm
(>)             -- \x y -> x > y
(> 5)           -- \x -> x > 5
(5 >)           -- \x -> 5 > x
```

The operand may be any expression that fits before the closing parenthesis.
Sections are canonicalized to ordinary hygienic lambdas, so later phases and
macros do not need section-specific handling. As in Haskell, `(-x)` remains
negation rather than a right section of `-`; write `\y -> y - x` for that
case. `(-)` is still the subtraction function, and `(x -)` is a valid left
section.

### Keyword expressions

```elm
assert (x > 0)            -- bool -> unit; power-assert reports sub-expressions
fail                      -- 'a
fail "reason"             -- 'a
todo                      -- 'a, warns at compile time
todo "later"
trace "msg" expr          -- evaluates expr, emits msg under trace levels
comptime (fib 20)         -- evaluated on the CEK machine at compile time
```

These five are keywords, not prelude functions. Codegen needs the source
region of the `assert` operand to render power-assert output, `trace` must
be stripped wholesale at the silent trace level, `todo` must warn with a
location, and `comptime` needs its operand's region for diagnostics. They
parse like `if`/`case`/`let`: they may start an expression or appear on the
right of an operator, and they extend as far right as possible
(`trace "m" x + 1` is `trace "m" (x + 1)`). `fail` and `todo` take an
optional term as message; the term must be on the same line or indented
further than the enclosing block.

### `do` blocks

```elm
do
    x <- fuzz int
    label "small"
    assert (x < 100)
```

`do` opens a layout block. Statements align on the column of the first
statement. A statement is a bind `pattern <- expression`, a `let` statement,
or an expression; the last statement must be an expression. `x <- e; rest`
desugars to `Monad.bind e (\x -> rest)`; an expression statement `e; rest`
desugars to `Monad.bind e (\_ -> rest)`, so it must have type `m unit`
(write `trace "msg" ()` for a trace inside a block). A `let` statement is
Haskell style: `let` followed by an aligned block of definitions and no
`in`; its scope is the rest of the block, and `let ... in e` on a statement
line is still an ordinary expression statement:

```elm
do
    let
        twice = x * 2
        name = "n"
    label name
    assert (twice > x)
```

A bind pattern that can fail is a canonicalization error
(`RefutableBindPattern`); only irrefutable patterns are allowed on the left
of `<-`. The top-level `do` of a test body uses the same statement syntax
but is sequenced with `let` instead of `bind` (see Tests block).

### Macro calls

```elm
json!({ a = 1 })
Cardano.Macros.address!("addr1...")
```

`name!(args)`: a lowercase (possibly qualified) name immediately followed by
`!(`, then comma-separated expression arguments. No whitespace is allowed
before `!`. The call node keeps its region; expansion happens in
`nash-macro` after type checking of the surrounding module.

### Records

Construction `{ x = 1, y = 2 }`, access `r.x`, accessor `.x` and update
`{ r | x = 1 }` are unchanged. Update requires `r` to be a variable (Elm
rule). Since records are nominal, the type of an update is the type of `r`.
`Datum { owner = o, deadline = d }` constructs a labeled-field constructor.
The parser reads it as the constructor applied to a record literal;
canonicalization turns it into labeled construction when `Datum` declares
labeled fields (the two forms cannot be told apart syntactically, and a
positional constructor taking an alias record is the other reading). Update
on a labeled constructor is not in v1. Labeled constructors are matched
positionally, `Datum owner deadline`, or by label with record-pattern sugar,
`Datum { owner, deadline }`. The sugar parses today as a constructor applied
to a record pattern; nash-can rewrites it to positional form when the
constructor has labeled fields and the names are a subset of the labels.

## Patterns

Unchanged from Elm minus char and float literals, plus bytes literals:

```elm
Constr 0 [I n, B bs]       -- Data constructors are ordinary constructors
#"deadbeef"                -- bytes literal pattern
x :: rest as all
{ owner, deadline }        -- record pattern (field punning)
```

`Data` is a prelude Big type `Constr int (list Data) | Map (list (pair Data
Data)) | List (list Data) | I int | B bytes`; its constructors need no
special grammar.

## Tests block

```elm
tests
    import Fuzz exposing (int, listOf)

    test "lt is strict" = do
        assert (not (lt 1 1))

    test "division by zero fails" fail = do
        assert (1 / 0 == 0)

    test "within budget" within (cpu 1000000, mem 5000) = do
        assert (expensive 10 == 55)

    prop "compare is antisymmetric" =
        let
            a via int
            b via int
        in
        do
            label (if a < b then "lt" else "ge")
            assert (compare a b == invert (compare b a))

    prop "sorted output" =
        let xs via listOf int in
        do
            let
                ys = sort xs
            n <- length ys
            assert (n == length xs)
            assert (isSorted ys)
```

- `tests` sits at column 1 and must be the last thing in the module. Its
  items form one layout block: all items align on the column of the first.
- A test or prop body is written with the `do` keyword, which is required.
  That top-level `do` is a *sequencing block*, not a monadic one: it uses
  the same statement syntax (`x <- e`, `let` statements, expression
  statements, last statement an expression) but desugars with plain `let`.
  `e; rest` with `e : unit` becomes `let () = e in rest`, and `x <- e; rest`
  becomes `let x = e in rest`. Any nested `do` (inside a statement, or a
  generator expression on the right of `via`) is an ordinary monadic `do`.
- Imports inside the block are scoped to the block and may pull in
  `testDependencies`.
- `test "name" [fail] [within (...)] = do block`. `fail` inverts the
  expectation. `within` takes one or two budgets, `cpu N` and `mem M`, in
  either order.
- `prop "name" [fail [once]] [within (...)] = ...`. `fail once` passes when
  at least one run fails. `once` after `fail` on a `test` item is a syntax
  error (`Test::OnceOnUnitTest`): `once` only makes sense for a property
  test, which runs many times.
- The `prop` body is `let binders in do block`. Binders are
  `pattern via expression` where the expression has type `fuzzer 'a`. The
  `do` after `in` is the sequencing block above. `prop` without a
  `via`-let is an error (use `test`), and `test` with one is an error (use
  `prop`).
- `label` is a prelude function, not syntax.
- Test names are string literals and must be unique per module (checked in
  canonicalization).

## Layout rules

Nash uses Elm's indentation model: a token at column 1 starts a new
top-level item; inside an item everything must be at column greater than the
enclosing indent. The new layout blocks follow the `let`/`case` rules:

| Block | Opens after | Item alignment | Ends when |
|---|---|---|---|
| `let` | `let` | column of first def | `in` |
| `case` | `of` | column of first pattern | next token not aligned |
| `do` (*new*) | `do` | column of first statement | next token not aligned |
| `let` statement in `do` (*new*) | `let` | column of first def | next aligned statement |
| `trait ... where` (*new*) | `where` | column of first signature | next token at column 1 |
| `impl ... where` (*new*) | `where` | column of first definition | next token at column 1 |
| `tests` (*new*) | `tests` | column of first import/test | end of file |
| test body `do` (*new*) | `do` after `=` or `in` | column of first statement | next item or end of file |

Alignment errors reuse Elm's shape: `DoAlignment(indent, row, col)`,
`TraitAlignment`, `ImplAlignment`, `TestsAlignment`. The body of a `test`
or `prop` is a `do` block indented past the item column; `= do` followed by
one statement on the next line is the common one-assertion form.

## Grammar (EBNF)

This is the complete grammar. Rules marked `(* new *)` or `(* changed *)`
differ from the Elm-derived grammar previously in `SPEC.md`.

### Notation

```
rule      = definition ;
( ... )   = grouping
[ ... ]   = optional
{ ... }   = zero or more
|         = alternation
"..."     = terminal string
'...'     = terminal char
```

### Lexical

```ebnf
digit          = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' ;
nonzero_digit  = '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' ;
hex_digit      = digit | 'a' | 'b' | 'c' | 'd' | 'e' | 'f'
                       | 'A' | 'B' | 'C' | 'D' | 'E' | 'F' ;
lower          = 'a' | ... | 'z' ;
upper          = 'A' | ... | 'Z' ;
inner_char     = lower | upper | digit | '_' ;

lower_var      = lower { inner_char } ;                (* not a reserved word *)
upper_var      = upper { inner_char } ;
qualified_var  = upper_var '.' ( lower_var | upper_var | qualified_var ) ;
qualified_upper = upper_var '.' ( upper_var | qualified_upper ) ;
module_name    = upper_var { '.' upper_var } ;
type_var       = "'" lower_var ;                       (* new *)

operator       = op_char { op_char } ;                 (* except . | -> = : => <- *)
op_char        = '+' | '-' | '*' | '/' | '=' | '.' | '<' | '>' | ':' | '&'
               | '|' | '^' | '?' | '%' | '!' | '$' ;

line_comment   = '--' { any_char - newline } ;
block_comment  = '{-' { any_char | block_comment } '-}' ;
doc_comment    = '{-|' { any_char | block_comment } '-}' ;
```

### Literals

```ebnf
number_literal = decimal_int | hex_int ;               (* '1.5' -> Number::Dot *)
decimal_int    = nonzero_digit { digit } | '0' ;
hex_int        = '0' ( 'x' | 'X' ) hex_digit { hex_digit } ;

string_literal = single_string | multi_string ;
single_string  = '"' { string_char | escape } '"' ;
multi_string   = '"""' { any_char | escape } '"""' ;
string_char    = (* any char except '"', '\', newline *) ;
escape         = '\' ( 'n' | 'r' | 't' | '"' | '\'' | '\' | unicode_escape ) ;
unicode_escape = 'u' '{' hex_digit hex_digit hex_digit hex_digit [ hex_digit [ hex_digit ] ] '}' ;

bytes_literal  = '#' '"' { hex_digit hex_digit } '"' ; (* new *)
```

### Types

```ebnf
type_expr      = type_app [ '->' type_expr ] ;
type_app       = type_head { type_term } ;             (* changed *)
type_head      = type_var | type_named ;               (* new *)
type_term      = type_var | type_named | type_tuple | type_record ;
type_named     = upper_var | lower_var | qualified_upper ;   (* changed *)
type_tuple     = '(' ')' | '(' type_expr ')'
               | '(' type_expr ':' repr ')'
               | '(' type_expr ',' type_expr { ',' type_expr } ')' ;
type_record    = '{' '}' | '{' type_field { ',' type_field } '}' ;  (* changed: no ext *)
type_field     = lower_var ':' type_expr ;

type_scheme    = [ context '=>' ] type_expr ;          (* new *)
context        = constraint | '(' constraint { ',' constraint } ')' ;
constraint     = ( upper_var | qualified_upper ) type_term { type_term } ;

type_param     = type_var | '(' type_var ':' repr ')' ; (* new *)
repr           = 'Big' | 'Const' | 'Term' | 'Storable' ;   (* sugar for a context entry *)
```

### Expressions

```ebnf
expression     = let_expr | case_expr | if_expr | lambda | do_expr
               | keyword_expr | binop_expr ;           (* changed *)
binop_expr     = possibly_neg_term { term } { operator binop_rhs } ;
binop_rhs      = possibly_neg_term { term } | let_expr | case_expr | if_expr
               | lambda | do_expr | keyword_expr ;     (* changed *)

let_expr       = 'let' let_def { let_def } 'in' expression ;   (* aligned defs *)
let_def        = definition | destructure ;
definition     = lower_var [ ':' type_scheme ] { pattern_term } '=' expression ;  (* changed *)
destructure    = pattern_term '=' expression ;
case_expr      = 'case' expression 'of' case_branch { case_branch } ; (* aligned *)
case_branch    = pattern_expr '->' expression ;
if_expr        = 'if' expression 'then' expression 'else' expression ;
lambda         = '\' pattern_term { pattern_term } '->' expression ;

do_expr        = 'do' stmt { stmt } ;                  (* new; aligned; last is expr_stmt *)
stmt           = let_stmt | bind_stmt | expr_stmt ;
let_stmt       = 'let' let_def { let_def } ;           (* aligned defs, no 'in' *)
bind_stmt      = pattern_expr '<-' expression ;
expr_stmt      = expression ;

keyword_expr   = 'assert' expression                   (* new *)
               | 'fail' [ term ]
               | 'todo' [ term ]
               | 'trace' term expression
               | 'comptime' expression ;

possibly_neg_term = '-' term | term ;
term           = ( variable [ macro_args ] | record | tuple ) { '.' lower_var }
               | string | bytes | number | list | accessor ;   (* changed *)
macro_args     = '!' '(' [ expression { ',' expression } ] ')' ; (* new; no space before '!' *)
accessor       = '.' lower_var ;
variable       = lower_var | upper_var | qualified_var ;
string         = string_literal ;
number         = number_literal ;
bytes          = bytes_literal ;                       (* new *)
list           = '[' [ expression { ',' expression } ] ']' ;
tuple          = '(' ')' | operator_section | '(' expression ')'
               | '(' expression ',' expression { ',' expression } ')' ;
operator_section = '(' operator ')'
                 | '(' operator expression ')'         (* right section; '-' is negation *)
                 | '(' expression operator ')' ;       (* left section *)
record         = '{' '}' | '{' field { ',' field } '}'
               | '{' lower_var '|' field { ',' field } '}' ;
field          = lower_var '=' expression ;
```

### Patterns

```ebnf
pattern_expr   = pattern_part { '::' pattern_part } [ 'as' lower_var ] ;
pattern_part   = ctor_pattern | pattern_term ;
ctor_pattern   = ( upper_var | qualified_upper ) { pattern_term } ;
pattern_term   = wildcard | lower_var | ctor_no_args | number | string | bytes
               | pattern_record | pattern_tuple | pattern_list ;  (* changed *)
wildcard       = '_' ;
ctor_no_args   = upper_var | qualified_upper ;
pattern_record = '{' '}' | '{' lower_var { ',' lower_var } '}' ;
pattern_tuple  = '(' ')' | '(' pattern_expr ')'
               | '(' pattern_expr ',' pattern_expr { ',' pattern_expr } ')' ;
pattern_list   = '[' ']' | '[' pattern_expr { ',' pattern_expr } ']' ;
```

### Declarations

```ebnf
declaration    = [ doc_comment ] { attribute }
                 ( type_decl | value_decl | trait_decl | impl_decl ) ;  (* changed *)
attribute      = '@' lower_var [ '(' [ expression { ',' expression } ] ')' ] ; (* new; own line *)

value_decl     = lower_var [ ':' type_scheme ] { pattern_term } '=' expression ; (* changed *)

type_decl      = 'type' ( alias_decl | union_decl ) ;
alias_decl     = 'alias' type_decl_name { type_param } '=' type_expr ;  (* changed *)
union_decl     = type_decl_name { type_param } '=' variant { '|' variant } ; (* changed *)
type_decl_name = upper_var | lower_var ;               (* new; not 'alias' *)
variant        = upper_var { type_term } | upper_var record_fields ; (* changed *)
record_fields  = '{' type_field { ',' type_field } '}' ; (* new: labeled fields *)

trait_decl     = 'trait' [ context '=>' ] upper_var type_param { type_param }
                 'where' { trait_item } ;              (* new; aligned block *)
trait_item     = lower_var ':' type_scheme
                 [ lower_var { pattern_term } '=' expression ] ;  (* same name *)

impl_decl      = 'impl' [ context '=>' ] ( upper_var | qualified_upper )
                 type_term { type_term } 'where' { impl_item } ;  (* new; aligned block *)
impl_item      = lower_var { pattern_term } '=' expression ;

infix_decl     = 'infix' associativity digit '(' operator ')' '=' lower_var ;
associativity  = 'left' | 'right' | 'non' ;
```

### Module

```ebnf
module         = [ module_header ] { import } { infix_decl } { declaration }
                 [ tests_block ] ;                     (* changed *)
module_header  = [ 'validator' ] 'module' module_name 'exposing' exposing ; (* changed *)
import         = 'import' module_name [ 'as' upper_var ] [ 'exposing' exposing ] ;

exposing       = '(' ( '..' | exposed { ',' exposed } ) ')' ;
exposed        = lower_var                             (* value *)
               | '(' operator ')'                      (* operator *)
               | upper_var [ '(' '..' ')' ]            (* Big type, optionally with constructors *)
               | 'type' lower_var [ '(' '..' ')' ] ;   (* new: little type *)

tests_block    = 'tests' { import } { test_item } ;    (* new; aligned block *)
test_item      = ( 'test' string_literal test_mods
                 | 'prop' string_literal prop_mods ) '=' test_body ;
test_body      = [ via_let ] 'do' block ;             (* via_let required on prop, forbidden on test *)
block          = stmt { stmt } ;                       (* aligned; last is expr_stmt; let-sequenced here *)
test_mods      = [ 'fail' ] [ within ] ;
prop_mods      = [ 'fail' [ 'once' ] ] [ within ] ;
within         = 'within' '(' budget [ ',' budget ] ')' ;
budget         = 'cpu' number_literal | 'mem' number_literal ;
via_let        = 'let' via_binder { via_binder } 'in' ;
via_binder     = pattern_term 'via' expression ;
```

## Error hierarchy additions

New error enums in `nash-parse/src/error.rs`, following Elm's nesting
(`Module` contains `Decl`, `Decl` contains `Trait`/`Impl`, `Expr` contains
`Do`/`Macro`/...). Each variant carries `Row, Col`; nested ones carry the
inner error:

| Parent | New variants |
|---|---|
| `Error` | (remove port/effect variants) |
| `Module` | `Validator`, `Tests(&Tests)`; remove `Port*`, `Effect` |
| `Exposing` | `TypeName` (uppercase or nothing after `type`) |
| `Decl` | `Attribute(&Attribute)`, `Trait(&Trait)`, `Impl(&Impl)`; remove `Port` |
| `DeclType` / `TypeAlias` / `CustomType` | `Param(&TypeParam)`; `CustomType::Field`, `FieldColon`, `FieldType` |
| `Type` | `Context`, `IndentAfterContext`, `VarStart` |
| `Expr` | `Do(&Do)`, `Macro(&Macro)`, `Bytes(Bytes)`, `Assert/Fail/Todo/Trace/Comptime(&Keyword)`; remove `Char`, `EndlessShader`, `ShaderProblem` |
| `Pattern` | `Bytes(Bytes)`; remove `Char`, `Float` |
| new | `Attribute`, `Trait`, `Impl`, `TypeParam`, `Kind`, `Keyword`, `Do`, `Macro`, `Tests`, `Test`, `Bytes` |

## Interactions

- **nash-can**: new source nodes (`Trait`, `Impl`, `Tests`, attributes,
  `Do`, `MacroCall`, keyword expressions, constraints, kinds, labeled
  constructor fields) are carried through as data; canonicalization of each
  is specified in `kinds.md`, `traits.md`, `testing.md`, `macros.md`. Until
  those plans land, nash-can rejects them with a typed `Unsupported` error
  so the workspace keeps compiling.
- **nash-fmt** and **nash-docs** consume the same AST; attributes and doc
  comments must survive round-trips, so they are stored on declarations,
  not dropped.
- **nash-macro** requires `Attribute.args` and `MacroCall.args` to be plain
  expressions with regions, which the grammar guarantees.

## Open questions

None. Record patterns `{ owner, deadline }` keep working on alias records
because field names resolve through the scrutinee's type, and the same
sugar applies to labeled constructors as described under Records.
