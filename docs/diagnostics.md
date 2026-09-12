# Diagnostics

## Purpose

Every compiler phase already records *why* something failed as plain data
(`nash_parse::error::*`, `nash_can::Error`, `nash_constrain::Error`,
`nash_nitpick::Error`, ...). None of them render text. One crate,
`nash-report`, turns that data into human prose, terminal output, JSON, and
LSP diagnostics.

The prose is Elm's. `Reporting/Error/*.hs`, `Reporting/Doc.hs`,
`Reporting/Render/Type.hs`, `Type/Error.hs`, and `Reporting/Suggest.hs`
are ported function by function; message text is kept nearly verbatim, with
edits only where Nash differs (no `Float`, no `Char`, Big/little types,
traits instead of `number`/`comparable`, `todo` instead of `Debug.todo`).
The terminal layout is [miette](https://docs.rs/miette)'s graphical
format, not Elm's `-- TITLE ---- path` bars, so Nash errors look like the
rest of the Rust-tooling world while reading like Elm.

## Concepts

### Collect independent errors before rendering

A check collects as many independent errors as it can safely establish, then
renders the complete diagnostic set. One bad definition must not suppress
unrelated type, kind, trait, or ambiguity errors elsewhere in the module or
in independent modules. Do not stop merely because the error list is nonempty.

Recovery must preserve sound inference state. Mark failed expressions and
constraints so that their dependent uses do not generate misleading follow-on
errors; continue at valid definition, constraint, and dependency boundaries.
Do not remove suppression guards without replacing their dependency tracking.
Preserve the known shape of a tuple or record when one child fails, so unaffected
children can still be checked against their annotations. Record selection depends
on the receiver's shape and the selected field; failure in an unrelated field
must not suppress a mismatch or trait obligation on the selected field. Changes
to shared inference variables still invalidate every computation that uses them.
If parsing or canonicalization cannot produce valid input for the next phase,
stop that module at that phase and continue independent modules. A failed
module exports neither an interface nor successful solved output. Mark its
dependents as blocked by the original failure instead of inventing missing
imports or missing types.

Resolution limits stop the affected computation and produce a diagnostic;
they do not discard errors already collected or suppress independent work.
Never silently truncate the result. Any resource limit on diagnostic collection
must explicitly report that additional errors may remain.

Order reports by canonical module path, primary source span, and stable
diagnostic identifiers and tie-breakers. Hash iteration order, pointer values,
and NodeId are not presentation order. Terminal, JSON, and LSP output expose
the same problems and primary spans. Keep distinct root errors even when they
share a region; deduplicate only diagnostics known to have the same cause.

### Report

Nash uses concise, direct messages inspired by Alder: state the problem,
show expected and actual types, and add at most one useful hint. Source labels
carry context instead of repeating it in paragraphs.

```rust
pub struct Report {
    pub code: &'static str,       // stable machine identity, independent of title
    pub title: String,           // short display title
    pub severity: Severity,
    pub region: Region,          // primary span in all output formats
    pub primary_label: Option<String>,
    pub labels: Vec<Label>,      // secondary locations in this source
    pub context: Option<Region>, // optional surrounding source, never a label
    pub related: Vec<ModuleReports>, // reports with their own source files
    pub before: Doc,             // direct problem and full type comparison
    pub after: Doc,              // supporting details and one useful hint
    pub suggestions: Vec<String>,
}
pub struct Label { pub region: Region, pub text: String }
```

Codes use explicit `nash::names::*`, `nash::type::*`, `nash::pattern::*`,
`nash::warning::*`, and `nash::syntax` identifiers. Syntax has specific codes
for module names, closing delimiters, and indentation. Editing a report's
display title does not change its code. `nash::diagnostic` is the default for
manually constructed reports; compiler phase entry points assign codes.

Annotations retain their type region, and list/branch expectations retain the
previous sibling's region. Reports label these as `declared type`, `previous
list element`, or `previous branch`. A sibling label shows the comparison
context; it does not claim that every part of an inferred type originated there.
Delimiter errors retain opening positions from the parser, including attribute,
macro, exposing, type-parameter, constructor-field, and test-budget parentheses
or braces. Lexer errors also retain the opening quote or block-comment marker
and the insertion boundary. Indentation errors distinguish an existing closer
from a missing one.

`Report` owns everything. It is produced from arena-allocated error values
and outlives the module's `Bump`, so the driver can collect reports for
all modules and render them at the end.

### Doc

`Reporting/Doc.hs` wraps a Wadler-style pretty printer. Nash keeps a small
one so that (a) `reflow` wraps at 80 columns exactly as Elm does, (b) type
renderings (`Render/Type.hs`) break long signatures the same way, and (c)
the same tree emits ANSI for terminals and styled chunks for JSON.

```rust
pub enum Doc {
    Empty,
    Text(String),              // never contains '\n'
    Styled(Style, Box<Doc>),
    Cat(Vec<Doc>),             // horizontal
    Nest(usize, Box<Doc>),     // indent lines broken inside
    Align(Box<Doc>),           // indent to current column
    Line,                      // ' ' flat, newline broken
    LineBreak,                 // '' flat, newline broken
    HardLine,                  // always newline (vcat)
    Group(Box<Doc>),           // try flat, else break
    Fill(Vec<Doc>),            // fillSep: greedy word wrap
}

pub struct Style { pub bold: bool, pub underline: bool, pub color: Option<Color> }
pub struct Color { pub base: BaseColor, pub vivid: bool }   // Elm: yellow vs YELLOW
```

Helpers mirror Elm's names: `reflow`, `stack`, `indent`, `vcat`, `hsep`,
`fill_sep`, `dullyellow`, `yellow`, `green`, `cyan`, `dullcyan`, `red`,
`dullred`, `blue`, `black`, `underline`, `to_simple_note`,
`to_fancy_note`, `to_simple_hint`, `to_fancy_hint`, `link`, `fancy_link`,
`reflow_link`, `comma_sep`, `args`, `more_args`, `ordinal`,
`int_to_ordinal`, `cycle`.

Style use is exactly Elm's: `dullyellow` for the parts of a type that
differ, `green` for suggested replacements, `cyan` for keywords in example
code, `red` for carets, `underline` for `Hint:` / `Note:`.

Links point at `https://nash-script.dev/hints/<name>` (Elm:
`https://elm-lang.org/0.19.1/<name>`). Hint page names are kept
(`imports`, `type-annotations`, `custom-types`, `missing-patterns`,
`bad-recursion`, `shadowing`, ...).

### Localizer

`Reporting/Render/Type/Localizer.hs`. A type name is rendered the way the
user could write it in *this* module: bare if the type is exposed by an
import (or is defined locally), `Alias.Name` if the module is imported
with an alias, `Module.Name` otherwise.

```rust
pub struct Localizer { imports: BTreeMap<String, Import> }
struct Import { alias: Option<String>, exposing: Exposing }
enum Exposing { All, Only(BTreeSet<String>) }

impl Localizer {
    pub fn from_module(module: &nash_source::Module<'_>, defaults: &[&nash_source::Import<'_>]) -> Localizer;
    pub fn to_string(&self, home: ModuleName<'_>, name: &str) -> String;
}
```

`defaults` are the implicit imports canonicalization prepends
(`docs/stdlib.md`, "Default imports": `import Prelude exposing (..)` plus
qualified `Int`, `List`, `Data`, ...). They are registered with their real
exposing, so the dual prelude types (`Int`, `int`, `List`, `list`, `Data`,
`option`, ...) render bare through the open `Prelude` import and, say,
`Data.Map.Map` renders as `Data.Map.Map` unless the user exposed it. That
subsumes Elm's hard-coded `List` special case.

Compiler primitive types from `nash_ast::primitives::PRIMITIVES` are already
available without imports. The localizer includes that current inventory and
keeps shadowed primitives qualified. Prelude default imports remain Plan 12;
the driver currently supplies no future default imports. Local union ownership
and package identity are retained for actionable impl advice.

The localizer is built once per module by the driver from the *source*
module (it only needs the import list) and is threaded to every type
error report, the same way `Reporting.Error.BadTypes` carries it.

### Type diff

`Type/Error.hs` `toDiff` / `toComparison` compare two `ErrorType`s
structurally, render both with the differing sub-terms in `dullyellow`,
and collect `Problem`s that drive hints. Nash keeps the machinery and
swaps the problem set:

| Elm `Problem` | Nash |
|---|---|
| `IntFloat`, `StringFromInt`, `StringFromFloat`, `StringToInt`, `StringToFloat` | dropped (no `Float`, literals are trait-polymorphic) |
| `AnythingToBool` | kept; the type is `bool` |
| `AnythingFromMaybe` | `AnythingFromOption` (`option 'a` vs `'a`) |
| `ArityMismatch`, `BadRigidVar`, `FieldTypo`, `FieldsMissing` | kept |
| `BadFlexSuper`, `BadRigidSuper` | removed with `Super`; the trait solver reports unsatisfied constraints as `MISSING IMPL` (below) |
| — | `BigLittle { name }`: `Int` vs `int`, `Bytes` vs `bytes`, `List Int` vs `list int`, `Map`/`pair`, Big record vs little record |

The full comparison is rendered in `before` as expected and actual types.
Structural differences retain their colors and full type detail. `after` uses
the first applicable type-difference hint, or one context-specific hint.

### Suggest

`Reporting/Suggest.hs`: restricted Damerau-Levenshtein distance,
case-insensitive `sort` and `rank`. Used for "Similar names:" lists (naming errors, record field typos, unknown exports,
unknown module imports). No external crate; the distance is ~25 lines.

## Rendering to the terminal

`Report::render(&self, source: &Source, path: &str, color: bool) -> Rendered`
produces a value implementing `miette::Diagnostic`:

| miette | from `Report` |
|---|---|
| `code()` | omitted in terminal output; retained in JSON and LSP |
| `severity()` | `severity` |
| `Display` (the `×` line) | `before` rendered at 80 columns |
| `labels()` | primary region and secondary labels, converted to byte spans by `Source` |
| `related()` | related reports rendered with their own source files |
| `help()` | `after` rendered at 80 columns, multi-line |
| `source_code()` | `NamedSource::new(path, source)` |

The CLI installs a handler built with `MietteHandlerOpts::new()
.width(80).wrap_lines(false)`, because `Doc` has already wrapped and
indented the text; miette only adds the `help:` prefix and indentation. Color is
decided once by the CLI (`--color`, `NO_COLOR`, `isatty`) and passed to
both the `Doc` renderer and miette's theme.

Driver-level errors (`nash_driver::DriverError`: file not found, config
problems, import cycles) already derive `miette::Diagnostic` via
`thiserror` and keep doing so; they are not `Report`s.

The following examples are checked against the shipping `core/` package by the
CLI integration tests. Output uses `--color=never --no-warnings`; only the
project path is shortened to `src/`. `map` currently comes from `Functor`; a
future `List` convenience module is not assumed.

### Example 1 — type mismatch with a diff

```elm
module Ledger exposing (settle)
import Functor exposing (Functor)

type alias Account = { owner : Bytes, balance : Int }

balanceOf : Account -> Int
balanceOf account = account.balance

settle : list Account -> list int
settle accounts =
    map balanceOf accounts
```

```text
  × Type mismatch: expected `list int`, found `list Int`.
    ╭─[src/Ledger.nash:11:5]
  8 │ 
  9 │ settle : list Account -> list int
    ·          ────────────┬───────────
    ·                      ╰── declared type
 10 │ settle accounts =
 11 │     map balanceOf accounts
    ·     ───────────┬──────────
    ·                ╰── body of `settle`
    ╰────
  help: Use `lower` to convert `Int` to `int` where a `Lift` impl is available.
```

### Example 2 — missing impl

```elm
module Steps exposing (isDone)
import Prelude exposing ((==))
import Eq exposing (Eq)

type step = Done | Next int

isDone : step -> bool
isDone s = s == Done
```

```text
  × No impl for `Eq step`.
   ╭─[src/Steps.nash:8:12]
 7 │ isDone : step -> bool
 8 │ isDone s = s == Done
   ·            ────┬────
   ·                ╰── required by `==`
   ╰────
  help: Available impl heads:

            bool
            bytes
            int
            (list 'a0)

        …

        Import or define an impl for `Eq step`.
```

### Example 3 — non-exhaustive case

```elm
module Tag exposing (tag)
import Builtin exposing (Data(..))
import Literal exposing (FromInt)

tag : Data -> int
tag d =
    case d of
        Constr n _ -> n
        List _ -> 0
```

```text
  × Case expression is not exhaustive.
   ╭─[src/Tag.nash:7:5]
 6 │     tag d =
 7 │ ╭─▶     case d of
 8 │ │           Constr n _ -> n
 9 │ ╰─▶         List _ -> 0
   ╰────
  help: Missing patterns:

            Map _
            I _
            B _

        Add the missing branches; use `todo` for unfinished bodies.
```

## Warnings

`nash_can::Warning` (`UnusedVariable`, `UnusedImport`) and, later, the
trait solver's `MissingTypeAnnotation` (Elm `Reporting/Warning.hs`)
become `Report`s with `Severity::Warning`. They render identically with
miette's warning marker, never fail the build, and are suppressed by
`nash check --no-warnings`. The LSP publishes them as
`DiagnosticSeverity::WARNING`.

## JSON output

`nash check --report=json` uses the Elm compile-error envelope with structured
Nash problem fields. Each problem contains `code`, `title`, `severity`, `region`,
`message`, `labels`, `suggestions`, and `related`.

```json
{
  "code": "nash::type::mismatch",
  "title": "TYPE MISMATCH",
  "severity": "error",
  "region": { "start": { "line": 11, "column": 5 }, "end": { "line": 11, "column": 27 } },
  "message": ["Type mismatch: expected `list int`, found `list Int`."],
  "labels": [
    { "region": { "start": { "line": 11, "column": 5 }, "end": { "line": 11, "column": 27 } }, "text": "body of `settle`", "primary": true },
    { "region": { "start": { "line": 9, "column": 10 }, "end": { "line": 9, "column": 34 } }, "text": "declared type", "primary": false }
  ],
  "suggestions": [],
  "related": []
}
```

`message` contains styled text chunks from `before` and `after`. Source labels
are separate structured data; it no longer embeds an ASCII source drawing.
Clients must render `labels` to show source context. `related` contains module
objects with `path`, `name`, and `problems`, recursively using the same schema.
This is a deliberate schema extension and a change to the content of `message`.
Driver errors retain `{"type":"error","path":..,"title":..,"message":[..]}`.

Warnings use `"type": "compile-warnings"` with the same problem shape
(Elm has no JSON warnings; this is an addition). `--report=json` writes one
compile-error document to stdout, including an empty error array on success.
Warnings are a separate JSON document on stderr; `--no-warnings` suppresses it.
Source I/O failures use driver-error documents on stderr alongside any warning
document. There is no progress prose in JSON mode. Exit status is 1 for errors
or blocked modules and 0 for successful checks with or without warnings.
Human output uses `--color=auto|always|never`; auto respects `NO_COLOR` and the
terminal capability. JSON never contains ANSI sequences.

## LSP consumption

`nash-language-server` converts each `Report` to
`tower_lsp_server::ls_types::Diagnostic`:

| LSP field | from |
|---|---|
| `range` | `region`, converted from 1-based line/column to 0-based UTF-16 positions |
| `severity` | `severity` |
| `code` | stable `code` |
| `source` | `"nash"` |
| `message` | `before`, primary label text, and `after`, rendered plain at width 80 |
| `related_information` | all secondary labels and related reports, with their own file URIs and UTF-16 ranges |
| `data` | `suggestions` (for a future quick-fix code action) |

The server uses full-text synchronization and snapshots unsaved buffers over
the filesystem. It rejects stale document versions, rebuilds on close, and
selects the closest project that owns the edited file.

The server runs the same driver pipeline on `didOpen`/`didChange` and
publishes one `PublishDiagnostics` per module, including modules that
became clean (empty list).

## What each crate's errors become

`nash-report` defines one enum tying a module's errors together, like
Elm's `Reporting.Error.Error`:

```rust
pub enum ModuleError<'a> {
    Syntax(nash_parse::error::Error<'a>),
    /// Name resolution, kind (plan 02), and trait/impl declaration
    /// (plan 03) errors.
    Names(Vec<nash_can::Error<'a>>),
    /// Type and trait solver (plan 03) errors.
    Types(Localizer, Vec<nash_constrain::Error<'a>>),
    Patterns(Vec<nash_nitpick::Error<'a>>),
    Codegen(Vec<nash_codegen::Error<'a>>),                 // codegen.md
    Docs(nash_docs::Error<'a>),                            // nash docs
}

pub fn to_reports(source: &Source, error: &ModuleError<'_>) -> Vec<Report>;
```

| Source | Elm file ported | Titles (examples) | Notes |
|---|---|---|---|
| `nash_parse::error::Error` / `Module` / `Decl` / `Expr` / `Pattern` / `Type` ... | `Reporting/Error/Syntax.hs` | `UNFINISHED CASE`, `MISSING ARROW`, `UNEXPECTED SYMBOL`, `NO TABS`, `ENDLESS COMMENT`, `EXPECTING MODULE NAME` | Row/col points become width-1 labels. Port/effect/shader/char variants are dropped with their AST cases. `Expr::Char` keeps a short `NO CHARACTERS` report until the lexer stops producing it. |
| `nash_can::Error::MissingModuleHeader` | `Syntax.hs` `ModuleNameUnspecified` | `MODULE NAME MISSING` | No source labels, example shows `module Main exposing (..)`. |
| `nash_can::Error` (rest) | `Reporting/Error/Canonicalize.hs` | `NAMING ERROR`, `AMBIGUOUS NAME`, `NAME CLASH`, `SHADOWING`, `CYCLIC DEFINITION`, `BAD TYPE ANNOTATION`, `TOO FEW ARGS`, `ALIAS PROBLEM`, `UNBOUND TYPE VARIABLE`, `BAD IMPORT`, `UNKNOWN EXPORT`, `UNKNOWN OPERATOR` | `BinopFunctionNotFound` is new: `INFIX PROBLEM`, "The `(<+>)` operator refers to `combine`, but I cannot find that definition in this file." Elm's `%`, `===`, `!=` operator hints are kept (they are JS-isms users still type). |
| `nash_constrain::Error` | `Reporting/Error/Type.hs` + `Type/Error.hs` | `TYPE MISMATCH`, `TOO MANY ARGS`, `INFINITE TYPE` | Operator-specific prose (`badMath`, `badBool`, `badCompLeft`, ...) is kept for the core operators; `//` and `^` hints lose their `Float` halves; `(::)`, `(++)`, `(|>)`, `(<|)` unchanged. |
| `nash_can::Error::{KindMismatch, KindInfinite, BadArity}` (plan 02) | none (new) | `KIND MISMATCH`, `INFINITE KIND`, `TOO MANY TYPE ARGS` | Kinds are `Type` and arrows. `BadArity` reports too many arguments to a named constructor. |
| `nash_can::Error::{RepresentationMismatch, ContradictoryRepresentation, IrregularRecursion}` | none (new) | `REPRESENTATION MISMATCH`, `CONTRADICTORY REPRESENTATION`, `IRREGULAR RECURSION` | Formation context and source region explain the failed representation predicate or growing recursive context. Representation failures are separate from kind mismatch. |
| `nash_constrain::Error::{MissingImpl, MissingConstraint, AmbiguousType, PolymorphicRecursion}` (plan 03) | none (new) | `MISSING IMPL`, `MISSING CONSTRAINT`, `AMBIGUOUS TYPE`, `POLYMORPHIC RECURSION` | Example 2 above. `AMBIGUOUS TYPE` is for literals whose predicates cannot default. |
| `nash_can::Error::{OrphanImpl, OverlappingImpls, MissingMethod, UnknownMethod, MissingSuperclass, BadInstanceHead, NotFoundTrait, DuplicateTrait, ...}` (plan 03) | `Canonicalize.hs` helpers | `ORPHAN IMPL`, `NAME CLASH`, `MISSING METHOD`, `UNKNOWN METHOD`, `MISSING SUPERCLASS`, `BAD IMPL HEAD`, `NAMING ERROR` | Duplicates and overlaps reuse `nameClash`; lookups reuse `notFound` / `ambiguousName` with "trait". |
| `nash_nitpick::Error` | `Reporting/Error/Pattern.hs` | `MISSING PATTERNS`, `UNSAFE PATTERN`, `REDUNDANT PATTERN` | Example 3. `Debug.todo` → `todo`. |
| validator checks (`nash_can::Error` variants, plan 09) | `Reporting/Error/Main.hs` | `NO MAIN`, `BAD MAIN` | "I cannot find a `main` value in this validator module:". Rendered in `canonicalize.rs`. |
| `tests` block checks (`nash_can::Error` / `nash_constrain::Error` variants, plan 10) | none | `BAD TEST`, `BAD PROP` | e.g. a `prop` body that is not a `do` block. No separate `ModuleError` variant. |
| `nash_codegen::Error` | none | `NOT COMPILABLE` | e.g. a `comptime` result that is not a constant. |
| `nash_docs::Error` | `Reporting/Error/Docs.hs` | `NO DOCS`, `DOCS MISTAKE`, `DUPLICATE DOCS` | Only for `nash docs`. |
| `nash_can::Warning` | `Reporting/Warning.hs` | `unused variable`, `unused import`, `missing type annotation` | Lowercase titles as in Elm. |

## Interactions

- **Driver.** `ModuleResult::Failed { message: String }` becomes
  `Failed { error: ModuleError }` rendered before the arena drops:
  `nash_report::ModuleReports { name, path, source, reports }`. The
  driver stops after the first failing phase per module when its output is
  invalid, collects independent errors within that phase, and continues modules
  whose prerequisites remain valid. Render after safe collection completes.
- **Macros.** Expansion re-runs the phases on the expanded surface AST.
  Regions inside macro output point at the macro call site
  (`docs/macros.md`); reports need no special casing.
- **Formatter / docs.** `nash fmt` never reports; `nash docs` adds
  `ModuleError::Docs`.
- **Tests.** `nash test` reuses reports for compile failures; test
  *failures* (assertion output, shrunk counterexamples) are not reports.
  They are rendered by `nash-test` (`docs/testing.md`).

