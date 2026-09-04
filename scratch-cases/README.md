# Scratch cases

These are personal manual tests, maintained separately from compiler plans and
release work. Run each command individually from the repository root. `pass/`
projects exit 0; `fail/` projects exit 1 with the indicated diagnostic. The
`check` commands compile the examples. The optional `build` command below writes
validator scripts; it does not run them against ledger inputs.

The cases cover diagnostics, pattern exhaustiveness and redundancy, records and
labels, Haskell 98 kinds, and representation predicates. Type self-application
fails the occurs check. Recursive datatype contexts use a terminating worklist;
constructed substitutions for applied-relevant parameters are rejected.
Representation predicates do not make identical impl heads disjoint. Arrow
kind annotations are removed; higher kinds are inferred.

Record aliases have nominal identity. Bare record literals need one visible alias
with the exact field set. Access needs a known receiver type. Labeled constructors
support reordered arguments and subset patterns; record updates require an alias.

Core-dependent cases are workspaces containing the real `core/` package and an
`app/` member. Imports are explicit. Other cases
are standalone applications. No Python or batch runner is required.

Pattern checks cover case branches, function and lambda arguments, let destructuring,
trait defaults and impl methods. Missing patterns report `MISSING PATTERNS`; unreachable
branches report `REDUNDANT PATTERN`. Literal branches need a fallback unless constructor
branches cover the type. Overloaded literals are treated conservatively.

## Diagnostics

These cases show terminal source highlights and independent-error recovery.
The mixed cases report the same five root errors in either declaration order.
Tuple and record selection cases preserve errors in unaffected fields. The
shared-variable case reports two root errors and suppresses dependent cascades.
The dependency case reports two naming errors, compiles `Good`, and skips
`Middle` and `Main` because `Broken` failed.

```sh
# Success: One unused-variable warning; warnings do not fail compilation
cargo run -p nash-cli -- check scratch-cases/pass/diagnostics/warnings

# TYPE MISMATCH + MISSING IMPL + MISSING CONSTRAINT + AMBIGUOUS TYPE + KIND MISMATCH: Five independent errors
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/mixed-errors

# KIND MISMATCH + AMBIGUOUS TYPE + MISSING CONSTRAINT + MISSING IMPL + TYPE MISMATCH: Reversed declarations retain all five errors
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/mixed-errors-reversed

# TYPE MISMATCH: Two errors; a failed field does not hide the other tuple slot
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/tuple-recovery

# TYPE MISMATCH: Two errors; selecting a healthy field still checks its annotation
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/record-selection-recovery

# TYPE MISMATCH: Two root errors; dependent calls through a changed variable stay suppressed
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/shared-variable-recovery

# NAMING ERROR: Two independent failed modules, two blocked dependents, one successful module
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/blocked-dependencies

# MISSING ELSE: An insertion caret at the end of the file
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/unfinished-expression

# NAMING ERROR: A missing name after a multibyte character
cargo run -p nash-cli -- check scratch-cases/fail/diagnostics/unicode-location
```

Use these variants to inspect JSON and warning controls:

```sh
cargo run -p nash-cli -- check --report=json scratch-cases/fail/diagnostics/mixed-errors
cargo run -p nash-cli -- check --report=json scratch-cases/pass/diagnostics/warnings
cargo run -p nash-cli -- check --no-warnings scratch-cases/pass/diagnostics/warnings
cargo run -p nash-cli -- check --color=never scratch-cases/fail/diagnostics/tuple-recovery
```

JSON errors go to stdout; JSON warnings go to stderr. A warning-only build
prints an empty `compile-errors` object on stdout and exits 0. Failed builds
exit 1. Blocked modules do not produce fabricated naming errors.

## Parser errors

There are **35 parser-error projects**: the 33 cases below, the unfinished `if`
case in Diagnostics, and the arrow-representation case under Removed
syntax. The parser reports only the first syntax error per module; a build can report
syntax errors from multiple modules. These fail during parsing, before name or
type checking. Each command
exits 1 and shows a source highlight; `--report=json` includes the stated title. Some files omit
the final newline deliberately to exercise end-of-file carets.

### Module headers and imports

```sh
# EXPECTED MODULE NAME: Module keyword without a name
cargo run -p nash-cli -- check scratch-cases/fail/syntax/module-problem

# EXPECTED MODULE NAME: Lowercase module name
cargo run -p nash-cli -- check scratch-cases/fail/syntax/module-name-lowercase

# MISSING EXPOSING PARENTHESIS: Missing parentheses around exports
cargo run -p nash-cli -- check scratch-cases/fail/syntax/exposing-missing-paren

# EXPECTED EXPOSED NAME: Invalid item in an exposing list
cargo run -p nash-cli -- check scratch-cases/fail/syntax/exposing-value-bad

# EXPECTED IMPORT NAME: Import keyword without a module
cargo run -p nash-cli -- check scratch-cases/fail/syntax/import-missing-name

# EXPECTED IMPORT ALIAS: Lowercase import alias
cargo run -p nash-cli -- check scratch-cases/fail/syntax/import-bad-alias

# MISSING EXPOSING PARENTHESIS: Missing parentheses around imported names
cargo run -p nash-cli -- check scratch-cases/fail/syntax/import-exposing-list-missing-paren

```

### Whitespace and declaration boundaries

```sh
# NO TABS: Tab character in source indentation
cargo run -p nash-cli -- check scratch-cases/fail/syntax/space-has-tab

# ENDLESS COMMENT: Unclosed block comment
cargo run -p nash-cli -- check scratch-cases/fail/syntax/space-endless-comment

# UNEXPECTED SYMBOL: Two definitions on one line
cargo run -p nash-cli -- check scratch-cases/fail/syntax/fresh-line-after-decl

```

### Declarations

```sh
# MISSING EQUALS: Type alias without an equals sign
cargo run -p nash-cli -- check scratch-cases/fail/syntax/type-alias-missing-equals

# EXPECTED TYPE: Integer used as a type alias body
cargo run -p nash-cli -- check scratch-cases/fail/syntax/type-alias-bad-body

# MISSING VARIANT: Custom type without a constructor
cargo run -p nash-cli -- check scratch-cases/fail/syntax/custom-type-missing-variant

# MISSING EQUALS: Definition without an equals sign
cargo run -p nash-cli -- check scratch-cases/fail/syntax/decl-def-missing-equals

# NAME MISMATCH: Annotation and definition names differ
cargo run -p nash-cli -- check scratch-cases/fail/syntax/decl-def-name-match

# MISSING EXPRESSION: Definition body starts at the wrong indentation
cargo run -p nash-cli -- check scratch-cases/fail/syntax/decl-def-indent-body

```

### Patterns

```sh
# UNFINISHED PATTERN: Pattern alias without a name
cargo run -p nash-cli -- check scratch-cases/fail/syntax/pattern-alias-missing-name

# UNEXPECTED NAME: Name attached to a wildcard
cargo run -p nash-cli -- check scratch-cases/fail/syntax/pattern-wildcard-not-var

# UNCLOSED DELIMITER: Unclosed record pattern
cargo run -p nash-cli -- check scratch-cases/fail/syntax/pattern-record-missing-end

# UNCLOSED DELIMITER: Unclosed tuple pattern
cargo run -p nash-cli -- check scratch-cases/fail/syntax/pattern-tuple-missing-end

# UNCLOSED DELIMITER: Unclosed list pattern
cargo run -p nash-cli -- check scratch-cases/fail/syntax/pattern-list-missing-end

```

### Type annotations

```sh
# EXPECTED TYPE: Integer used as a type annotation
cargo run -p nash-cli -- check scratch-cases/fail/syntax/type-start-bad-in-annotation

# MISSING COLON: Record type field without a colon
cargo run -p nash-cli -- check scratch-cases/fail/syntax/type-record-missing-colon

# UNCLOSED DELIMITER: Unclosed tuple type
cargo run -p nash-cli -- check scratch-cases/fail/syntax/type-tuple-missing-end

```

### Expressions and literals

```sh
# MISSING ARROW: Case branch uses the wrong arrow
cargo run -p nash-cli -- check scratch-cases/fail/syntax/case-wrong-arrow

# RESERVED WORD: Reserved word used as a record field
cargo run -p nash-cli -- check scratch-cases/fail/syntax/record-reserved-field

# MISSING EXPRESSION: Trailing comma in a list
cargo run -p nash-cli -- check scratch-cases/fail/syntax/list-trailing-comma

# MISSING RESULT: Do block ends with a binding
cargo run -p nash-cli -- check scratch-cases/fail/syntax/do-last-binding

# UNCLOSED DELIMITER: Unclosed macro arguments
cargo run -p nash-cli -- check scratch-cases/fail/syntax/macro-missing-close

# BAD BYTE STRING: Non-hexadecimal byte-string digit
cargo run -p nash-cli -- check scratch-cases/fail/syntax/bytes-invalid-hex

# BAD UNICODE ESCAPE: Unicode escape has too few digits
cargo run -p nash-cli -- check scratch-cases/fail/syntax/unicode-short-escape

# MISSING IN: Let expression without in
cargo run -p nash-cli -- check scratch-cases/fail/syntax/let-missing-in

# MISSING EXPRESSION: Lambda without a body
cargo run -p nash-cli -- check scratch-cases/fail/syntax/lambda-missing-body

```

## Pattern checks

```sh
# Success: Complete Boolean branches
cargo run -p nash-cli -- check scratch-cases/pass/patterns/case-bool-complete

# Success: Safe record, unit, tuple and single-constructor arguments
cargo run -p nash-cli -- check scratch-cases/pass/patterns/arguments-irrefutable

# Success: Safe tuple destructuring in let
cargo run -p nash-cli -- check scratch-cases/pass/patterns/let-destructure-safe

# Success: Complete cases in trait defaults and impl methods
cargo run -p nash-cli -- check scratch-cases/pass/patterns/trait-and-impl-complete

# Success: Empty and nonempty list coverage
cargo run -p nash-cli -- check scratch-cases/pass/patterns/list-complete

# Success: Subset labels cover the single constructor
cargo run -p nash-cli -- check scratch-cases/pass/patterns/labeled-subset-irrefutable

# Success: Nested Data list coverage with a fallback
cargo run -p nash-cli -- check scratch-cases/pass/patterns/data-fields-list-complete

# Success: Overloaded literals between complete constructor branches
cargo run -p nash-cli -- check scratch-cases/pass/patterns/mixed-literal-and-constructor-patterns

# MISSING PATTERNS: Missing False branch
cargo run -p nash-cli -- check scratch-cases/fail/patterns/case-bool-missing

# MISSING PATTERNS: Lists with two or more elements are missing
cargo run -p nash-cli -- check scratch-cases/fail/patterns/nested-list-missing

# MISSING PATTERNS: Missing correlated Boolean tuple
cargo run -p nash-cli -- check scratch-cases/fail/patterns/tuple-correlation-missing

# MISSING PATTERNS: Missing alternative to a labeled constructor
cargo run -p nash-cli -- check scratch-cases/fail/patterns/labeled-multi-constructor-missing

# MISSING PATTERNS: Byte literals need a fallback
cargo run -p nash-cli -- check scratch-cases/fail/patterns/bytes-need-wildcard

# MISSING PATTERNS: Integer literals need a fallback
cargo run -p nash-cli -- check scratch-cases/fail/patterns/int-need-wildcard

# MISSING PATTERNS: String literals need a fallback
cargo run -p nash-cli -- check scratch-cases/fail/patterns/string-need-wildcard

# MISSING PATTERNS: Missing Data constructors
cargo run -p nash-cli -- check scratch-cases/fail/patterns/data-missing-constructors

# MISSING PATTERNS: Literal Data tags leave other tags uncovered
cargo run -p nash-cli -- check scratch-cases/fail/patterns/data-tag-literals-need-wildcard

# UNSAFE PATTERN: List argument excludes the empty list
cargo run -p nash-cli -- check scratch-cases/fail/patterns/untyped-arg-unsafe

# UNSAFE PATTERN: Lambda argument excludes False
cargo run -p nash-cli -- check scratch-cases/fail/patterns/lambda-arg-unsafe

# UNSAFE PATTERN: Let destructuring excludes the empty list
cargo run -p nash-cli -- check scratch-cases/fail/patterns/let-destructure-unsafe

# MISSING PATTERNS: Trait default body misses False
cargo run -p nash-cli -- check scratch-cases/fail/patterns/trait-default-checked

# UNSAFE PATTERN: Impl argument excludes False
cargo run -p nash-cli -- check scratch-cases/fail/patterns/impl-argument-checked

# REDUNDANT PATTERN: Branch after wildcard is unreachable
cargo run -p nash-cli -- check scratch-cases/fail/patterns/redundant-after-wildcard

# REDUNDANT PATTERN: Fallback after complete constructors is unreachable
cargo run -p nash-cli -- check scratch-cases/fail/patterns/redundant-after-all-constructors

# REDUNDANT PATTERN: Subset labels already cover positional branch
cargo run -p nash-cli -- check scratch-cases/fail/patterns/labeled-omitted-fields-cover-positional-patterns

# REDUNDANT PATTERN: Repeated byte literal is unreachable
cargo run -p nash-cli -- check scratch-cases/fail/patterns/bytes-duplicate

# REDUNDANT PATTERN: General Data constructor already covers literal tag
cargo run -p nash-cli -- check scratch-cases/fail/patterns/data-tag-redundant

# REDUNDANT PATTERN: Complete constructors already cover overloaded literal
cargo run -p nash-cli -- check scratch-cases/fail/patterns/overloaded-literal-after-complete-constructors
```

## Passing cases

```sh
# Success: Known alias access, update, patterns and transparent wrappers
cargo run -p nash-cli -- check scratch-cases/pass/nominal-records

# Success: Explicit constructors distinguish aliases with identical fields
cargo run -p nash-cli -- check scratch-cases/pass/record-constructor-disambiguation

# Success: Imported lowercase alias constructors and field-set literals
cargo run -p nash-cli -- check scratch-cases/pass/qualified-records

# Success: Captured record projection keeps the outer parameter shared
cargo run -p nash-cli -- check scratch-cases/pass/record-capture

# Success: Out-of-order labeled arguments, subset patterns and projection
cargo run -p nash-cli -- check scratch-cases/pass/labeled-constructors

# Success: Parenthesized records remain positional constructor arguments
cargo run -p nash-cli -- check scratch-cases/pass/labeled-record-argument

# Success: Imported labeled sugar and higher-kinded projection
cargo run -p nash-cli -- check scratch-cases/pass/imported-labeled-applications

# Success: Big labeled constructors, label sugar and case patterns on several constructors
cargo run -p nash-cli -- check scratch-cases/pass/labeled-big-constructors

# Success: Label sugar and subset case patterns on a little multi-constructor union
cargo run -p nash-cli -- check scratch-cases/pass/labeled-multi-constructors

# Success: Builtin-qualified types and unit identity without imports
cargo run -p nash-cli -- check scratch-cases/pass/builtin-inventory

# Success: Value annotations preserve Storable alias requirements
cargo run -p nash-cli -- check scratch-cases/pass/annotation-pass

# Success: Byte literal syntax infers a FromBytes constraint
cargo run -p nash-cli -- check scratch-cases/pass/bytes

# Success: Shipping option/result do and Prelude operators
cargo run -p nash-cli -- check scratch-cases/pass/core-do

# Success: Literal defaulting, arithmetic, ordering, Show and equality
cargo run -p nash-cli -- check scratch-cases/pass/core-traits

# Success: Big/little ADTs, aliases, records and recursive declarations
cargo run -p nash-cli -- check scratch-cases/pass/declarations

# Success: Imported alias context accepts Big
cargo run -p nash-cli -- check scratch-cases/pass/import-pass

# Success: Higher-kinded applications across imports, annotations and impl heads
cargo run -p nash-cli -- check scratch-cases/pass/imported-applications

# Success: Direct and transitive trait-constrained consumers
cargo run -p nash-cli -- check scratch-cases/pass/imported-traits

# Success: Mixed Storable pair components and projections
cargo run -p nash-cli -- check scratch-cases/pass/pair-const

# Success: Inferred higher kinds and explicit representation predicates
cargo run -p nash-cli -- check scratch-cases/pass/parameters

# Success: Partially applied pair satisfies supplied and remaining contexts
cargo run -p nash-cli -- check scratch-cases/pass/partial-applications

# Success: Regular parameter permutation and nested recursive contexts
cargo run -p nash-cli -- check scratch-cases/pass/recursive-contexts

# Success: Nested concrete impl heads and repeated-variable matching
cargo run -p nash-cli -- check scratch-cases/pass/recursive-impls

# Success: Surface syntax with explicit core literal imports
cargo run -p nash-cli -- check scratch-cases/pass/syntax

# Success: Trait and impl declarations
cargo run -p nash-cli -- check scratch-cases/pass/traits

# Success: Transparent alias contexts and nominal record-body bounds
cargo run -p nash-cli -- check scratch-cases/pass/transparent-aliases

# Success: Validator module; the optional build command below emits scripts
cargo run -p nash-cli -- check scratch-cases/pass/validator

# Success: Higher-kinded value annotations
cargo run -p nash-cli -- check scratch-cases/pass/value-application

```

## Build validator scripts

```sh
cargo run -p nash-cli -- build scratch-cases/pass/validator --out build
```

This writes `Main.uplc`, `Main.flat`, and `Main.cbor` under
`scratch-cases/pass/validator/build/`. The current example reports two unused
argument warnings and still succeeds. Its Flat output is 9 bytes. The build
files are personal inspection artifacts and can be regenerated with this command.
`Main.cbor` contains hex text of the single CBOR wrapper around the Flat bytes.
`.nash-artifacts` records the generated files so later builds can replace or
remove them safely.

## Kind mismatches

```sh
# KIND MISMATCH: base application
cargo run -p nash-cli -- check scratch-cases/fail/kind-mismatch/base-application

# KIND MISMATCH: higher value
cargo run -p nash-cli -- check scratch-cases/fail/kind-mismatch/higher-value

# KIND MISMATCH: little record arrow
cargo run -p nash-cli -- check scratch-cases/fail/kind-mismatch/little-record-arrow

# KIND MISMATCH: parameter arrow
cargo run -p nash-cli -- check scratch-cases/fail/kind-mismatch/parameter-arrow

```

## Infinite kinds

```sh
# INFINITE KIND: self-application fails at the declaration
cargo run -p nash-cli -- check scratch-cases/fail/infinite-kind/abstract-self-application

# INFINITE KIND: infinite
cargo run -p nash-cli -- check scratch-cases/fail/infinite-kind/infinite

# INFINITE KIND: self-application fails at the declaration
cargo run -p nash-cli -- check scratch-cases/fail/infinite-kind/retained-self-application

```

## Recursive datatype contexts

```sh
# IRREGULAR RECURSION: recursive application would grow its context
cargo run -p nash-cli -- check scratch-cases/fail/recursive-contexts/growing-parameter

# IRREGULAR RECURSION: a parameter becomes applied-relevant through another declaration
cargo run -p nash-cli -- check scratch-cases/fail/recursive-contexts/late-relevance

```

## Representation requirements

```sh
# REPRESENTATION MISMATCH: alias contract
cargo run -p nash-cli -- check scratch-cases/fail/representation/alias-contract

# REPRESENTATION MISMATCH: annotation term
cargo run -p nash-cli -- check scratch-cases/fail/representation/annotation-term

# REPRESENTATION MISMATCH: big alias const
cargo run -p nash-cli -- check scratch-cases/fail/representation/big-alias-const

# REPRESENTATION MISMATCH: big field const
cargo run -p nash-cli -- check scratch-cases/fail/representation/big-field-const

# REPRESENTATION MISMATCH: big field tuple
cargo run -p nash-cli -- check scratch-cases/fail/representation/big-field-tuple

# REPRESENTATION MISMATCH: big record const
cargo run -p nash-cli -- check scratch-cases/fail/representation/big-record-const

# REPRESENTATION MISMATCH: container term
cargo run -p nash-cli -- check scratch-cases/fail/representation/container-term

# REPRESENTATION MISMATCH: import fail
cargo run -p nash-cli -- check scratch-cases/fail/representation/import-fail

# MISSING IMPL: inferred apply
cargo run -p nash-cli -- check scratch-cases/fail/representation/inferred-apply

# CONTRADICTORY REPRESENTATION: inferred contradiction
cargo run -p nash-cli -- check scratch-cases/fail/representation/inferred-contradiction

# MISSING IMPL: inline bound
cargo run -p nash-cli -- check scratch-cases/fail/representation/inline-bound

# MISSING IMPL: list map term
cargo run -p nash-cli -- check scratch-cases/fail/representation/list-map-term

# REPRESENTATION MISMATCH: little alias big
cargo run -p nash-cli -- check scratch-cases/fail/representation/little-alias-big

# REPRESENTATION MISMATCH: nested alias contract
cargo run -p nash-cli -- check scratch-cases/fail/representation/nested-alias-contract

# REPRESENTATION MISMATCH: pair term
cargo run -p nash-cli -- check scratch-cases/fail/representation/pair-term

# CONTRADICTORY REPRESENTATION: parameter base
cargo run -p nash-cli -- check scratch-cases/fail/representation/parameter-base

# REPRESENTATION MISMATCH: partial argument
cargo run -p nash-cli -- check scratch-cases/fail/representation/partial-argument

# CONTRADICTORY REPRESENTATION: recursive parameter
cargo run -p nash-cli -- check scratch-cases/fail/representation/recursive-parameter

```

## Trait resolution and coherence

```sh
# STRUCTURAL EQUALITY: big eq override
cargo run -p nash-cli -- check scratch-cases/fail/traits/big-eq-override

# MISSING IMPL: list monad
cargo run -p nash-cli -- check scratch-cases/fail/traits/list-monad

# MISSING IMPL: missing impl
cargo run -p nash-cli -- check scratch-cases/fail/traits/missing-impl

# MISSING SUPERCLASS: missing superclass
cargo run -p nash-cli -- check scratch-cases/fail/traits/missing-superclass

# ORPHAN IMPL: orphan
cargo run -p nash-cli -- check scratch-cases/fail/traits/orphan

# OVERLAPPING IMPL: overlap
cargo run -p nash-cli -- check scratch-cases/fail/traits/overlap

# MISSING IMPL: pair functor
cargo run -p nash-cli -- check scratch-cases/fail/traits/pair-functor

# MISSING IMPL: repeated variable
cargo run -p nash-cli -- check scratch-cases/fail/traits/repeated-variable

# OVERLAPPING IMPL: Big/Const contexts cannot distinguish identical heads
cargo run -p nash-cli -- check scratch-cases/fail/traits/representation-overlap

```

## Do patterns

```sh
# UNSAFE PATTERN: refutable pattern
cargo run -p nash-cli -- check scratch-cases/fail/do/refutable-pattern

```

## Named constructor arity

```sh
# TOO MANY TYPE ARGS: named arity
cargo run -p nash-cli -- check scratch-cases/fail/arity/named-arity

```

## Removed syntax

```sh
# REPRESENTATION ARROW: arrow representation annotation is invalid
cargo run -p nash-cli -- check scratch-cases/fail/syntax/arrow-representation

```

## Records and labeled constructors

```sh
# RECORD TYPE: Anonymous record annotations require a named alias
cargo run -p nash-cli -- check scratch-cases/fail/records/anonymous-type

# RECORD TYPE: Nested anonymous record types require their own aliases
cargo run -p nash-cli -- check scratch-cases/fail/records/nested-anonymous-type

# UNKNOWN RECORD: Record literals need a visible alias with the exact field set
cargo run -p nash-cli -- check scratch-cases/fail/records/literal-no-alias

# AMBIGUOUS RECORD: Identical alias field sets make a bare literal ambiguous
cargo run -p nash-cli -- check scratch-cases/fail/records/literal-ambiguous

# TYPE MISMATCH: Identical fields do not make different aliases interchangeable
cargo run -p nash-cli -- check scratch-cases/fail/records/nominal-identity

# AMBIGUOUS RECORD ACCESS: Field access cannot generalize an unknown receiver
cargo run -p nash-cli -- check scratch-cases/fail/records/unknown-receiver

# TYPE MISMATCH: A known alias must contain the requested field
cargo run -p nash-cli -- check scratch-cases/fail/records/missing-field

# TYPE MISMATCH: An update must preserve the declared field type
cargo run -p nash-cli -- check scratch-cases/fail/records/update-field-type

# MISSING FIELD: Labeled construction requires every declared field
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-missing-field

# UNKNOWN FIELD: Labeled construction rejects undeclared fields
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-extra-field

# NAME CLASH: Constructor declarations reject duplicate labels
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-duplicate-label

# UNKNOWN FIELD: Subset patterns reject unknown constructor labels
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-unknown-pattern

# TYPE MISMATCH: Projection requires exactly one labeled constructor
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-multi-constructor

# TYPE MISMATCH: Big unions with several labeled constructors also reject projection
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-big-multi-constructor

# TYPE MISMATCH: Record updates remain alias-only
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-update

# NAME CLASH: Big and little twins must have identical ordered labels
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-twin-order

# TYPE MISMATCH: A captured field cannot generalize independently of its receiver
cargo run -p nash-cli -- check scratch-cases/fail/records/captured-field

# TYPE MISMATCH: Closed type exports hide constructor labels
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-hidden-export

# TYPE MISMATCH: Exported values do not expose private constructor labels
cargo run -p nash-cli -- check scratch-cases/fail/records/labeled-private-return

```

## Unsupported features

```sh
# NOT SUPPORTED: tests
cargo run -p nash-cli -- check scratch-cases/fail/unsupported/tests

```

Verified on 2026-09-18 against compiler commit `bac182ab`: **157 projects,
37 successful compilations and 120 expected failures**. Each failure was checked
for its documented JSON diagnostic title, not only its exit status. The validator
build also succeeded and regenerated its three script files and ownership manifest. These results are
personal testing notes, not plan acceptance criteria.
