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

### Report

Elm's `Reporting.Report.Report` is a title, a region, suggestions, and a
`Doc` that already contains the rendered code snippet. miette draws
snippets itself from labelled spans, so the Nash `Report` keeps the
snippet *placement* separate from the prose:

```rust
pub struct Report {
    pub title: String,          // "TYPE MISMATCH", "MISSING PATTERNS", ...
    pub severity: Severity,     // Error | Warning
    pub region: Region,         // Elm's `_region`: JSON + LSP range
    pub snippet: Snippet,       // what miette underlines
    pub before: Doc,            // Elm's preHint: the sentence ending in ":"
    pub after: Doc,             // Elm's postHint: details, hints, notes
    pub suggestions: Vec<String>, // Elm's `_sgstns` (editor quick-fix names)
}

pub enum Snippet {
    /// `Code.toSnippet source region highlight`.
    Region { region: Region, highlight: Option<Region> },
    /// `Code.toPair source r1 r2`: two labelled spans in one snippet.
    Pair { first: Label, second: Label },
    /// No code shown (`ModuleNameUnspecified`).
    None,
}

pub struct Label { pub region: Region, pub text: String }
```

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

The comparison is rendered into the report's `after` doc (miette `help`):
"It is ... / But you are trying to use it as ..." with the two indented
type blocks, then hints from the first problem.

### Suggest

`Reporting/Suggest.hs`: restricted Damerau-Levenshtein distance,
case-insensitive `sort` and `rank`. Used for "These names seem close
though:" lists (naming errors, record field typos, unknown exports,
unknown module imports). No external crate; the distance is ~25 lines.

## Rendering to the terminal

`Report::render(&self, source: &Source, path: &str, color: bool) -> Rendered`
produces a value implementing `miette::Diagnostic`:

| miette | from `Report` |
|---|---|
| `code()` | `title` |
| `severity()` | `severity` |
| `Display` (the `×` line) | `before` rendered at 80 columns |
| `labels()` | `snippet` regions, converted to byte offsets via `Source` (zero-width regions become width 1, like Elm's `max 1` caret) |
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

### Example 1 — type mismatch with a diff

```elm
module Ledger exposing (settle)

type alias Account = { owner : Bytes, balance : Int }

balanceOf : Account -> Int
balanceOf account = account.balance

settle : list Account -> list int
settle accounts =
    List.map balanceOf accounts
```

```
Error: TYPE MISMATCH

  × Something is off with the body of the `settle` definition:
    ╭─[src/Ledger.nash:10:5]
  9 │ settle accounts =
 10 │     List.map balanceOf accounts
    ·     ─────────────┬─────────────
    ·                  ╰── this `List.map` call
    ╰────
  help: This `List.map` call produces:

            list Int

        But the type annotation on `settle` says it should be:

            list int

        Hint: `Int` is the Big (Data) integer and `int` is the little
        builtin one. They never convert implicitly. Use `lower` to go from
        `Int` to `int`, or `lift` to go the other way.
```

`Int` and `int` in the two type blocks are `dullyellow` on a color
terminal; the rest of each type is plain. The hint is
`Problem::BigLittle`.

### Example 2 — missing impl

```elm
module Steps exposing (isDone)

type step = Done | Next int

isDone : step -> bool
isDone s = s == Done
```

```
Error: MISSING IMPL

  × I cannot find an `Eq` impl for `step`:
    ╭─[src/Steps.nash:6:12]
  6 │ isDone s = s == Done
    ·            ────┬────
    ·                ╰── needs `Eq step`
    ╰────
  help: The (==) operator needs its arguments to implement `Eq`, and here
        they are:

            step

        But there is no `impl Eq step` in this module or in any import,
        and `step` is not marked `@derive(Eq)`.

        Hint: Add `@derive(Eq)` above the `step` declaration, or write
        the impl by hand:

            impl Eq step where
                (==) a b = ...
```

### Example 3 — non-exhaustive case

```elm
module Tag exposing (tag)

tag : Data -> int
tag d =
    case d of
        Constr n _ -> n
        List _ -> 0
```

```
Error: MISSING PATTERNS

  × This `case` does not have branches for all possibilities:
    ╭─[src/Tag.nash:5:5]
  5 │ ╭─▶     case d of
  6 │ │           Constr n _ -> n
  7 │ ├─▶         List _ -> 0
    · ╰──── 
    ╰────
  help: Missing possibilities include:

            Map _
            I _
            B _

        I would have to crash if I saw one of those. Add branches for them!

        Hint: If you want to write the code for each branch later, use
        `todo` as a placeholder. Read
        <https://nash-script.dev/hints/missing-patterns> for more guidance
        on this workflow.
```

The missing-pattern block is `dullyellow`. Pattern text comes from
`nash_nitpick::render::pattern_to_string`.

## Warnings

`nash_can::Warning` (`UnusedVariable`, `UnusedImport`) and, later, the
trait solver's `MissingTypeAnnotation` (Elm `Reporting/Warning.hs`)
become `Report`s with `Severity::Warning`. They render identically with
miette's `Warning:` header, never fail the build, and are suppressed by
`nash check --no-warnings`. The LSP publishes them as
`DiagnosticSeverity::WARNING`.

## JSON output

`nash check --report=json` emits Elm's `--report=json` shape so existing
editor tooling for Elm can be adapted with a rename:

```json
{
  "type": "compile-errors",
  "errors": [
    {
      "path": "src/Ledger.nash",
      "name": "Ledger",
      "problems": [
        {
          "title": "TYPE MISMATCH",
          "region": { "start": { "line": 10, "column": 5 }, "end": { "line": 10, "column": 32 } },
          "message": [
            "Something is off with the body of the `settle` definition:\n\n",
            { "bold": false, "underline": false, "color": "yellow", "string": "Int" },
            "..."
          ]
        }
      ]
    }
  ]
}
```

`message` is Elm's `Doc.encode`: an array of plain strings and styled
chunks. Because miette does not draw JSON, the JSON `message` is the full
Elm-style document: `before`, blank line, an Elm-style code snippet
rendered by `nash-report` itself (`Render/Code.hs` port, kept for this
purpose only), then `after`. Driver errors serialize as
`{"type":"error","path":..,"title":..,"message":[..]}`.

Warnings use `"type": "compile-warnings"` with the same problem shape
(Elm has no JSON warnings; this is an addition).

## LSP consumption

`nash-language-server` converts each `Report` to
`tower_lsp_server::ls_types::Diagnostic`:

| LSP field | from |
|---|---|
| `range` | `region`, converted from 1-based line/column to 0-based UTF-16 positions |
| `severity` | `severity` |
| `code` | `title` |
| `source` | `"nash"` |
| `message` | `before` + `"\n\n"` + `after`, rendered plain at width 80 |
| `related_information` | `Snippet::Pair` second label, and the `highlight` of `Snippet::Region` when it differs from `region` |
| `data` | `suggestions` (for a future quick-fix code action) |

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
| `nash_can::Error::MissingModuleHeader` | `Syntax.hs` `ModuleNameUnspecified` | `MODULE NAME MISSING` | `Snippet::None`, example shows `module Main exposing (..)`. |
| `nash_can::Error` (rest) | `Reporting/Error/Canonicalize.hs` | `NAMING ERROR`, `AMBIGUOUS NAME`, `NAME CLASH`, `SHADOWING`, `CYCLIC DEFINITION`, `BAD TYPE ANNOTATION`, `TOO FEW ARGS`, `ALIAS PROBLEM`, `UNBOUND TYPE VARIABLE`, `BAD IMPORT`, `UNKNOWN EXPORT`, `UNKNOWN OPERATOR` | `BinopFunctionNotFound` is new: `INFIX PROBLEM`, "The `(<+>)` operator refers to `combine`, but I cannot find that definition in this file." Elm's `%`, `===`, `!=` operator hints are kept (they are JS-isms users still type). |
| `nash_constrain::Error` | `Reporting/Error/Type.hs` + `Type/Error.hs` | `TYPE MISMATCH`, `TOO MANY ARGS`, `INFINITE TYPE` | Operator-specific prose (`badMath`, `badBool`, `badCompLeft`, ...) is kept for the core operators; `//` and `^` hints lose their `Float` halves; `(::)`, `(++)`, `(|>)`, `(<|)` unchanged. |
| `nash_can::Error::{KindMismatch, KindInfinite, KindTooManyArgs}` (plan 02) | none (new) | `KIND MISMATCH`, `INFINITE KIND`, `TOO MANY TYPE ARGS` | `KindContext` picks the sentence: "`list` can only hold `Big` or `Const` values, but `option int` is a `Term`:". |
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
  driver stops after the first failing phase per module, as now.
- **Macros.** Expansion re-runs the phases on the expanded surface AST.
  Regions inside macro output point at the macro call site
  (`docs/macros.md`); reports need no special casing.
- **Formatter / docs.** `nash fmt` never reports; `nash docs` adds
  `ModuleError::Docs`.
- **Tests.** `nash test` reuses reports for compile failures; test
  *failures* (assertion output, shrunk counterexamples) are not reports.
  They are rendered by `nash-test` (`docs/testing.md`).

## Open questions

1. **Hint URLs.** `https://nash-script.dev/hints/<name>` is assumed. The
   pages do not exist yet.
2. **miette fallback.** If the pinned miette lacks
   `MietteHandlerOpts::wrap_lines`, `after` goes through a custom
   `miette::ReportHandler` that delegates header and snippet to the
   graphical handler and appends the help text verbatim.
3. **`Option.withDefault` name.** `AnythingFromOption`'s hint names a
   stdlib function; fill in once `docs/stdlib.md` fixes it.
