# Plan 06 — `nash-report`: Elm prose, miette rendering

## Goal

Build the `nash-report` crate described in `docs/diagnostics.md`: every
phase's error data becomes a `Report` (Elm's prose, kept nearly verbatim),
and `Report` renders through miette (terminal), to Elm-shaped JSON, and to
LSP diagnostics. Replace the driver's `format!("{:?}", errors)`
placeholders.

Collect as many safely recoverable independent errors as possible before
rendering. Reporting a vector of errors is insufficient if inference or the
driver suppressed independent errors before constructing it. Follow the
collection and cascade-suppression contract in `docs/diagnostics.md`.

## Prerequisites

- Plan 05 (`nash-nitpick`) for the pattern chunk.
- Chunks 1–12 need nothing from plans 01–04. Chunk 13 (kinds, traits) needs
  plans 02 and 03.

## Crates touched

- new: `crates/nash-report`
- `crates/nash-parse` (make `keyword::is_reserved` and
  `symbol::is_binop_char` public)
- `crates/nash-driver`, `crates/nash-cli`, `crates/nash-language-server`
- `crates/nash-can`, `crates/nash-solve`, `crates/nash-constrain` where
  collection and recovery changes are needed; include their Sampo changesets.

## Reference

| Elm | Port |
|---|---|
| `Reporting/Report.hs` | `src/lib.rs` `Report` |
| `Reporting/Doc.hs` | `src/doc.rs` |
| `Reporting/Render/Code.hs` | `src/code.rs` |
| `Reporting/Suggest.hs` | `src/suggest.rs` |
| `Reporting/Render/Type/Localizer.hs` | `src/localizer.rs` |
| `Reporting/Render/Type.hs` | `src/render_type.rs` |
| `Type/Error.hs` (rendering half) | `src/type_diff.rs` |
| `Reporting/Error/Syntax.hs` | `src/syntax/{mod,module,decl,expr,pattern,type_}.rs` |
| `Reporting/Error/Canonicalize.hs` | `src/canonicalize.rs` |
| `Reporting/Error/Type.hs` | `src/type_.rs` |
| `Reporting/Error/Pattern.hs` | `src/pattern.rs` |
| `Reporting/Warning.hs` | `src/warning.rs` |
| `Reporting/Error.hs` (`toJson`, `toReports`) | `src/lib.rs` `ModuleError`, `src/json.rs` |
| `Reporting/Error/Import.hs` | folded into driver errors (module resolution lives in `nash-driver`) |

Current Nash error data: `crates/nash-parse/src/error.rs`,
`crates/nash-can/src/error.rs`, `crates/nash-can/src/warning.rs`,
`crates/nash-constrain/src/error.rs`, `crates/nash-constrain/src/error_type.rs`,
`crates/nash-nitpick/src/pattern.rs` (plan 05).

## Conventions used by every chunk

- One Rust function per Haskell function, same name in `snake_case`;
  `toReport` families become `to_report`, `to_*_report`.
- Message strings are copied from the Haskell source and edited only for
  Nash differences. Each edit is called out in the chunk.
- Tests render with `render_plain` (chunk 1) and snapshot the text with
  `insta::assert_snapshot!`. ANSI is never in snapshots.
- Every chunk ends with `cargo fmt`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo insta test`.

---

## Chunk 1 — crate skeleton, `Report`, `Source`, miette bridge

**Files**

- `crates/nash-report/Cargo.toml` (new)
- `crates/nash-report/src/lib.rs` (new)
- `crates/nash-report/src/code.rs` (new)
- `crates/nash-report/src/render.rs` (new)
- `crates/nash-parse/src/lib.rs` (`pub mod keyword; pub mod symbol;`)
- `.sampo/changesets/report-skeleton.md`

**Change**

Create the crate with the owned `Report` type, the `Source` line table
(Elm `Render/Code.hs`: `toSource`, `whatIsNext`,
`nextLineStartsWithKeyword`, `nextLineStartsWithCloseCurly`), and the
miette `Diagnostic` bridge. `Doc` is a placeholder newtype over `String`
until chunk 2 replaces it; chunk 1 tests only use `Doc::text`.

**Code**

`Cargo.toml`:

```toml
[package]
name = "nash-report"
version = "0.1.0"
edition.workspace = true
description = "Error reports for the Nash compiler: Elm's prose rendered with miette"
homepage.workspace = true
repository.workspace = true
license.workspace = true

[dependencies]
miette.workspace = true
serde.workspace = true
serde_json = "1"
nash-ast = { path = "../nash-ast", version = "0.3.1" }
nash-can = { path = "../nash-can", version = "0.3.1" }
nash-constrain = { path = "../nash-constrain", version = "0.2.1" }
nash-nitpick = { path = "../nash-nitpick", version = "0.1.0" }
nash-parse = { path = "../nash-parse", version = "0.2.2" }
nash-region = { path = "../nash-region", version = "0.2.0" }
nash-source = { path = "../nash-source", version = "0.3.0" }

[dev-dependencies]
bumpalo.workspace = true
indoc.workspace = true
insta.workspace = true
nash-solve = { path = "../nash-solve", version = "0.2.1" }
```

(`serde_json` goes into `[workspace.dependencies]`.)

`src/lib.rs`:

```rust
//! Error reports: Elm's `Reporting/*` prose as miette diagnostics.
//!
//! Each phase's error data (`nash_parse::error`, `nash_can::Error`, ...)
//! is turned into an owned `Report` that outlives the module arena. A
//! `Report` renders three ways: `render` (miette, terminal), `json`
//! (Elm's `--report=json` shape), and the LSP conversion in
//! `nash-language-server`.

pub mod code;
pub mod doc;
mod render;

use nash_region::Region;

pub use code::Source;
pub use doc::Doc;
pub use render::{Rendered, render_plain};

/// Elm's `Reporting.Report.Report` with the snippet placement split out
/// so miette can draw the code.
#[derive(Debug)]
pub struct Report {
    pub title: String,
    pub severity: Severity,
    pub region: Region,
    pub snippet: Snippet,
    pub before: Doc,
    pub after: Doc,
    pub suggestions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug)]
pub enum Snippet {
    /// `Code.toSnippet source region highlight`.
    Region { region: Region, highlight: Option<Region> },
    /// `Code.toPair source r1 r2`.
    Pair { first: Label, second: Label },
    /// No code shown.
    None,
}

#[derive(Debug)]
pub struct Label {
    pub region: Region,
    pub text: String,
}

impl Report {
    /// `Report.Report title region [] (Code.toSnippet source region highlight (before, after))`.
    pub fn snippet(
        title: &str,
        region: Region,
        highlight: Option<Region>,
        before: Doc,
        after: Doc,
    ) -> Report {
        Report {
            title: title.to_string(),
            severity: Severity::Error,
            region,
            snippet: Snippet::Region { region, highlight },
            before,
            after,
            suggestions: Vec::new(),
        }
    }

    /// `Report.Report title r2 [] (Code.toPair source r1 r2 ...)`. The
    /// "one line" / "two chunks" wording choice Elm makes is replaced by
    /// two labels; `before` is Elm's `twoStart` and `after` its `twoEnd`.
    pub fn pair(title: &str, first: Label, second: Label, before: Doc, after: Doc) -> Report {
        let region = second.region;
        Report {
            title: title.to_string(),
            severity: Severity::Error,
            region,
            snippet: Snippet::Pair { first, second },
            before,
            after,
            suggestions: Vec::new(),
        }
    }

    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Report {
        self.suggestions = suggestions;
        self
    }

    pub fn warning(mut self) -> Report {
        self.severity = Severity::Warning;
        self
    }
}
```

`src/code.rs`:

```rust
//! Source text access for reports, from Elm's `Reporting/Render/Code.hs`.
//! Columns are byte-based, matching `nash_parse::Parser::advance`.

use miette::SourceSpan;
use nash_parse::{Col, Row};
use nash_region::{Position, Region};

pub struct Source<'s> {
    text: &'s str,
    /// Byte offset of every line start; the last entry is `text.len()`
    /// when the text ends in a newline (Elm's `lines ++ [""]`).
    line_starts: Vec<usize>,
}

impl<'s> Source<'s> {
    pub fn new(text: &'s str) -> Source<'s> {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Source { text, line_starts }
    }

    pub fn text(&self) -> &'s str {
        self.text
    }

    /// Byte offset of a 1-based position produced by the parser.
    pub fn offset(&self, position: Position) -> usize {
        let start = self.line_starts[usize::from(position.line) - 1];
        (start + usize::from(position.column) - 1).min(self.text.len())
    }

    /// miette span for a region; a zero-width region gets width 1, like
    /// Elm's `max 1 (c2 - c1)` caret.
    pub fn span(&self, region: Region) -> SourceSpan {
        let start = self.offset(region.start);
        let end = self.offset(region.end).max(start + 1).min(self.text.len().max(start + 1));
        (start, end - start).into()
    }

    /// Text of a 1-based row, without its newline.
    pub fn line(&self, row: Row) -> Option<&'s str> {
        let start = *self.line_starts.get(usize::from(row) - 1)?;
        let end = self.line_starts.get(usize::from(row)).map_or(self.text.len(), |next| next - 1);
        Some(&self.text[start..end.max(start)])
    }

    /// Elm's `whatIsNext`.
    pub fn what_is_next(&self, row: Row, col: Col) -> Next<'s> {
        let Some(rest) = self.line(row).and_then(|line| line.get(usize::from(col) - 1..)) else {
            return Next::Other(None);
        };
        let mut chars = rest.chars();
        let Some(c) = chars.next() else {
            return Next::Other(None);
        };
        let inner_len = |s: &str| s.chars().take_while(|c| c.is_alphanumeric() || *c == '_').map(char::len_utf8).sum::<usize>();
        if c.is_uppercase() {
            Next::Upper(&rest[..c.len_utf8() + inner_len(chars.as_str())])
        } else if c.is_lowercase() {
            let name = &rest[..c.len_utf8() + inner_len(chars.as_str())];
            if nash_parse::keyword::is_reserved(name) { Next::Keyword(name) } else { Next::Lower(name) }
        } else if c.is_ascii() && nash_parse::symbol::is_binop_char(c as u8) {
            let len = rest.bytes().take_while(|b| nash_parse::symbol::is_binop_char(*b)).count();
            Next::Operator(&rest[..len])
        } else {
            match c {
                ')' => Next::Close("parenthesis", ')'),
                ']' => Next::Close("square bracket", ']'),
                '}' => Next::Close("curly brace", '}'),
                other => Next::Other(Some(other)),
            }
        }
    }

    /// Elm's `nextLineStartsWithKeyword`.
    pub fn next_line_starts_with_keyword(&self, keyword: &str, row: Row) -> Option<(Row, Col)> {
        let line = self.line(row + 1)?;
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        let rest = &line[indent..];
        let follows = rest.strip_prefix(keyword)?;
        let boundary = follows.chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        boundary.then_some((row + 1, 1 + indent as Col))
    }

    /// Elm's `nextLineStartsWithCloseCurly`.
    pub fn next_line_starts_with_close_curly(&self, row: Row) -> Option<(Row, Col)> {
        let line = self.line(row + 1)?;
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        line[indent..].starts_with('}').then_some((row + 1, 1 + indent as Col))
    }
}

/// Elm's `Next`.
#[derive(Debug, PartialEq, Eq)]
pub enum Next<'s> {
    Keyword(&'s str),
    Operator(&'s str),
    Close(&'static str, char),
    Upper(&'s str),
    Lower(&'s str),
    Other(Option<char>),
}

/// Elm's `toRegion row col`.
pub fn to_region(row: Row, col: Col) -> Region {
    let pos = Position::new(row, col);
    Region::new(pos, pos)
}

/// Elm's `toWiderRegion`.
pub fn to_wider_region(row: Row, col: Col, extra: u16) -> Region {
    Region::new(Position::new(row, col), Position::new(row, col + extra))
}

/// Elm's `toKeywordRegion`.
pub fn to_keyword_region(row: Row, col: Col, keyword: &str) -> Region {
    to_wider_region(row, col, keyword.len() as u16)
}
```

`src/render.rs`:

```rust
//! `Report` -> `miette::Diagnostic`.

use std::fmt;

use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, MietteHandler, MietteHandlerOpts, NamedSource, SourceCode};

use crate::code::Source;
use crate::{Report, Severity, Snippet};

pub const WIDTH: usize = 80;

/// A `Report` bound to its file, owning everything miette needs.
#[derive(Debug)]
pub struct Rendered {
    title: String,
    severity: Severity,
    message: String,
    help: Option<String>,
    labels: Vec<LabeledSpan>,
    source: NamedSource<String>,
}

impl Report {
    pub fn render(&self, source: &Source<'_>, path: &str, color: bool) -> Rendered {
        let labels = match &self.snippet {
            Snippet::Region { region, highlight } => vec![LabeledSpan::new_with_span(
                None,
                source.span(highlight.unwrap_or(*region)),
            )],
            Snippet::Pair { first, second } => vec![
                LabeledSpan::new_with_span(Some(first.text.clone()), source.span(first.region)),
                LabeledSpan::new_with_span(Some(second.text.clone()), source.span(second.region)),
            ],
            Snippet::None => Vec::new(),
        };
        let after = self.after.render(WIDTH, color);
        Rendered {
            title: self.title.clone(),
            severity: self.severity,
            message: self.before.render(WIDTH, color),
            help: (!after.is_empty()).then_some(after),
            labels,
            source: NamedSource::new(path, source.text().to_string()),
        }
    }
}

impl fmt::Display for Rendered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Rendered {}

impl Diagnostic for Rendered {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(&self.title))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
        })
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        self.help.as_ref().map(|help| Box::new(help) as Box<dyn fmt::Display>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.source)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(self.labels.iter().cloned()))
    }
}

/// The handler the CLI installs through `miette::set_hook`: fixed width,
/// no re-wrapping (Doc already wrapped), color decided by the CLI.
pub fn handler(color: bool) -> MietteHandler {
    MietteHandlerOpts::new()
        .width(WIDTH)
        .wrap_lines(false)
        .color(color)
        .unicode(true)
        .build()
}

/// Plain-text rendering for snapshot tests: the same options
/// `MietteHandlerOpts::build` applies, on a handler that can write to a
/// `String`.
pub fn render_plain(report: &Report, source: &Source<'_>, path: &str) -> String {
    let rendered = report.render(source, path, false);
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
        .with_width(WIDTH)
        .with_wrap_lines(false)
        .render_report(&mut out, &rendered)
        .expect("render");
    out
}
```

If the pinned miette lacks `wrap_lines`, the fallback is a
`miette::ReportHandler` of our own that delegates header and snippet to
`GraphicalReportHandler` and appends `help` verbatim.

Changeset: `cargo/nash-report: minor`, `cargo/nash-parse: patch`
("expose keyword and symbol helpers").

**Elm reference** — `Reporting/Report.hs`; `Reporting/Render/Code.hs`
`toSource`, `whatIsNext`, `detectKeywords`, `isInner`, `isSymbol`,
`startsWithKeyword`, `nextLineStartsWithKeyword`,
`nextLineStartsWithCloseCurly`; `Reporting/Error/Syntax.hs` `toRegion`,
`toWiderRegion`, `toKeywordRegion`.

**Tests** (`src/render.rs` and `src/code.rs` `mod tests`)

- `render_snippet_report` — hand-built `Report::snippet("TEST", region
  of `x` in `f = x + 1`, None, Doc::text("Before:"), Doc::text("After."))`
  → `assert_snapshot!(render_plain(..))`
- `render_pair_report` — two labels on one line
- `render_no_snippet_report` — `Snippet::None` shows no code block
- `render_warning_header` — `.warning()` renders a `Warning:` header
- `render_zero_width_region_gets_caret`
- `what_is_next_keyword` / `_operator` / `_close_paren` / `_upper` /
  `_lower` / `_end_of_line` — `assert_eq!`
- `next_line_starts_with_keyword_in` — `Some((row+1, col))`
- `offset_of_last_line_without_newline`

**Done when**

Snapshots accepted; the placeholder `Doc` is `pub struct Doc(String)` with
`Doc::text` and `Doc::render(width, color) -> String` returning the text
unchanged.

---

## Chunk 2 — `Doc`

**Files**

- `crates/nash-report/src/doc.rs` (replace placeholder)
- `crates/nash-report/src/doc/print.rs` (new; layout algorithm)

**Change**

Port `Reporting/Doc.hs`: the `Doc` tree, the pretty printer (Wadler's
`best`/`fits` in the form `prettyprinter`/`ansi-wl-pprint` use), ANSI
output, and the JSON chunk encoder. Elm relies on `P.renderPretty 1 80`
(ribbon fraction 1, width 80).

**Code**

```rust
//! Elm's `Reporting/Doc.hs`: a small Wadler-style pretty printer with the
//! handful of combinators the reports use.

mod print;

pub use print::Chunk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Doc {
    Empty,
    Text(String),
    Styled(Style, Box<Doc>),
    Cat(Vec<Doc>),
    Nest(usize, Box<Doc>),
    Align(Box<Doc>),
    Line,
    LineBreak,
    HardLine,
    Group(Box<Doc>),
    Fill(Vec<Doc>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub underline: bool,
    pub color: Option<Color>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub base: BaseColor,
    pub vivid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseColor { Black, Red, Green, Yellow, Blue, Magenta, Cyan, White }

impl From<&str> for Doc {
    fn from(s: &str) -> Doc { Doc::text(s) }
}

impl Doc {
    /// `P.text` / `fromChars` / `fromName`. Panics on '\n': use `vcat`.
    pub fn text(s: impl Into<String>) -> Doc {
        let s = s.into();
        assert!(!s.contains('\n'), "Doc::text must not contain newlines");
        Doc::Text(s)
    }

    pub fn from_int(n: impl std::fmt::Display) -> Doc { Doc::text(n.to_string()) }

    /// `a <> b`.
    pub fn cat(docs: impl IntoIterator<Item = Doc>) -> Doc { Doc::Cat(docs.into_iter().collect()) }
    /// `P.hsep`.
    pub fn hsep(docs: impl IntoIterator<Item = Doc>) -> Doc { intersperse(docs, Doc::text(" ")) }
    /// `P.hcat`.
    pub fn hcat(docs: impl IntoIterator<Item = Doc>) -> Doc { Doc::cat(docs) }
    /// `P.vcat`.
    pub fn vcat(docs: impl IntoIterator<Item = Doc>) -> Doc { intersperse(docs, Doc::HardLine) }
    /// `P.sep` = `group (vsep docs)`.
    pub fn sep(docs: impl IntoIterator<Item = Doc>) -> Doc { Doc::Group(Box::new(intersperse(docs, Doc::Line))) }
    /// `P.fillSep`.
    pub fn fill_sep(docs: impl IntoIterator<Item = Doc>) -> Doc { Doc::Fill(docs.into_iter().collect()) }
    /// `P.indent`.
    pub fn indent(n: usize, doc: Doc) -> Doc { Doc::Nest(n, Box::new(Doc::cat([Doc::text(" ".repeat(n)), doc]))) }
    /// `P.hang`.
    pub fn hang(n: usize, doc: Doc) -> Doc { Doc::Align(Box::new(Doc::Nest(n, Box::new(doc)))) }
    /// `P.align`.
    pub fn align(doc: Doc) -> Doc { Doc::Align(Box::new(doc)) }

    /// Elm `stack`: paragraphs separated by blank lines.
    pub fn stack(docs: impl IntoIterator<Item = Doc>) -> Doc {
        intersperse(docs, Doc::cat([Doc::HardLine, Doc::HardLine]))
    }

    /// Elm `reflow`: words wrapped to the width.
    pub fn reflow(paragraph: &str) -> Doc {
        Doc::fill_sep(paragraph.split_whitespace().map(Doc::text))
    }

    /// Elm `commaSep`.
    pub fn comma_sep(conjunction: &str, style: impl Fn(Doc) -> Doc, names: Vec<Doc>) -> Vec<Doc> {
        match names.len() {
            0 => Vec::new(),
            1 => names.into_iter().map(style).collect(),
            2 => {
                let mut it = names.into_iter();
                vec![style(it.next().unwrap()), Doc::text(conjunction), style(it.next().unwrap())]
            }
            n => {
                let mut docs: Vec<Doc> = names
                    .iter()
                    .take(n - 1)
                    .map(|name| Doc::cat([style(name.clone()), Doc::text(",")]))
                    .collect();
                docs.push(Doc::text(conjunction));
                docs.push(style(names[n - 1].clone()));
                docs
            }
        }
    }

    // STYLES (Elm's P.red .. P.dullyellow)

    fn styled(self, f: impl FnOnce(&mut Style)) -> Doc {
        let mut style = Style::default();
        f(&mut style);
        Doc::Styled(style, Box::new(self))
    }
    fn color(self, base: BaseColor, vivid: bool) -> Doc { self.styled(|s| s.color = Some(Color { base, vivid })) }
    pub fn red(self) -> Doc { self.color(BaseColor::Red, true) }
    pub fn dullred(self) -> Doc { self.color(BaseColor::Red, false) }
    pub fn green(self) -> Doc { self.color(BaseColor::Green, true) }
    pub fn yellow(self) -> Doc { self.color(BaseColor::Yellow, true) }
    pub fn dullyellow(self) -> Doc { self.color(BaseColor::Yellow, false) }
    pub fn cyan(self) -> Doc { self.color(BaseColor::Cyan, true) }
    pub fn dullcyan(self) -> Doc { self.color(BaseColor::Cyan, false) }
    pub fn blue(self) -> Doc { self.color(BaseColor::Blue, true) }
    pub fn magenta(self) -> Doc { self.color(BaseColor::Magenta, true) }
    pub fn black(self) -> Doc { self.color(BaseColor::Black, true) }
    pub fn underline(self) -> Doc { self.styled(|s| s.underline = true) }

    // NOTES, HINTS, LINKS

    /// `toFancyNote`: `fillSep (underline "Note" <> ":" : chunks)`.
    pub fn to_fancy_note(chunks: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::fill_sep(std::iter::once(Doc::cat([Doc::text("Note").underline(), Doc::text(":")])).chain(chunks))
    }
    pub fn to_simple_note(message: &str) -> Doc {
        Doc::to_fancy_note(message.split_whitespace().map(Doc::text))
    }
    pub fn to_fancy_hint(chunks: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::fill_sep(std::iter::once(Doc::cat([Doc::text("Hint").underline(), Doc::text(":")])).chain(chunks))
    }
    pub fn to_simple_hint(message: &str) -> Doc {
        Doc::to_fancy_hint(message.split_whitespace().map(Doc::text))
    }

    /// `link word before fileName after`.
    pub fn link(word: &str, before: &str, file_name: &str, after: &str) -> Doc {
        Doc::fill_sep(
            std::iter::once(Doc::cat([Doc::text(word).underline(), Doc::text(":")]))
                .chain(before.split_whitespace().map(Doc::text))
                .chain(std::iter::once(Doc::text(make_link(file_name))))
                .chain(after.split_whitespace().map(Doc::text)),
        )
    }
    pub fn fancy_link(word: &str, before: Vec<Doc>, file_name: &str, after: Vec<Doc>) -> Doc { /* same shape */ }
    pub fn reflow_link(before: &str, file_name: &str, after: &str) -> Doc { /* no word */ }

    /// Elm `cycle`: the boxed dependency cycle drawing.
    pub fn cycle(indent: usize, name: &str, names: &[&str]) -> Doc {
        let to_ln = |n: &str| Doc::cat([Doc::text("│    "), Doc::text(n).dullyellow()]);
        let mut lines = vec![Doc::text("┌─────┐")];
        for (i, n) in std::iter::once(name).chain(names.iter().copied()).enumerate() {
            if i > 0 { lines.push(Doc::text("│     ↓")); }
            lines.push(to_ln(n));
        }
        lines.push(Doc::text("└─────┘"));
        Doc::indent(indent, Doc::vcat(lines))
    }

    // RENDERING

    /// Elm `toString` / `toAnsi` at the given width.
    pub fn render(&self, width: usize, color: bool) -> String { print::render(self, width, color) }
    /// Elm `Doc.encode`: styled chunks for JSON.
    pub fn chunks(&self, width: usize) -> Vec<Chunk> { print::chunks(self, width) }
}

pub fn make_link(file_name: &str) -> String { format!("<https://nash-script.dev/hints/{file_name}>") }
pub fn make_naked_link(file_name: &str) -> String { format!("https://nash-script.dev/hints/{file_name}") }

/// Elm `args`.
pub fn args(n: usize) -> String { format!("{n} argument{}", if n == 1 { "" } else { "s" }) }
/// Elm `moreArgs`.
pub fn more_args(n: usize) -> String { format!("{n} more argument{}", if n == 1 { "" } else { "s" }) }
/// Elm `ordinal` over a zero-based index.
pub fn ordinal(index: usize) -> String { int_to_ordinal(index + 1) }
/// Elm `intToOrdinal`.
pub fn int_to_ordinal(number: usize) -> String {
    let ending = match (number % 100, number % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{number}{ending}")
}

fn intersperse(docs: impl IntoIterator<Item = Doc>, sep: Doc) -> Doc {
    let mut out = Vec::new();
    for (i, doc) in docs.into_iter().enumerate() {
        if i > 0 { out.push(sep.clone()); }
        out.push(doc);
    }
    Doc::Cat(out)
}
```

`src/doc/print.rs` — the layout algorithm. Output is a list of lines,
each a list of `(Style, String)` runs; ANSI and JSON are two encoders
over that.

```rust
//! Wadler's `best` for `Doc`, plus `fillSep` and `align`.

use super::{BaseColor, Doc, Style};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    Plain(String),
    Styled { style: Style, text: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Flat, Break }

struct Printer<'d> {
    width: usize,
    column: usize,
    runs: Vec<(Style, String)>,
    /// Work stack: (indent, mode, style, doc).
    stack: Vec<(usize, Mode, Style, &'d Doc)>,
}

pub fn layout(doc: &Doc, width: usize) -> Vec<(Style, String)> {
    let mut p = Printer { width, column: 0, runs: Vec::new(), stack: vec![(0, Mode::Break, Style::default(), doc)] };
    while let Some((indent, mode, style, doc)) = p.stack.pop() {
        match doc {
            Doc::Empty => {}
            Doc::Text(s) => p.emit(style, s),
            Doc::Styled(st, inner) => p.stack.push((indent, mode, merge(style, *st), inner)),
            Doc::Cat(docs) => p.stack.extend(docs.iter().rev().map(|d| (indent, mode, style, d))),
            Doc::Nest(n, inner) => p.stack.push((indent + n, mode, style, inner)),
            Doc::Align(inner) => p.stack.push((p.column, mode, style, inner)),
            Doc::Line => match mode { Mode::Flat => p.emit(style, " "), Mode::Break => p.newline(indent) },
            Doc::LineBreak => if mode == Mode::Break { p.newline(indent) },
            Doc::HardLine => p.newline(indent),
            Doc::Group(inner) => {
                let mode = if p.fits(indent, inner) { Mode::Flat } else { Mode::Break };
                p.stack.push((indent, mode, style, inner));
            }
            Doc::Fill(docs) => p.fill(indent, style, docs),
        }
    }
    p.finish()
}

impl<'d> Printer<'d> {
    /// Does `doc` laid out flat fit in the remaining width? Looks at the
    /// rest of the current line only (Wadler's `fits`).
    fn fits(&self, indent: usize, doc: &Doc) -> bool {
        let mut remaining = self.width as isize - self.column as isize;
        let mut work = vec![doc];
        while let Some(d) = work.pop() {
            if remaining < 0 { return false; }
            match d {
                Doc::Empty | Doc::LineBreak => {}
                Doc::Text(s) => remaining -= s.chars().count() as isize,
                Doc::Line => remaining -= 1,
                Doc::HardLine => return true,
                Doc::Styled(_, inner) | Doc::Nest(_, inner) | Doc::Align(inner) | Doc::Group(inner) => work.push(inner),
                Doc::Cat(docs) | Doc::Fill(docs) => work.extend(docs.iter().rev()),
            }
        }
        let _ = indent;
        remaining >= 0
    }

    /// `fillSep`: put each word on the current line if it fits, else
    /// break first. Words are laid out flat.
    fn fill(&mut self, indent: usize, style: Style, docs: &'d [Doc]) {
        for (i, doc) in docs.iter().enumerate() {
            let sep = if i == 0 { 0 } else { 1 };
            if i > 0 {
                let flat_width = flat_width(doc);
                if self.column + sep + flat_width > self.width { self.newline(indent); } else { self.emit(style, " "); }
            }
            self.run_flat(indent, style, doc);
        }
    }

    fn run_flat(&mut self, indent: usize, style: Style, doc: &'d Doc) {
        // Lay out `doc` with a nested printer in Flat mode, sharing runs.
        let mark = self.stack.len();
        self.stack.push((indent, Mode::Flat, style, doc));
        while self.stack.len() > mark {
            let (indent, mode, style, doc) = self.stack.pop().unwrap();
            /* same match as `layout`, with `Group` forced Flat */
        }
    }

    fn emit(&mut self, style: Style, s: &str) { self.column += s.chars().count(); push_run(&mut self.runs, style, s); }
    fn newline(&mut self, indent: usize) { push_run(&mut self.runs, Style::default(), "\n"); push_run(&mut self.runs, Style::default(), &" ".repeat(indent)); self.column = indent; }
    fn finish(self) -> Vec<(Style, String)> { trim_trailing_spaces(self.runs) }
}

fn flat_width(doc: &Doc) -> usize { /* sum of Text widths, Line = 1 */ }

fn merge(outer: Style, inner: Style) -> Style {
    Style { bold: outer.bold || inner.bold, underline: outer.underline || inner.underline, color: inner.color.or(outer.color) }
}

pub fn render(doc: &Doc, width: usize, color: bool) -> String {
    let mut out = String::new();
    for (style, text) in layout(doc, width) {
        if color && style != Style::default() {
            out.push_str(&ansi_open(style));
            out.push_str(&text);
            out.push_str("\x1b[0m");
        } else {
            out.push_str(&text);
        }
    }
    out
}

fn ansi_open(style: Style) -> String {
    let mut codes = Vec::new();
    if style.bold { codes.push(1) }
    if style.underline { codes.push(4) }
    if let Some(c) = style.color {
        let base = match c.base { BaseColor::Black => 30, BaseColor::Red => 31, BaseColor::Green => 32, BaseColor::Yellow => 33, BaseColor::Blue => 34, BaseColor::Magenta => 35, BaseColor::Cyan => 36, BaseColor::White => 37 };
        codes.push(if c.vivid { base + 60 } else { base });
    }
    format!("\x1b[{}m", codes.iter().map(ToString::to_string).collect::<Vec<_>>().join(";"))
}

/// Elm `Doc.encode`: adjacent same-style runs merged, plain runs as bare
/// strings.
pub fn chunks(doc: &Doc, width: usize) -> Vec<Chunk> {
    layout(doc, width)
        .into_iter()
        .map(|(style, text)| if style == Style::default() { Chunk::Plain(text) } else { Chunk::Styled { style, text } })
        .collect()
}
```

Trailing spaces on a line are trimmed at `finish` (Elm's renderer never
emits them; snapshots depend on this).

**Elm reference** — all of `Reporting/Doc.hs`; the printer semantics are
`Text.PrettyPrint.ANSI.Leijen` (`renderPretty 1 80`).

**Tests** (`doc.rs` `mod tests`, `assert_snapshot!` of `render(80, false)`)

- `reflow_wraps_at_80` — a 200-character sentence
- `reflow_inside_indent_keeps_indent` — `indent(4, reflow(..))` wraps with
  4-space continuation
- `stack_separates_with_blank_line`
- `sep_flat_when_fits` — `sep(["a", "->", "b"])` → `a -> b`
- `sep_breaks_when_too_wide` — 30 long words → one per line
- `hang_aligns_continuations` — `hang(4, sep(..))` at column 10
- `cycle_box` — `cycle(4, "a", &["b", "c"])`
- `fancy_note_underlines_word` — `render(80, true)` contains `\x1b[4mNote`
  (the one ANSI test; asserted with `assert!`, not a snapshot)
- `chunks_merge_plain_runs` — `assert_debug_snapshot!(chunks(..))`
- `int_to_ordinal_table` — `1st 2nd 3rd 4th 11th 12th 13th 21st 22nd 23rd 101st 111th`
- `comma_sep_three` — `["a,", "b,", "and", "c"]`

**Done when**

Chunk 1's tests still pass with the real `Doc`; the width-80 wrap matches
Elm on the `reflow_wraps_at_80` input (compare by hand against `elm make`
output once).

---

## Chunk 3 — `Suggest`

**Files**

- `crates/nash-report/src/suggest.rs` (new)

**Code**

```rust
//! Elm's `Reporting/Suggest.hs`: near-miss name suggestions.

/// Restricted Damerau-Levenshtein (optimal string alignment) distance.
pub fn distance(x: &str, y: &str) -> usize {
    let a: Vec<char> = x.chars().collect();
    let b: Vec<char> = y.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() { row[0] = i; }
    for j in 0..=b.len() { d[0][j] = j; }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Elm `sort`: candidates ordered by distance to `target`, case-insensitive.
pub fn sort<T>(target: &str, to_string: impl Fn(&T) -> String, mut values: Vec<T>) -> Vec<T> {
    let target = target.to_lowercase();
    values.sort_by_cached_key(|v| distance(&target, &to_string(v).to_lowercase()));
    values
}

/// Elm `rank`.
pub fn rank<T>(target: &str, to_string: impl Fn(&T) -> String, values: Vec<T>) -> Vec<(usize, T)> {
    let target = target.to_lowercase();
    let mut ranked: Vec<(usize, T)> = values
        .into_iter()
        .map(|v| (distance(&target, &to_string(&v).to_lowercase()), v))
        .collect();
    ranked.sort_by_key(|(d, _)| *d);
    ranked
}
```

**Tests** — `distance_transposition_is_one` (`"ab"`/`"ba"`),
`distance_empty`, `sort_prefers_case_insensitive_match` (`"lenght"` →
`["length", "len", "height"]`), `rank_keeps_stable_order_for_ties`.

**Done when** tests pass. (Placed before the canonicalize chunk that needs
it; the brief listed it later.)

---

## Chunk 4 — Syntax errors A: module header, imports, exposing, space, weird ends

**Files**

- `crates/nash-report/src/syntax/mod.rs` (new)
- `crates/nash-report/src/syntax/module.rs` (new)

**Change**

Port `toReport` (the `Error` cases that survive: `ModuleNameUnspecified`,
`ModuleNameMismatch`, `ParseError`; port/effect cases are dropped with
their variants), `toParseErrorReport`, `toWeirdEndReport`,
`toImportReport`, `toExposingReport`, `toSpaceReport`. `toDeclStartReport`
and `toDeclarationsReport` are stubbed to `todo!()`-free by landing them in
this chunk too (they are small and `ModuleBadEnd` needs
`toDeclStartReport`).

Nash edits: "Elm" → "Nash" in prose; the example module names
(`Html.Attributes`, `Json.Decode`) become `Cardano.Tx`, `Data.Decoder`;
the `(..)` note stays.

**Code**

```rust
// syntax/mod.rs
//! `Reporting/Error/Syntax.hs`, split by error group.

mod decl;
mod expr;
mod module;
mod pattern;
mod type_;

use nash_parse::error::{Error, Space};
use nash_parse::{Col, Row};

use crate::code::{Source, to_region, to_wider_region};
use crate::{Doc, Report};

/// Elm's `toReport`.
pub fn to_report(source: &Source<'_>, error: &Error<'_>) -> Report {
    match error {
        Error::ModuleNameUnspecified(name) => Report {
            title: "MODULE NAME MISSING".into(),
            severity: crate::Severity::Error,
            region: to_region(1, 1),
            snippet: crate::Snippet::None,
            before: Doc::stack([
                Doc::reflow("I need the module name to be declared at the top of this file, like this:"),
                Doc::indent(4, Doc::fill_sep([Doc::text("module").cyan(), Doc::text(*name), Doc::text("exposing").cyan(), Doc::text("(..)")])),
                Doc::reflow("Try adding that as the first line of your file!"),
            ]),
            after: Doc::to_simple_note(
                "It is best to replace (..) with an explicit list of types and functions you want to expose. \
                 When you know a value is only used within this module, you can refactor without worrying \
                 about uses elsewhere. Limiting exposed values can also speed up compilation because I can \
                 skip a bunch of work if I see that the exposed API has not changed.",
            ),
            suggestions: Vec::new(),
        },
        Error::ModuleNameMismatch { expected, actual, row, col } => {
            let region = to_wider_region(*row, *col, actual.len() as u16);
            Report::snippet(
                "MODULE NAME MISMATCH",
                region,
                None,
                Doc::text("It looks like this module name is out of sync:"),
                Doc::stack([
                    Doc::reflow(&format!(
                        "I need it to match the file path, so I was expecting to see `{expected}` here. \
                         Make the following change, and you should be all set!"
                    )),
                    Doc::indent(4, Doc::cat([Doc::text(*actual).dullyellow(), Doc::text(" -> "), Doc::text(*expected).green()])),
                    Doc::to_simple_note(
                        "I require that module names correspond to file paths. This makes it much easier to \
                         explore unfamiliar codebases! So if you want to keep the current module name, try \
                         renaming the file instead.",
                    ),
                ]),
            )
            .with_suggestions(vec![expected.to_string()])
        }
        Error::Unsupported { feature, region } => Report::snippet(
            "UNSUPPORTED SYNTAX",
            *region,
            None,
            Doc::reflow(&format!("Nash does not have {feature}:")),
            Doc::reflow("This is Elm syntax that Nash dropped. Remove it and I can keep going."),
        ),
        Error::ParseError(module) => module::to_parse_error_report(source, module),
    }
}

/// Elm's `toSpaceReport`.
pub(crate) fn to_space_report(source: &Source<'_>, space: &Space, row: Row, col: Col) -> Report {
    let _ = source;
    match space {
        Space::HasTab => Report::snippet(
            "NO TABS",
            to_region(row, col),
            None,
            Doc::reflow("I ran into a tab, but tabs are not allowed in Nash files."),
            Doc::reflow("Replace the tab with spaces."),
        ),
        Space::EndlessMultiComment => Report::snippet(
            "ENDLESS COMMENT",
            to_wider_region(row, col, 2),
            None,
            Doc::reflow("I cannot find the end of this multi-line comment:"),
            Doc::stack([
                Doc::reflow("Add a -} somewhere after this to end the comment."),
                Doc::to_simple_hint(
                    "Multi-line comments can be nested in Nash, so {- {- -} -} is a comment that happens to \
                     contain another comment. Like parentheses and curly braces, the start and end markers \
                     must always be balanced. Maybe that is the problem?",
                ),
            ]),
        ),
    }
}
```

The port/effect variants of `nash_parse::error::Error`, `error::Module`,
and `error::Decl` are deleted by `plans/01-syntax.md` chunk 0, which
replaces them with `Error::Unsupported { feature, region }`. This chunk
matches that variant; it never matches port or effect arms.

`syntax/module.rs` exemplar — `toWeirdEndReport`:

```rust
/// Elm's `toWeirdEndReport`.
pub(crate) fn to_weird_end_report(source: &Source<'_>, row: Row, col: Col) -> Report {
    match source.what_is_next(row, col) {
        Next::Keyword(keyword) => Report::snippet(
            "RESERVED WORD",
            to_keyword_region(row, col, keyword),
            None,
            Doc::reflow("I got stuck on this reserved word:"),
            Doc::reflow(&format!("The name `{keyword}` is reserved, so try using a different name?")),
        ),
        Next::Operator(op) => Report::snippet(
            "UNEXPECTED SYMBOL",
            to_keyword_region(row, col, op),
            None,
            Doc::reflow("I ran into an unexpected symbol:"),
            Doc::reflow(&format!(
                "I was not expecting to see a {op} here. Try deleting it? Maybe I can give a better hint from there?"
            )),
        ),
        Next::Close(term, bracket) => Report::snippet(
            &format!("UNEXPECTED {}", term.to_uppercase()),
            to_region(row, col),
            None,
            Doc::reflow(&format!("I ran into an unexpected {term}:")),
            Doc::reflow(&format!(
                "This {bracket} does not match up with an earlier open {term}. Try deleting it?"
            )),
        ),
        Next::Lower(name) | Next::Upper(name) => Report::snippet(
            "UNEXPECTED NAME",
            to_keyword_region(row, col, name),
            None,
            Doc::reflow("I got stuck on this name:"),
            Doc::reflow(
                "It is confusing me a lot! Normally I can give fairly specific hints, but something is \
                 really tripping me up this time.",
            ),
        ),
        Next::Other(Some(c)) => Report::snippet(
            "UNEXPECTED CHARACTER",
            to_region(row, col),
            None,
            Doc::reflow("I got stuck on this character:"),
            Doc::reflow(&format!(
                "It is not a character I expect here (`{c}`). Try deleting it?"
            )),
        ),
        Next::Other(None) => Report::snippet(
            "UNFINISHED FILE",
            to_region(row, col),
            None,
            Doc::reflow("I got to the end of the file, but I was expecting more."),
            Doc::reflow("Maybe a declaration or an expression is incomplete?"),
        ),
    }
}
```

(Elm's `Upper` branch reads "It is confusing me a lot!" as well and the
`Other` branches follow the same shape; copy the exact strings from lines
930–1114 of `Syntax.hs`.)

**Elm reference** — `Syntax.hs` lines 490–1478: `toReport`,
`toParseErrorReport`, `toWeirdEndReport`, `toImportReport`,
`toExposingReport`, `toSpaceReport`, `toDeclarationsReport`,
`toDeclStartReport`.

**Tests** (`syntax/module.rs` `mod tests`, one helper for all syntax chunks)

```rust
fn parse_error_report(input: &str) -> String {
    let bump = Bump::new();
    let src = bump.alloc_str(input);
    let error = nash_parse::Parser::new(&bump, src.as_bytes()).module().expect_err("expected parse error");
    let report = to_report(&Source::new(input), &Error::ParseError(&error));
    render_plain(&report, &Source::new(input), "src/Main.nash")
}

macro_rules! assert_syntax_report_snapshot {
    ($input:expr) => {{
        let input = indoc!($input);
        insta::with_settings!({ description => format!("Code:\n\n{}", input), omit_expression => true }, {
            insta::assert_snapshot!(parse_error_report(input));
        });
    }};
}
```

- `module_name_missing` — `to_report` on `Error::ModuleNameUnspecified("Main")`
- `module_name_mismatch`
- `module_problem` — `module` alone
- `module_name_lowercase` — `module main exposing (..)`
- `exposing_missing_paren` — `module Main exposing ..`
- `exposing_value_bad` — `module Main exposing (1)`
- `import_missing_name` — `import`
- `import_bad_alias` — `import Cardano.Tx as tx`
- `import_exposing_list_missing_paren`
- `space_has_tab`
- `space_endless_comment`
- `weird_end_reserved_word` — `module Main exposing (..)\n\nif`
- `weird_end_close_paren` — `module Main exposing (..)\n\n)`
- `weird_end_operator`
- `fresh_line_after_decl` — `x = 1 y = 2` on one line

**Done when** all `Module`/`Exposing` variants of
`nash_parse::error` are covered by a match arm (no wildcard) and snapshots
accepted.

---

## Chunk 5 — Syntax errors B: declarations

**Files**

- `crates/nash-report/src/syntax/decl.rs` (new)

**Change**

Port `toDeclarationsReport` (move here from chunk 4 if it landed in
`module.rs`), `toDeclStartReport`, `toDeclTypeReport`,
`toTypeAliasReport`, `typeAliasNote`, `toCustomTypeReport`,
`customTypeNote`, `toDeclDefReport`, `declDefNote`. `toPortReport` and
`portNote` are dropped.

Nash edits: example declarations use Nash types (`type alias Account = {
owner : Bytes, balance : Int }`, `type option 'a = None | Some 'a`); the
"greet" example in `declDefNote` becomes `greet : string -> string` with
`++` kept (it is `Semigroup.append`).

**Code** — signatures:

```rust
pub(crate) fn to_declarations_report(source: &Source<'_>, decl: &Decl<'_>) -> Report;
pub(crate) fn to_decl_start_report(source: &Source<'_>, row: Row, col: Col) -> Report;
fn to_decl_type_report(source: &Source<'_>, decl_type: &DeclType<'_>, row: Row, col: Col) -> Report;
fn to_type_alias_report(source: &Source<'_>, alias: &TypeAlias<'_>, row: Row, col: Col) -> Report;
fn type_alias_note() -> Doc;
fn to_custom_type_report(source: &Source<'_>, custom: &CustomType<'_>, row: Row, col: Col) -> Report;
fn custom_type_note() -> Doc;
fn to_decl_def_report(source: &Source<'_>, name: &str, def: &DeclDef<'_>, row: Row, col: Col) -> Report;
fn decl_def_note() -> Doc;
```

Exemplar, `to_custom_type_report`'s `CT_Bar` arm (Elm `Syntax.hs` ~2010):

```rust
CustomType::Bar(row, col) => {
    let surroundings = Region::new(Position::new(start_row, start_col), Position::new(*row, *col));
    let region = to_region(*row, *col);
    Report::snippet(
        "UNFINISHED CUSTOM TYPE",
        region,
        Some(region),
        Doc::reflow("I am partway through parsing a custom type, but I got stuck here:"),
        Doc::stack([
            Doc::reflow("I was expecting to see a vertical bar like | next."),
            custom_type_note(),
        ]),
    )
    .with_region(surroundings)
}
```

Elm's `Code.toSnippet source surroundings (Just region)` shows the whole
declaration with the failure point highlighted; here that is
`Snippet::Region { region: surroundings, highlight: Some(region) }` and the
report `region` stays the point. Add
`Report::with_region(surroundings)` to chunk 1's API when the first
chunk needs it (it sets `snippet` to the wide region keeping `highlight`).

**Elm reference** — `Syntax.hs` 1441–2440.

**Tests** — `decl_start_close_bracket`, `decl_start_lowercase_keyword`
(`type` misspelt), `type_alias_missing_equals`, `type_alias_bad_body`,
`type_alias_indent_body`, `custom_type_missing_variant`,
`custom_type_bar_indent`, `custom_type_variant_arg_bad`,
`decl_def_missing_equals`, `decl_def_name_repeat` (`f : int\ng = 1`),
`decl_def_name_match`, `decl_def_indent_body`, `decl_def_arg_bad_pattern`,
`fresh_line_after_doc_comment`.

**Done when** all `Decl`, `DeclType`, `TypeAlias`, `CustomType`,
`DeclDef` variants are matched.

---

## Chunk 6 — Syntax errors C: expressions

**Files**

- `crates/nash-report/src/syntax/expr.rs` (new)

**Change**

Port `Context`/`Node`, `getDefName`, `isWithin`, `toExprReport`,
`toCharReport` (kept minimal: Nash has no char literals, but the error
variant still exists until plan 01 removes it), `toStringReport`,
`toEscapeReport`, `toNumberReport`, `toOperatorReport`, `toLetReport`,
`toUnfinishLetReport`, `toLetDefReport`, `defNote`, `toLetDestructReport`,
`toCaseReport`, `toUnfinishCaseReport`, `noteForCaseError`,
`noteForCaseIndentError`, `toIfReport`, `toRecordReport`,
`noteForRecordError`, `noteForRecordIndentError`, `toTupleReport`,
`toListReport`, `toFuncReport`. Shader arms are dropped with their variants.

Nash edits: the `case` example in `noteForCaseError` becomes

```
    case maybeWidth of
      Some width ->
        width + 200

      None ->
        400
```

and `Just`/`Nothing` become `Some`/`None` everywhere. `NumberDot` prose
loses the "Floats" sentence and says Nash has no floating point numbers.

**Code** — exemplar `to_case_report` (Elm 3595–3711):

```rust
/// Elm's `Context` for expression errors.
#[derive(Clone, Copy)]
pub(crate) enum Context<'c> {
    InNode(Node, Row, Col, &'c Context<'c>),
    InDef(&'c str, Row, Col),
    InDestruct(Row, Col),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Node { Record, Parens, List, Func, Cond, Then, Else, Case, Branch }

fn to_case_report(source: &Source<'_>, context: Context<'_>, case: &Case<'_>, start_row: Row, start_col: Col) -> Report {
    let surroundings = |row, col| Region::new(Position::new(start_row, start_col), Position::new(row, col));
    match *case {
        Case::Space(ref space, row, col) => to_space_report(source, space, row, col),
        Case::Of(row, col) | Case::IndentOf(row, col) => to_unfinish_case_report(
            source, row, col, start_row, start_col,
            Doc::fill_sep(["I", "was", "expecting", "to", "see", "the"].map(Doc::text).into_iter().chain([Doc::text("of").dullyellow(), Doc::text("keyword"), Doc::text("next.")])),
        ),
        Case::Pattern(pattern, row, col) => pattern::to_pattern_report(source, PContext::Case, pattern, row, col),
        Case::Arrow(row, col) => match source.what_is_next(row, col) {
            Next::Keyword(keyword) => Report::snippet(
                "RESERVED WORD",
                to_keyword_region(row, col, keyword),
                None,
                Doc::reflow("I am partway through parsing a `case` expression, but I got stuck here:"),
                Doc::reflow(&format!(
                    "It looks like you are trying to use `{keyword}` in one of your patterns, but it is a \
                     reserved word. Try using a different name?"
                )),
            )
            .with_region(surroundings(row, col)),
            Next::Operator(":") => Report::snippet(
                "UNEXPECTED OPERATOR",
                to_region(row, col),
                None,
                Doc::reflow("I am partway through parsing a `case` expression, but I got stuck here:"),
                Doc::fill_sep([
                    Doc::text("I"), Doc::text("am"), Doc::text("seeing"), Doc::text(":").dullyellow(),
                    Doc::text("but"), Doc::text("maybe"), Doc::text("you"), Doc::text("want"), Doc::text("::").green(),
                    Doc::text("instead?"), Doc::text("For"), Doc::text("pattern"), Doc::text("matching"), Doc::text("on"), Doc::text("lists?"),
                ]),
            )
            .with_region(surroundings(row, col)),
            Next::Operator("=") => Report::snippet(
                "UNEXPECTED OPERATOR",
                to_region(row, col),
                None,
                Doc::reflow("I am partway through parsing a `case` expression, but I got stuck here:"),
                Doc::fill_sep([
                    Doc::text("I"), Doc::text("am"), Doc::text("seeing"), Doc::text("=").dullyellow(),
                    Doc::text("but"), Doc::text("maybe"), Doc::text("you"), Doc::text("want"), Doc::text("->").green(), Doc::text("instead?"),
                ]),
            )
            .with_region(surroundings(row, col)),
            _ => Report::snippet(
                "MISSING ARROW",
                to_region(row, col),
                None,
                Doc::reflow("I am partway through parsing a `case` expression, but I got stuck here:"),
                Doc::stack([Doc::reflow("I was expecting to see an arrow next."), note_for_case_indent_error()]),
            )
            .with_region(surroundings(row, col)),
        },
        Case::Expr(expr, row, col) => to_expr_report(source, Context::InNode(Node::Case, start_row, start_col, &context), expr, row, col),
        Case::Branch(expr, row, col) => to_expr_report(source, Context::InNode(Node::Branch, start_row, start_col, &context), expr, row, col),
        Case::IndentExpr(row, col) => to_unfinish_case_report(source, row, col, start_row, start_col, Doc::reflow("I was expecting to see a expression next.")),
        Case::IndentPattern(row, col) => to_unfinish_case_report(source, row, col, start_row, start_col, Doc::reflow("I was expecting to see a pattern next.")),
        Case::IndentArrow(row, col) => to_unfinish_case_report(
            source, row, col, start_row, start_col,
            Doc::fill_sep(["I", "just", "saw", "a", "pattern,", "so", "I", "was", "expecting", "to", "see", "a"].map(Doc::text).into_iter().chain([Doc::text("->").dullyellow(), Doc::text("next.")])),
        ),
        Case::IndentBranch(row, col) => to_unfinish_case_report(
            source, row, col, start_row, start_col,
            Doc::reflow("I was expecting to see an expression next. What should I do when I run into this particular pattern?"),
        ),
        Case::PatternAlignment(indent, row, col) => to_unfinish_case_report(
            source, row, col, start_row, start_col,
            Doc::reflow(&format!("I suspect this is a pattern that is not indented far enough? ({indent} spaces)")),
        ),
    }
}

fn to_unfinish_case_report(source: &Source<'_>, row: Row, col: Col, start_row: Row, start_col: Col, message: Doc) -> Report {
    let _ = source;
    Report::snippet(
        "UNFINISHED CASE",
        to_region(row, col),
        None,
        Doc::reflow("I was partway through parsing a `case` expression, but I got stuck here:"),
        Doc::stack([message, note_for_case_error()]),
    )
    .with_region(Region::new(Position::new(start_row, start_col), Position::new(row, col)))
}
```

**Elm reference** — `Syntax.hs` 2423–4740.

**Tests** — one snapshot per branch that has distinct prose. Minimum set:
`expr_start_bad`, `expr_dot_without_name`, `operator_reserved_arrow`,
`operator_right_missing`, `string_endless_single`, `string_endless_multi`,
`escape_unknown`, `escape_bad_unicode`, `number_dot`, `number_hex_digit`,
`number_no_leading_zero`, `let_missing_in`, `let_def_alignment`,
`let_def_indent_body`, `let_destruct_missing_equals`, `case_missing_of`,
`case_missing_arrow`, `case_colon_instead_of_cons`,
`case_equals_instead_of_arrow`, `case_pattern_alignment`,
`case_indent_branch`, `if_missing_then`, `if_missing_else`,
`if_else_branch_start`, `record_missing_end`, `record_field_bad`,
`record_indent_end`, `tuple_missing_end`, `tuple_operator_close`,
`list_missing_end`, `list_indent_expr`, `func_missing_arrow`,
`func_indent_body`, `weird_end_in_def_context` (the `getDefName` path).

**Done when** every `Expr`, `Record`, `Tuple`, `List`, `Func`, `Case`,
`If`, `Let`, `Def`, `Destruct`, `StringError`, `Escape`, `Number`,
`BadOperator` variant is matched.

---

## Chunk 7 — Syntax errors D: patterns and types

**Files**

- `crates/nash-report/src/syntax/pattern.rs` (new)
- `crates/nash-report/src/syntax/type_.rs` (new)

**Change**

Port `PContext`, `toPatternReport`, `toPRecordReport`,
`toUnfinishRecordPatternReport`, `toPTupleReport`, `toPListReport`;
`TContext`, `toTypeReport`, `toTRecordReport`, `noteForRecordTypeError`,
`noteForRecordTypeIndentError`, `toTTupleReport`. `PFloat` prose changes:
Nash has no floats, so `PFloat` says "I do not accept floating point
numbers here; Nash has no `float` type." `TC_Port` is dropped.

Nash edit: the `PWildcardNotVar` message ("`_foo` ... underscore") is
unchanged. Type examples use `'a`.

**Code** — signatures:

```rust
pub(crate) enum PContext { Case, Arg, Let }
pub(crate) fn to_pattern_report(source: &Source<'_>, context: PContext, pattern: &Pattern<'_>, row: Row, col: Col) -> Report;
pub(crate) enum TContext<'c> { Annotation(&'c str), CustomType, TypeAlias }
pub(crate) fn to_type_report(source: &Source<'_>, context: TContext<'_>, tipe: &Type<'_>, row: Row, col: Col) -> Report;
```

**Elm reference** — `Syntax.hs` 4735–5847.

**Tests** — `pattern_start_bad`, `pattern_alias_missing_name`,
`pattern_wildcard_not_var`, `pattern_float`, `pattern_record_missing_end`,
`pattern_tuple_missing_end`, `pattern_list_missing_end`,
`pattern_in_case_vs_arg_vs_let` (three inputs, same variant, different
prose), `type_start_bad_in_annotation`, `type_start_bad_in_custom_type`,
`type_start_bad_in_alias`, `type_record_missing_colon`,
`type_record_indent_end`, `type_tuple_missing_end`.

**Done when** `nash_parse::error` has no variant without a report and the
`syntax` module has no `unreachable!`.

---

## Chunk 8 — Canonicalize errors

**Files**

- `crates/nash-report/src/canonicalize.rs` (new)
- `crates/nash-report/src/render_type.rs` (new; only `src_to_doc` is needed
  here — `aliasToUnionDoc` renders a source type)

**Change**

Port `Reporting/Error/Canonicalize.hs` `toReport` and helpers:
`toKindInfo`, `unboundTypeVars`, `nameClash`, `ambiguousName`,
`notFound`, `toQualString`, `aliasRecursionReport`, `aliasToUnionDoc`.
Port `Render/Type.hs` `Context`, `lambda`, `apply`, `tuple`, `record`,
`srcToDoc`, `srcFieldToDocs`, `collectSrcArgs` (`vrecord`,
`vrecordSnippet`, `canToDoc` come in chunk 9).

Mapping to `nash_can::Error` (`crates/nash-can/src/error.rs`):

| Nash variant | Elm case |
|---|---|
| `MissingModuleHeader` | `Syntax.ModuleNameUnspecified` — delegate to `syntax::to_report` with the expected name from the driver (add `expected_name: &str` parameter to `to_reports`) |
| `NotFoundType/Ctor/Var { prefix, name, suggestions }` | `notFound` with "type" / "variant" / "variable" |
| `AmbiguousType/Ctor/Var/Binop` | `ambiguousName` |
| `BadArity { context, .. }` | `BadArity` (`TypeArity` → "type", `PatternArity` → "variant") |
| `ExportNotFound { kind, name, suggestions }` | `ExportNotFound` with `toKindInfo` |
| `Duplicate*` | `nameClash` |
| `BinopFunctionNotFound { op, function }` | new: title `INFIX PROBLEM`, "The (`op`) operator says it is implemented by `function`, but I cannot find a `function` definition in this file." / "Define it, or point the `infix` declaration at an existing top-level value." |
| `RecursiveAlias` | `aliasRecursionReport` |
| `TypeVarsUnboundInUnion`, `TypeVarsMessedUpInAlias` | `unboundTypeVars` and the three-way `TypeVarsMessedUpInAlias` case (includes Elm's "delared" typo — fix it to "declared") |
| `PatternHasRecordCtor`, `DuplicatePattern`, `TupleLargerThanThree`, `NotFoundBinop`, `BinopConflict`, `Shadowing`, `RecursiveLet`, `RecursiveDecl`, `AnnotationTooShort`, `ImportExposingNotFound`, `ImportCtorByName`, `ImportOpenAlias`, `ExportOpenAlias`, `ExportDuplicate`, `ImportNotFound` | same-named Elm case |

Nash edits: "Elm does not have mutation" → "Nash"; `Basics#remainderBy` /
`modBy` links become `Int.rem` / `Int.mod` (stdlib names TBD, flag);
`(/=)` prose kept.

**Code** — exemplar `not_found`:

```rust
/// Elm's `notFound`.
fn not_found(region: Region, prefix: Option<&str>, name: &str, thing: &str, possible: PossibleNames<'_>) -> Report {
    let given_name = match prefix {
        Some(p) => to_qual_string(p, name),
        None => name.to_string(),
    };
    let mut possible_names: Vec<String> = possible.locals.iter().map(|s| s.to_string()).collect();
    for (module, names) in possible.qualified {
        possible_names.extend(names.iter().map(|n| to_qual_string(module, n)));
    }
    let nearby: Vec<String> = suggest::sort(&given_name, Clone::clone, possible_names).into_iter().take(4).collect();

    let to_details = |no_suggestions: &str, yes_suggestions: &str| -> Doc {
        let hint = Doc::link("Hint", "Read", "imports", "to see how `import` declarations work in Nash.");
        if nearby.is_empty() {
            Doc::stack([Doc::reflow(no_suggestions), hint])
        } else {
            Doc::stack([
                Doc::reflow(yes_suggestions),
                Doc::indent(4, Doc::vcat(nearby.iter().map(|n| Doc::text(n.as_str()).dullyellow()))),
                hint,
            ])
        }
    };

    let after = match prefix {
        None => to_details("Is there an `import` or `exposing` missing up top?", "These names seem close though:"),
        Some(p) => {
            if possible.qualified.iter().any(|(module, _)| *module == p) {
                to_details(
                    &format!("The `{p}` module does not expose a `{name}` {thing}."),
                    &format!("The `{p}` module does not expose a `{name}` {thing}. These names seem close though:"),
                )
            } else {
                to_details(
                    &format!("I cannot find a `{p}` module. Is there an `import` for it?"),
                    &format!("I cannot find a `{p}` import. These names seem close though:"),
                )
            }
        }
    };

    Report::snippet("NAMING ERROR", region, None, Doc::reflow(&format!("I cannot find a `{given_name}` {thing}:")), after)
        .with_suggestions(nearby)
}

fn to_qual_string(prefix: &str, name: &str) -> String { format!("{prefix}.{name}") }
```

Exemplar `name_clash` with `Snippet::Pair`:

```rust
/// Elm's `nameClash`.
fn name_clash(r1: Region, r2: Region, message: &str) -> Report {
    Report::pair(
        "NAME CLASH",
        Label { region: r1, text: "one here".into() },
        Label { region: r2, text: "and another one here".into() },
        Doc::reflow(&format!("{message} One here:")),
        Doc::text("How can I know which one you want? Rename one of them!"),
    )
}
```

**Elm reference** — all of `Reporting/Error/Canonicalize.hs` except the
port/effect cases; `Render/Type.hs` 1–160.

**Tests** — helper `can_error_reports(input) -> String` (parse,
canonicalize with `Context::default()`, expect `Err`, map every error
through `to_report`, join with a blank line). One test per Nash variant:
`not_found_var`, `not_found_var_with_suggestion`,
`not_found_qualified_no_import`, `not_found_qualified_not_exposed`,
`ambiguous_var`, `ambiguous_type_with_prefix`, `bad_arity_too_few`,
`bad_arity_too_many`, `export_not_found`, `export_open_alias`,
`duplicate_decl`, `duplicate_type`, `duplicate_ctor`, `duplicate_field`,
`duplicate_pattern_case`, `binop_function_not_found`, `binop_conflict`,
`recursive_alias_single`, `recursive_alias_cycle`, `unbound_type_var`,
`unused_alias_var`, `alias_vars_messed_up`, `shadowing`,
`recursive_decl_self`, `recursive_decl_cycle`, `recursive_let`,
`annotation_too_short`, `tuple_larger_than_three`, `import_exposing_not_found`,
`import_ctor_by_name`, `import_open_alias`, `not_found_binop_suggestion`,
`not_found_binop_percent`.

**Done when** every `nash_can::Error` variant has an arm and a snapshot.

---

## Chunk 9 — `Localizer`, `render_type`, `type_diff`

**Files**

- `crates/nash-report/src/localizer.rs` (new)
- `crates/nash-report/src/render_type.rs` (extend)
- `crates/nash-report/src/type_diff.rs` (new)

**Change**

1. `Localizer` from `Reporting/Render/Type/Localizer.hs`, over
   `nash_source::Module` imports plus the prelude imports.
2. `render_type.rs` gains `vrecord`, `vrecord_snippet`, `can_to_doc`,
   `can_field_to_doc`, `collect_args`.
3. `type_diff.rs` ports `Type/Error.hs`: `to_doc`, `alias_to_doc`,
   `fields_to_docs`, `ext_to_doc`, `Diff`, `Status`, `Problem`,
   `Direction`, `merge`, `to_comparison`, `to_diff`, `same`, `similar`,
   `different`, `is_similar`, `is_bool`/`is_int`/`is_string`/`is_list`
   (over `nash_constrain::error_type` helpers), `is_option` (Elm
   `isMaybe`), `name_clash_to_doc`, `diff_aliased_record`, `diff_record`,
   `has_fixed_fields`, `ext_to_diff`, `ext_to_status`.

Nash `Problem`:

```rust
pub enum Problem<'a> {
    AnythingToBool,
    AnythingFromOption,
    ArityMismatch(usize, usize),
    BadRigidVar(&'a str, &'a ErrorType<'a>),
    FieldTypo(&'a str, Vec<&'a str>),
    FieldsMissing(Vec<&'a str>),
    /// `Int` vs `int`, `List` vs `list`, ...: same name up to case.
    BigLittle { big: &'a str, little: &'a str, direction: Direction },
}
pub enum Direction { Have, Need }
```

`FlexSuper`/`RigidSuper` arms of `to_diff` are removed together with
`ErrorType::FlexSuper`/`RigidSuper` (plan 03 removes `Super`; until then
they fall into the `pair` catch-all with no problem).

**Code** — `Localizer`:

```rust
//! `Reporting/Render/Type/Localizer.hs`: render type names the way this
//! module could write them.

use std::collections::{BTreeMap, BTreeSet};

use nash_ast::ModuleName;
use nash_source::{Exposed, Exposing, Import, Module};

#[derive(Clone, Debug, Default)]
pub struct Localizer {
    imports: BTreeMap<String, ImportInfo>,
}

#[derive(Clone, Debug)]
struct ImportInfo {
    alias: Option<String>,
    exposing: ExposedTypes,
}

#[derive(Clone, Debug)]
enum ExposedTypes {
    All,
    Only(BTreeSet<String>),
}

impl Localizer {
    /// Elm's `fromModule`, plus the implicit prelude imports canonicalization
    /// added (`nash_can::imports::defaults()`, Elm's `Imports.defaults`;
    /// see `docs/stdlib.md`).
    pub fn from_module(module: &Module<'_>, defaults: &[&Import<'_>]) -> Localizer {
        let mut imports = BTreeMap::new();
        if let Some(name) = module.name {
            imports.insert(name.value.to_string(), ImportInfo { alias: None, exposing: ExposedTypes::All });
        }
        for import in defaults.iter().chain(module.imports.iter()) {
            imports.insert(import.import.value.to_string(), to_info(import));
        }
        Localizer { imports }
    }

    /// Elm's `fromNames`: every listed module counts as `exposing (..)`.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Localizer {
        Localizer {
            imports: names.into_iter().map(|n| (n.to_string(), ImportInfo { alias: None, exposing: ExposedTypes::All })).collect(),
        }
    }

    /// Elm's `toChars`.
    pub fn to_string(&self, home: ModuleName<'_>, name: &str) -> String {
        match self.imports.get(home.name) {
            None => format!("{}.{name}", home.name),
            Some(ImportInfo { exposing: ExposedTypes::All, .. }) => name.to_string(),
            Some(ImportInfo { alias, exposing: ExposedTypes::Only(set) }) => {
                if set.contains(name) {
                    name.to_string()
                } else {
                    format!("{}.{name}", alias.as_deref().unwrap_or(home.name))
                }
            }
        }
    }

    pub fn to_doc(&self, home: ModuleName<'_>, name: &str) -> Doc { Doc::text(self.to_string(home, name)) }
}

fn to_info(import: &Import<'_>) -> ImportInfo {
    let exposing = match import.exposing {
        Exposing::Open => ExposedTypes::All,
        Exposing::Explicit(exposed) => ExposedTypes::Only(
            exposed.iter().filter_map(|e| match e {
                Exposed::Upper { name, .. } | Exposed::LowerType { name, .. } => Some(name.value.to_string()),
                Exposed::Lower(_) | Exposed::Operator { .. } => None,
            }).collect(),
        ),
    };
    ImportInfo { alias: import.alias.map(str::to_string), exposing }
}
```

`to_diff` core (Elm `Type/Error.hs` 235–350), abbreviated to the arms
that change:

```rust
pub struct Diff<T> { pub left: T, pub right: T, pub status: Status }
pub enum Status { Similar, Different(Vec<Problem<'static>>) }   // lifetimes elided in the sketch

pub fn to_comparison(localizer: &Localizer, t1: &ErrorType<'_>, t2: &ErrorType<'_>) -> (Doc, Doc, Vec<Problem>) {
    let Diff { left, right, status } = to_diff(localizer, Ctx::None, t1, t2);
    (left, right, match status { Status::Similar => Vec::new(), Status::Different(ps) => ps })
}

fn to_diff(localizer: &Localizer, ctx: Ctx, t1: &ErrorType<'_>, t2: &ErrorType<'_>) -> Diff<Doc> {
    use ErrorType::*;
    match (t1, t2) {
        (Unit, Unit) | (Error, Error) | (Infinite, Infinite) => same(localizer, ctx, t1),
        (FlexVar(x), FlexVar(y)) | (RigidVar(x), RigidVar(y)) if x == y => same(localizer, ctx, t1),
        (FlexVar(_), _) | (_, FlexVar(_)) => similar(localizer, ctx, t1, t2),

        (Lambda(a, b, cs), Lambda(x, y, zs)) if cs.len() == zs.len() => {
            let arg1 = to_diff(localizer, Ctx::Func, a, x);
            let arg2 = to_diff(localizer, Ctx::Func, b, y);
            let rest: Vec<Diff<Doc>> = cs.iter().zip(zs.iter()).map(|(c, z)| to_diff(localizer, Ctx::Func, c, z)).collect();
            Diff::map3(arg1, arg2, Diff::sequence(rest), |a, b, cs| render_type::lambda(ctx, a, b, cs))
        }
        (Lambda(a, b, cs), Lambda(x, y, zs)) => different(
            render_type::lambda(ctx, to_doc(localizer, Ctx::Func, a), to_doc(localizer, Ctx::Func, b), cs.iter().map(|c| to_doc(localizer, Ctx::Func, c)).collect()).dullyellow(),
            render_type::lambda(ctx, to_doc(localizer, Ctx::Func, x), to_doc(localizer, Ctx::Func, y), zs.iter().map(|z| to_doc(localizer, Ctx::Func, z)).collect()).dullyellow(),
            vec![Problem::ArityMismatch(2 + cs.len(), 2 + zs.len())],
        ),

        (Tuple(a, b, None), Tuple(x, y, None)) => Diff::map2(to_diff(localizer, Ctx::None, a, x), to_diff(localizer, Ctx::None, b, y), |a, b| render_type::tuple(a, b, vec![])),
        (Tuple(a, b, Some(c)), Tuple(x, y, Some(z))) => Diff::map3(to_diff(localizer, Ctx::None, a, x), to_diff(localizer, Ctx::None, b, y), to_diff(localizer, Ctx::None, c, z), |a, b, c| render_type::tuple(a, b, vec![c])),

        (Record { fields: f1, ext: e1 }, Record { fields: f2, ext: e2 }) => diff_record(localizer, f1, *e1, f2, *e2),

        (Type { home: h1, name: n1, args: a1 }, Type { home: h2, name: n2, args: a2 }) if h1 == h2 && n1 == n2 => {
            let args: Vec<Diff<Doc>> = a1.iter().zip(a2.iter()).map(|(a, b)| to_diff(localizer, Ctx::App, a, b)).collect();
            Diff::sequence(args).map(|args| render_type::apply(ctx, localizer.to_doc(*h1, n1), args))
        }
        (Alias { home: h1, name: n1, args: a1, .. }, Alias { home: h2, name: n2, args: a2, .. }) if h1 == h2 && n1 == n2 => { /* same over `args[i].1` */ }

        // Nash: `Int` vs `int` and friends.
        (Type { home: h1, name: n1, args: a1 }, Type { home: h2, name: n2, args: a2 })
            if is_big_little_pair(n1, n2) && a1.len() == a2.len() =>
        {
            let (big, little, direction) = if n1.starts_with(char::is_uppercase) { (*n1, *n2, Direction::Have) } else { (*n2, *n1, Direction::Need) };
            different(
                render_type::apply(ctx, localizer.to_doc(*h1, n1).dullyellow(), a1.iter().map(|a| to_doc(localizer, Ctx::App, a)).collect()),
                render_type::apply(ctx, localizer.to_doc(*h2, n2).dullyellow(), a2.iter().map(|a| to_doc(localizer, Ctx::App, a)).collect()),
                vec![Problem::BigLittle { big, little, direction }],
            )
        }

        (Type { home: h1, name: n1, args: a1 }, Type { home: h2, name: n2, args: a2 })
            if localizer.to_string(*h1, n1) == localizer.to_string(*h2, n2) =>
        {
            different(name_clash_to_doc(ctx, localizer, *h1, n1, a1), name_clash_to_doc(ctx, localizer, *h2, n2, a2), vec![])
        }
        (Type { home, name, args: [t1] }, t2) if is_option(*home, name) && is_similar(&to_diff(localizer, ctx, t1, t2)) => different(
            render_type::apply(ctx, localizer.to_doc(*home, name).dullyellow(), vec![to_doc(localizer, Ctx::App, t1)]),
            to_doc(localizer, ctx, t2),
            vec![Problem::AnythingFromOption],
        ),
        (t1, Type { home, name, args: [t2] }) if is_list(*home, name) && is_similar(&to_diff(localizer, ctx, t1, t2)) => different(
            to_doc(localizer, ctx, t1),
            render_type::apply(ctx, localizer.to_doc(*home, name).dullyellow(), vec![to_doc(localizer, Ctx::App, t2)]),
            vec![],
        ),

        (Alias { home, name, args, real }, t2) => match diff_aliased_record(localizer, real, t2) {
            Some(Diff { right, status, .. }) => Diff { left: alias_to_doc(localizer, ctx, *home, name, args).dullyellow(), right, status },
            None => fallback_alias_left(localizer, ctx, t1, t2),
        },
        (t1, Alias { .. }) => { /* mirror */ }

        _ => {
            let problems = match (t1, t2) {
                (RigidVar(x), other) | (other, RigidVar(x)) => vec![Problem::BadRigidVar(x, other)],
                (_, Type { home, name, args: [] }) if is_bool(*home, name) => vec![Problem::AnythingToBool],
                _ => vec![],
            };
            different(to_doc(localizer, ctx, t1).dullyellow(), to_doc(localizer, ctx, t2).dullyellow(), problems)
        }
    }
}

/// `Int`/`int`, `Bytes`/`bytes`, `List`/`list`, `Map`/`map`.
fn is_big_little_pair(n1: &str, n2: &str) -> bool {
    n1 != n2 && n1.eq_ignore_ascii_case(n2)
}
```

`diff_record` is a straight port (Elm 470–520); `BTreeMap` replaces
`Map`, and `nash_constrain::Extension` is used as is.

**Elm reference** — `Localizer.hs` (all), `Render/Type.hs` 160–256,
`Type/Error.hs` 1–588.

**Tests**

- `localizer_bare_when_exposed`, `localizer_alias_when_not_exposed`,
  `localizer_qualified_when_not_imported`, `localizer_prelude_bare`
  (`from_module` with a default `import Prelude exposing (..)`)
- `can_to_doc_lambda_breaks_when_long` (`render(40)` snapshot)
- `can_to_doc_record`, `can_to_doc_alias_with_args`
- `to_comparison_same_types_no_problems`
- `to_comparison_int_vs_int_little` → `BigLittle`
- `to_comparison_list_int_vs_list_int_little` — nested diff only marks the
  element
- `to_comparison_record_field_typo` → `FieldTypo("nmae", ["name", ..])`
- `to_comparison_record_fields_missing`
- `to_comparison_arity_mismatch`
- `to_comparison_rigid_var` → `BadRigidVar`
- `to_comparison_option_vs_bare` → `AnythingFromOption`
- `to_comparison_name_clash_same_localized_name` — two `Account` types from
  different modules render as `Ledger.Account` vs `Bank.Account`

Type inputs are built by hand in a `Bump` as `ErrorType` values; snapshot
the two rendered docs and `Debug` of the problems.

**Done when** `to_diff` has no wildcard arm hiding a case Elm handles
separately, and tests pass.

---

## Chunk 10 — Type errors

**Files**

- `crates/nash-report/src/type_.rs` (new)

**Change**

Port `Reporting/Error/Type.hs`: `toReport`, `toPatternReport`,
`patternTypeComparison`, `addPatternCategory`, `typeComparison`,
`loneType`, `addCategory`, `problemsToHint`, `problemToHint`,
`badRigidVar`, `badDoubleRigid`, `toExprReport`, `countArgs`,
`toNearbyRecord`, `fieldToDocs`, `extToDoc`, `opLeftToDocs`,
`RightDocs`, `opRightToDocs`, `badOpRightFallback`, `isInt`, `isString`,
`isList`, `badConsRight`, `AppendType`, `toAppendType`, `badAppendLeft`,
`badAppendRight`, `badMath`, `badBool`, `badCompLeft`, `badCompRight`,
`badEquality`, `toInfiniteReport`.

Dropped: `toASuperThing`, `badFlexSuper`, `badRigidSuper`,
`badFlexFlexSuper`, `badCast`, `badCastHelp`, `badFDiv`, `badIDiv`,
`badStringAdd`, `badListAdd`, `badListMul`, the `Float`/`Char`/`Shader`/
`Effects` categories, and the `IntFloat`/`String*` problem hints.

Nash edits:

- `addCategory`: `Number -> " a number of type:"` stays (int literals);
  `Char`/`Float` arms gone.
- `Negate`: "But I only know how to negate `Int` and `Float` values." →
  "But I only know how to negate values whose type implements `Num`."
- `opLeftToDocs`/`opRightToDocs`: `+ - * ^` keep `badMath` with the
  closing sentence "But (+) only works with values whose type implements
  `Num`." `/` and `//` collapse into one `badDiv` for `/` ("The (/)
  operator is integer division; both sides must be `int`." Big twins have
  no arithmetic).
  `&&`, `||` say `bool`. `< > <= >=` mention `Ord`. `==`, `/=` mention
  `Eq` and drop the IEEE 754 note. `::`, `++`, `<|`, `|>` unchanged
  except `String.fromInt` → `Int.toString` hints (name TBD).
- `IfCondition`: "True or False" → "`True` or `False`", `Bool` →
  `bool`.
- `problemToHint` gains:

```rust
Problem::BigLittle { big, little, direction } => vec![Doc::to_simple_hint(&format!(
    "`{big}` is the Big (Data) type and `{little}` is the little builtin one. They never convert \
     implicitly. Use `lower` to go from `{big}` to `{little}`, or `lift` to go the other way.{}",
    match direction { Direction::Have => "", Direction::Need => " If this value comes from a validator argument, decode it with `fromData` first." }
))],
Problem::AnythingFromOption => vec![Doc::to_fancy_hint([
    "Use", GREEN("Option.withDefault"), "to", "handle", "possible", "errors.", "Longer", "term,", "it",
    "is", "usually", "better", "to", "write", "out", "the", "full", "`case`", "though!",
])],
```

**Code** — exemplar: `to_expr_report`'s `FromAnnotation` and `CallArg`
paths (Elm 640–700, 760–790):

```rust
/// Elm's `toReport`.
pub fn to_report(localizer: &Localizer, error: &Error<'_>) -> Report {
    match error {
        Error::BadExpr(region, category, actual, expected) => to_expr_report(localizer, *region, *category, actual, expected),
        Error::BadPattern(region, category, actual, expected) => to_pattern_report(localizer, *region, *category, actual, expected),
        Error::InfiniteType { region, name, overall_type } => to_infinite_report(localizer, *region, name, overall_type),
    }
}

fn to_expr_report(localizer: &Localizer, expr_region: Region, category: Category<'_>, tipe: &ErrorType<'_>, expected: &Expected<'_, &ErrorType<'_>>) -> Report {
    match expected {
        Expected::NoExpectation(expected_type) => Report::snippet(
            "TYPE MISMATCH",
            expr_region,
            None,
            Doc::text("This expression is being used in an unexpected way:"),
            type_comparison(localizer, tipe, expected_type, &add_category("It is", category), "But you are trying to use it as:", vec![]),
        ),

        Expected::FromAnnotation(name, _arity, sub_context, expected_type) => {
            let (thing, it_is) = match sub_context {
                SubContext::TypedIfBranch(index) => (format!("{} branch of this `if` expression:", ordinal(*index)), format!("The {} branch is", ordinal(*index))),
                SubContext::TypedCaseBranch(index) => (format!("{} branch of this `case` expression:", ordinal(*index)), format!("The {} branch is", ordinal(*index))),
                SubContext::TypedBody => (format!("body of the `{name}` definition:"), "The body is".to_string()),
            };
            Report::snippet(
                "TYPE MISMATCH",
                expr_region,
                None,
                Doc::reflow(&format!("Something is off with the {thing}")),
                type_comparison(
                    localizer, tipe, expected_type,
                    &add_category(&it_is, category),
                    &format!("But the type annotation on `{name}` says it should be:"),
                    vec![],
                ),
            )
        }

        Expected::FromContext(region, context, expected_type) => {
            let mismatch = |highlight: Option<Region>, problem: &str, this_is: &str, instead_of: &str, details: Vec<Doc>| {
                Report::snippet(
                    "TYPE MISMATCH",
                    expr_region,
                    highlight,
                    Doc::reflow(problem),
                    type_comparison(localizer, tipe, expected_type, &add_category(this_is, category), instead_of, details),
                )
                .with_region(*region)
            };
            let bad_type = |highlight: Option<Region>, problem: &str, this_is: &str, details: Vec<Doc>| {
                Report::snippet(
                    "TYPE MISMATCH",
                    expr_region,
                    highlight,
                    Doc::reflow(problem),
                    lone_type(localizer, tipe, expected_type, Doc::reflow(&add_category(this_is, category)), details),
                )
                .with_region(*region)
            };
            match context {
                Context::CallArg(maybe_name, index) => {
                    let ith = ordinal(*index);
                    let this_function = match maybe_name {
                        MaybeName::NoName => "this function".to_string(),
                        MaybeName::FuncName(n) | MaybeName::CtorName(n) => format!("`{n}`"),
                        MaybeName::OpName(op) => format!("({op})"),
                    };
                    mismatch(
                        Some(expr_region),
                        &format!("The {ith} argument to {this_function} is not what I expect:"),
                        "This argument is",
                        &format!("But {this_function} needs the {ith} argument to be:"),
                        if *index == 0 { vec![] } else { vec![Doc::to_simple_hint(
                            "I always figure out the argument types from left to right. If an argument is \
                             acceptable, I assume it is “correct” and move on. So the problem may actually be \
                             in one of the previous arguments!",
                        )] },
                    )
                }
                /* ListEntry, Negate, OpLeft, OpRight, IfCondition, IfBranch, CaseBranch,
                   CallArity, RecordAccess, RecordUpdateKeys, RecordUpdateValue, Destructure:
                   straight ports */
                _ => todo!(),
            }
        }
    }
}

/// Elm's `typeComparison`.
fn type_comparison(localizer: &Localizer, actual: &ErrorType<'_>, expected: &ErrorType<'_>, i_am_seeing: &str, instead_of: &str, context_hints: Vec<Doc>) -> Doc {
    let (actual_doc, expected_doc, problems) = type_diff::to_comparison(localizer, actual, expected);
    Doc::stack(
        [Doc::reflow(i_am_seeing), Doc::indent(4, actual_doc), Doc::reflow(instead_of), Doc::indent(4, expected_doc)]
            .into_iter()
            .chain(context_hints)
            .chain(problems_to_hint(&problems)),
    )
}
```

(`todo!()` never survives the chunk; it marks where the remaining arms go.)

**Elm reference** — all of `Reporting/Error/Type.hs`.

**Tests** — helper `type_error_reports(input) -> String` (parse,
canonicalize, constrain, solve, expect `Err`, `Localizer::from_module`,
render each). Inputs in Nash syntax; the current solver produces the
errors, so they must type-fail today:

- `mismatch_annotation_body` (example 1 in `docs/diagnostics.md`)
- `mismatch_if_branches`, `mismatch_case_branches`, `mismatch_list_entries`
- `mismatch_if_condition_not_bool`
- `mismatch_call_arg_first`, `mismatch_call_arg_second_has_hint`
- `too_many_args_on_value`, `too_many_args_on_function`
- `record_access_missing_field_typo`, `record_access_on_non_record`
- `record_update_unknown_field`, `record_update_change_type`
- `op_plus_left_string`, `op_cons_right_not_list`, `op_append_string_list`,
  `op_compare_mismatch`, `op_equality_mismatch`, `op_pipe_right_not_function`
- `pattern_case_first_mismatch`, `pattern_case_later_mismatch`,
  `pattern_ctor_arg_mismatch`, `pattern_typed_arg_mismatch`,
  `pattern_list_tail`
- `infinite_type`
- `rigid_var_mismatch` (`'a` annotation used as `int`)
- `destructure_mismatch`

**Done when** every `Context`, `SubContext`, `Category`, `PContext`,
`PCategory` variant is matched and the example 1 snapshot matches
`docs/diagnostics.md` up to miette glyph details.

---

## Chunk 11 — Pattern errors and warnings

**Files**

- `crates/nash-report/src/pattern.rs` (new)
- `crates/nash-report/src/warning.rs` (new)

**Change**

Port `Reporting/Error/Pattern.hs` `toReport` and
`unhandledPatternsToDocBlock` (pattern text from
`nash_nitpick::render::pattern_to_string`). Port `Reporting/Warning.hs`
`toReport` for `UnusedImport`, `UnusedVariable` (`MissingTypeAnnotation`
waits for plan 03's warning).

Nash edit: `Debug.todo` → `todo`.

**Code**

```rust
//! `Reporting/Error/Pattern.hs`.

use nash_nitpick::render::{RenderContext, pattern_to_string};
use nash_nitpick::{Context, Error, Pattern};

pub fn to_report(error: &Error<'_>) -> Report {
    match error {
        Error::Redundant { case_region, pattern_region, index } => Report::snippet(
            "REDUNDANT PATTERN",
            *pattern_region,
            Some(*pattern_region),
            Doc::reflow(&format!("The {} pattern is redundant:", int_to_ordinal(*index))),
            Doc::reflow("Any value with this shape will be handled by a previous pattern, so it should be removed."),
        )
        .with_region(*case_region),

        Error::Incomplete { region, context, unhandled } => match context {
            Context::BadArg => Report::snippet(
                "UNSAFE PATTERN",
                *region,
                None,
                Doc::text("This pattern does not cover all possibilities:"),
                Doc::stack([
                    Doc::text("Other possibilities include:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::reflow(
                        "I would have to crash if I saw one of those! So rather than pattern matching in \
                         function arguments, put a `case` in the function body to account for all possibilities.",
                    ),
                ]),
            ),
            Context::BadDestruct => Report::snippet(
                "UNSAFE PATTERN",
                *region,
                None,
                Doc::text("This pattern does not cover all possible values:"),
                Doc::stack([
                    Doc::text("Other possibilities include:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::reflow(
                        "I would have to crash if I saw one of those! You can use `let` to deconstruct values \
                         only if there is ONE possibility. Switch to a `case` expression to account for all \
                         possibilities.",
                    ),
                    Doc::to_simple_hint(
                        "Are you calling a function that definitely returns values with a very specific shape? \
                         Try making the return type of that function more specific!",
                    ),
                ]),
            ),
            Context::BadCase => Report::snippet(
                "MISSING PATTERNS",
                *region,
                None,
                Doc::text("This `case` does not have branches for all possibilities:"),
                Doc::stack([
                    Doc::text("Missing possibilities include:"),
                    unhandled_patterns_to_doc_block(unhandled),
                    Doc::reflow("I would have to crash if I saw one of those. Add branches for them!"),
                    Doc::link(
                        "Hint",
                        "If you want to write the code for each branch later, use `todo` as a placeholder. Read",
                        "missing-patterns",
                        "for more guidance on this workflow.",
                    ),
                ]),
            ),
        },
    }
}

fn unhandled_patterns_to_doc_block(unhandled: &[Pattern<'_>]) -> Doc {
    Doc::indent(4, Doc::vcat(unhandled.iter().map(|p| Doc::text(pattern_to_string(RenderContext::Unambiguous, *p)))).dullyellow())
}
```

`warning.rs`:

```rust
pub fn to_report(warning: &Warning<'_>) -> Report {
    match warning {
        Warning::UnusedImport { region, module_name } => Report::snippet(
            "unused import", *region, None,
            Doc::reflow(&format!("Nothing from the `{module_name}` module is used in this file.")),
            Doc::text("I recommend removing unused imports."),
        ).warning(),
        Warning::UnusedVariable { region, context, name } => {
            let (title, advice) = match context {
                WarningContext::Def => ("unused definition", "If you are sure there is no typo, remove the definition. This way future readers will not have to wonder why it is there!".to_string()),
                WarningContext::Pattern => ("unused variable", format!("If you are sure there is no typo, replace `{name}` with _ so future readers will not have to wonder why it is there!")),
            };
            Report::snippet(
                title, *region, None,
                Doc::reflow(&format!("You are not using `{name}` anywhere.")),
                Doc::stack([
                    Doc::reflow(&format!("Is there a typo? Maybe you intended to use `{name}` somewhere but typed another name instead?")),
                    Doc::reflow(&advice),
                ]),
            ).warning()
        }
    }
}
```

**Elm reference** — `Reporting/Error/Pattern.hs` `toReport`,
`unhandledPatternsToDocBlock`; `Reporting/Warning.hs` `toReport`,
`defOrPat`.

**Tests** — pipeline helper through `nash_nitpick::check`:
`missing_patterns_data` (example 3), `missing_patterns_nested_list`,
`unsafe_arg`, `unsafe_destruct`, `redundant_pattern`; warnings:
`unused_import`, `unused_variable_pattern`, `unused_definition`.

**Done when** example 3's snapshot matches `docs/diagnostics.md`.

---

## Chunk 12 — `ModuleError`, JSON output

**Files**

- `crates/nash-report/src/lib.rs` (`ModuleError`, `to_reports`, `ModuleReports`)
- `crates/nash-report/src/json.rs` (new)

**Change**

Add the aggregate types from `docs/diagnostics.md` and Elm's
`Reporting/Error.hs` `toReports` / `toJson` / `reportToJson` /
`encodeRegion`, plus `Doc.encode` via `Doc::chunks`. For JSON the snippet
is rendered in Elm's own text style (`Render/Code.hs` `render`,
`makeUnderline`, `drawLines`, `addLineNumber`, `renderPair`) so the JSON
`message` is self-contained. Put that in `code.rs` as
`Source::snippet_doc(region, highlight) -> Doc` and
`Source::pair_doc(r1, r2) -> Doc`.

**Code**

```rust
/// Elm's `Reporting.Error.Error`.
pub enum ModuleError<'a> {
    Syntax(nash_parse::error::Error<'a>),
    Names(Vec<nash_can::Error<'a>>),
    Types(Localizer, Vec<nash_constrain::Error<'a>>),
    Patterns(Vec<nash_nitpick::Error<'a>>),
}

/// Elm's `toReports`.
pub fn to_reports(source: &Source<'_>, expected_module_name: &str, error: &ModuleError<'_>) -> Vec<Report> {
    match error {
        ModuleError::Syntax(e) => vec![syntax::to_report(source, e)],
        ModuleError::Names(errors) => errors.iter().map(|e| canonicalize::to_report(source, expected_module_name, e)).collect(),
        ModuleError::Types(localizer, errors) => errors.iter().map(|e| type_::to_report(localizer, e)).collect(),
        ModuleError::Patterns(errors) => errors.iter().map(pattern::to_report).collect(),
    }
}

/// Everything the CLI/LSP needs after the module arena is gone.
#[derive(Debug)]
pub struct ModuleReports {
    pub name: String,
    pub path: String,
    pub source: String,
    pub reports: Vec<Report>,
}

impl ModuleReports {
    pub fn render(&self, color: bool) -> Vec<Rendered> { let s = Source::new(&self.source); self.reports.iter().map(|r| r.render(&s, &self.path, color)).collect() }
    pub fn json(&self) -> serde_json::Value { json::module_to_json(self) }
}
```

`json.rs`:

```rust
pub fn module_to_json(module: &ModuleReports) -> serde_json::Value {
    let source = Source::new(&module.source);
    serde_json::json!({
        "path": module.path,
        "name": module.name,
        "problems": module.reports.iter().map(|r| report_to_json(&source, r)).collect::<Vec<_>>(),
    })
}

fn report_to_json(source: &Source<'_>, report: &Report) -> serde_json::Value {
    let message = Doc::vcat([report.before.clone(), Doc::Empty, source.snippet_doc(&report.snippet), report.after.clone()]);
    serde_json::json!({
        "title": report.title,
        "region": encode_region(report.region),
        "message": message.chunks(WIDTH).into_iter().map(chunk_to_json).collect::<Vec<_>>(),
    })
}

fn chunk_to_json(chunk: Chunk) -> serde_json::Value {
    match chunk {
        Chunk::Plain(s) => serde_json::Value::String(s),
        Chunk::Styled { style, text } => serde_json::json!({
            "bold": style.bold,
            "underline": style.underline,
            "color": style.color.map(encode_color),
            "string": text,
        }),
    }
}

fn encode_color(color: Color) -> String {
    let name = match color.base { BaseColor::Red => "red", BaseColor::Magenta => "magenta", BaseColor::Yellow => "yellow", BaseColor::Green => "green", BaseColor::Cyan => "cyan", BaseColor::Blue => "blue", BaseColor::Black => "black", BaseColor::White => "white" };
    if color.vivid { name.to_uppercase() } else { name.to_string() }
}

fn encode_region(region: Region) -> serde_json::Value {
    serde_json::json!({
        "start": { "line": region.start.line, "column": region.start.column },
        "end": { "line": region.end.line, "column": region.end.column },
    })
}

/// Elm's top-level `--report=json` object for a failed build.
pub fn compile_errors(modules: &[ModuleReports]) -> serde_json::Value {
    serde_json::json!({ "type": "compile-errors", "errors": modules.iter().map(module_to_json).collect::<Vec<_>>() })
}
```

**Elm reference** — `Reporting/Error.hs` (all), `Reporting/Doc.hs`
`encode`..`encodeColor`, `Render/Code.hs` `render`..`renderPair`.

**Tests** — `json_type_mismatch` (snapshot of pretty-printed JSON for
example 1), `json_name_clash_pair_snippet`, `snippet_doc_single_line_caret`
(Elm's `^^^^` underline), `snippet_doc_multi_line_arrows` (Elm's `>`
gutter), `json_no_snippet_report`.

**Done when** the JSON for example 1 has the exact key set Elm emits.

---

## Chunk 13 — Kind and trait errors (after plans 02, 03)

**Files**

- `crates/nash-report/src/type_.rs` (new arms; the trait solver errors
  are variants of `nash_constrain::Error`, so they flow through
  `ModuleError::Types` — no new `ModuleError` variant)
- `crates/nash-report/src/canonicalize.rs` (new arms for the kind errors
  plan 02 adds and the trait/impl declaration errors plan 03 adds to
  `nash_can::Error`; both flow through `ModuleError::Names`)
- `crates/nash-report/src/kind.rs` (new; closed `Kind` and `KindContext`
  rendering helpers used by `canonicalize.rs`)

**Change**

No Elm source. Titles and prose per `docs/diagnostics.md`. The real
variants:

- `nash_can::Error`: `KindMismatch { region, context, expected, actual }`
  contains closed Haskell 98 `Kind` values (`Type` and arrows).
  `KindInfinite { region, context }` reports an occurs-check failure.
  `BadArity` reports excess arguments to a named constructor.
  `KindContext` describes a type annotation, Big/little constructor field,
  record field, alias casing, named value annotation, or impl head. Impl
  heads are checked against their trait's closed parameter kinds.
  Representation failures use `RepresentationMismatch { region, context,
  required, actual }` and `ContradictoryRepresentation`; growing recursive
  contexts use `IrregularRecursion`. Keep these separate from kind mismatch.
  Use the current enums in `nash-can/src/error.rs` and diagnostic prose in
  `nash-driver/src/diagnostics.rs`; do not restore obsolete kind-bound or
  kind-application diagnostic variants.
- `nash_constrain::Error` (plan 03 chunk 4): `MissingImpl { region, name,
  trait_, args, available }`, `MissingConstraint { region, name, trait_,
  args, binder }`, `AmbiguousType { region, binder, var, preds }`,
  `PolymorphicRecursion { region, binder, trait_, args }`. Titles:
  `MISSING IMPL`, `MISSING CONSTRAINT`, `AMBIGUOUS TYPE`, `POLYMORPHIC
  RECURSION`.
- `nash_can::Error` (plan 03 chunks 2–3): `DuplicateTrait`,
  `DuplicateMethod`, `NotFoundTrait`, `AmbiguousTrait`, `TraitArity`,
  `ContextVarNotInType`, `MethodMissingParameter`, `RecursiveSuperclass`,
  `ExportOpenTrait`, `DuplicateTraitParameter`, `SuperclassBadArg`,
  `BadInstanceHead`, `OrphanImpl`, `OverlappingImpls`, `MissingSuperclass`,
  `MissingMethod`, `UnknownMethod`, `ImplContextVarNotInHead`,
  `RepeatedHeadVar`. `Duplicate*` and `Overlapping*` go through
  `name_clash`; `NotFoundTrait`/`AmbiguousTrait` through
  `not_found`/`ambiguous_name` with `thing = "trait"`; the rest get their
  own titles (`ORPHAN IMPL`, `BAD IMPL HEAD`, `MISSING SUPERCLASS`,
  `MISSING METHOD`, `UNKNOWN METHOD`, `TRAIT PROBLEM`).

Exemplar `MISSING IMPL` (example 2 in `docs/diagnostics.md`), an arm of
`type_::to_report`:

```rust
Error::MissingImpl { region, name, trait_, args, available } => {
    let args_doc = Doc::hsep(args.iter().map(|t| type_diff::to_doc(localizer, Ctx::App, t)));
    let head = args.iter().map(|t| type_diff::to_doc(localizer, Ctx::App, t).render(WIDTH, false)).collect::<Vec<_>>().join(" ");
    Report::snippet(
        "MISSING IMPL",
        *region,
        None,
        Doc::reflow(&format!("I cannot find an `{}` impl for `{head}`:", trait_.name)),
        Doc::stack(
            [
                Doc::reflow(&format!("`{name}` needs its arguments to implement `{}`, and here they are:", trait_.name)),
                Doc::indent(4, args_doc),
                Doc::reflow(&format!(
                    "But there is no `impl {} {head}` in this module or in any import.",
                    trait_.name
                )),
            ]
            .into_iter()
            .chain(available_hint(localizer, trait_, available))
            .chain([derive_hint(trait_, args)]),
        ),
    )
}
```

`name` is the overloaded name or literal that wanted the predicate
(`==` for example 2); `available_hint` lists up to four heads that do have
impls ("`Eq` is implemented for `int`, `bytes`, `option 'a`, ..."), and
`derive_hint` suggests `@derive(Trait)` when the trait is derivable
(`Eq`, `Ord`, `Show`, `ToData`, `FromData`) and the head is a local ADT,
otherwise an `impl` skeleton.

**Tests** — `missing_impl_eq_on_little_adt` (example 2),
`missing_impl_from_string_literal` (`add "one" 2`), `missing_constraint`,
`ambiguous_literal`, `polymorphic_recursion`, `orphan_impl`,
`overlapping_impls`, `missing_method`, `unknown_method`,
`missing_superclass`, `bad_instance_head`, `missing_storable_constraint_for_list_element`,
`kind_mismatch_type_arg`, `non_function_kind_application`.

**Done when** example 2's snapshot matches `docs/diagnostics.md`.

---

## Chunk 14 — Driver, CLI, LSP wiring

**Error collection is part of this chunk, not only rendering.** Audit the
producer paths before wiring their outputs. The current solver uses global
`state.errors.is_empty()` guards in predicate reporting, escape checking,
ambiguity/defaulting and scheme-kind checking. These can hide independent
errors. Replace them with recovery that tracks affected inference state and
dependencies; merely removing the guards or resetting an error counter is not
sound. Preserve existing poisoned-root suppression of dependent mismatches.
The current driver continues modules but needs explicit failed-dependency
handling; the CLI's HashMap iteration must not determine report order.

Collect all safely recoverable root errors, mark invalid dependents as blocked,
and render the complete ordered set after collection. A resource limit must
produce an explicit truncation or limit diagnostic. Do not publish an interface
or SolvedTypes from a failed solve.

**Required acceptance tests for collection and UX:**

- A module with independent ordinary mismatches, missing impls/constraints,
  ambiguity and kind errors reports every independent root error, including
  after reordering the declarations.
- Repeated uses of one poisoned expression do not create a cascade, while an
  unrelated sibling error still appears. Include shared inference variables
  and recursive definitions so recovery is not tested only on disjoint trees.
- Two independent failing modules both report. A dependent module is blocked
  by the original failure without fabricated missing-import/type errors.
- An impl-resolution cycle or work limit terminates the affected computation
  while preserving collected errors and unrelated diagnostics.
- Repeated CLI runs and shuffled module discovery have identical ordering:
  canonical module path, primary span, then stable diagnostic tie-breakers.
  Terminal snapshots, JSON and LSP assert the same problem set and spans.
- A failed parse or canonicalization does not enter later phases with invalid
  input. A failed solve exports neither an interface nor successful solved
  output. Independent modules continue.

Snapshot prose and labels for mixed errors, including expected/actual types,
trait context, original call sites and source highlights. Test valid partial
constructors separately from constructors incorrectly used as value types;
the latter diagnostic must explain the missing type arguments or kind mismatch.

**Files**

- `crates/nash-driver/src/compile.rs`
- `crates/nash-driver/Cargo.toml`
- `crates/nash-cli/src/cmd/check.rs`, `crates/nash-cli/src/main.rs`
- `crates/nash-language-server/src/server.rs`, `Cargo.toml`
- `.sampo/changesets/report-wiring.md`

**Change**

1. Driver: `ModuleResult::Failed { message: String }` becomes
   `Failed(nash_report::ModuleReports)`; `BuildResult.warnings` becomes
   `Vec<ModuleReports>` (one per module with warnings). In
   `compile_module` each phase's `Err` builds a `ModuleError`, calls
   `to_reports` while the arena is alive, and returns the owned
   `ModuleReports`:

   ```rust
   let source_view = nash_report::Source::new(src);
   let fail = |error: nash_report::ModuleError<'_>| {
       let reports = nash_report::to_reports(&source_view, expected_name, &error);
       ModuleResult::Failed(nash_report::ModuleReports {
           name: expected_name.to_string(),
           path: uri.path().to_string(),
           source: source.clone(),
           reports,
       })
   };

   let module = match parser.module() {
       Ok(module) => module,
       Err(e) => return finish(fail(ModuleError::Syntax(nash_parse::error::Error::ParseError(bump.alloc(e))))),
   };
   let localizer = nash_report::Localizer::from_module(&module, nash_can::imports::defaults());
   let can_result = match nash_can::canonicalize(&bump, context, &module) {
       Ok(r) => r,
       Err(errors) => return finish(fail(ModuleError::Names(errors))),
   };
   // Final signature in plans/03-traits.md chunk 5 "Solver API":
   // `run(bump, uf, constraint, tables: &Tables, fields: &FieldTable, mode: Mode)
   //   -> Result<(Annotations, SolvedTypes), Vec<Error>>`.
   let (annotations, solved) =
       match nash_solve::run(&bump, &mut uf, &constraint, &can_result.tables, &can_result.fields, mode) {
           Ok(a) => a,
       Err(errors) => return finish(fail(ModuleError::Types(localizer, errors))),
   };
   if let Err(errors) = nash_nitpick::check(&bump, &can_result.module) {
       return finish(fail(ModuleError::Patterns(errors)));
   }
   ```

   Warnings: `can_result.warnings.iter().map(nash_report::warning::to_report)`
   into a `ModuleReports` with `Severity::Warning` reports.

2. CLI (`check.rs`): install `nash_report::handler(color)` via
   `miette::set_hook` in `main.rs` before anything prints; in `check`,
   for every failed module print each `Rendered` with
   `eprintln!("{:?}", miette::Report::new(rendered))`; with
   `--report=json` print `nash_report::json::compile_errors(&failed)`
   instead. Add `--report <human|json>` and `--no-warnings` flags. Exit
   code 1 on errors, 0 on warnings only.

3. LSP: `nash-language-server` depends on `nash-driver` and
   `nash-report`; on `did_open`/`did_change` run the driver on the
   workspace and publish per-module diagnostics using

   ```rust
   pub fn to_lsp(report: &nash_report::Report, source: &nash_report::Source<'_>) -> ls_types::Diagnostic {
       ls_types::Diagnostic {
           range: to_range(report.region),
           severity: Some(match report.severity { Severity::Error => DiagnosticSeverity::ERROR, Severity::Warning => DiagnosticSeverity::WARNING }),
           code: Some(NumberOrString::String(report.title.clone())),
           source: Some("nash".into()),
           message: format!("{}\n\n{}", report.before.render(80, false), report.after.render(80, false)),
           related_information: related(report, source, uri),
           data: (!report.suggestions.is_empty()).then(|| serde_json::json!(report.suggestions)),
           ..Default::default()
       }
   }

   fn to_range(region: Region) -> ls_types::Range {
       // 1-based line/column (bytes) -> 0-based line/character (UTF-16); ASCII sources make these equal.
   }
   ```

   `related` maps `Snippet::Pair`'s first label and `Snippet::Region`'s
   `highlight` to `DiagnosticRelatedInformation`.

**Elm reference** — `Reporting/Error.hs` `toDoc` (module separators are
not kept; miette prints one block per report), `elm-language-server`'s
JSON consumption for the field set.

**Tests**

- `crates/nash-driver/src/compile.rs`: `test_failed_module_has_reports`
  (module with a naming error → one report titled `NAMING ERROR`),
  `test_warnings_are_reports`.
- `crates/nash-cli`: an integration test running `nash check` on
  `tests/fixtures/type-mismatch/` capturing stderr with `NO_COLOR=1`,
  snapshot of the output; same for `--report=json`.
- `crates/nash-language-server`: `to_lsp_range_is_zero_based`,
  `to_lsp_pair_has_related_information`.

Changeset: `cargo/nash-report: minor`, `cargo/nash-driver: minor`,
`cargo/nash-cli: minor`, `cargo/nash-language-server: minor` — "Render
compiler errors with Elm's prose through miette; add `--report=json`."

**Done when** `nash check` on a project with a type error prints example
1's layout, `--report=json` prints the Elm-shaped document, and the LSP
publishes diagnostics with ranges that line up in an editor. All collection
and UX acceptance tests above must also pass; one-error rendering fixtures do
not establish completion.

---

## Open questions

1. **`Report::with_region`.** Elm's `Code.toSnippet source surroundings
   (Just region)` pattern (wide snippet, narrow highlight, report region =
   the point) appears in most syntax reports. The API in chunk 1 grows a
   `with_region(wide)` builder in chunk 5; consider adding it in chunk 1
   directly.
2. **Prelude imports.** `Localizer::from_module` needs the same default
   import list canonicalization prepends (`docs/stdlib.md`, "Default
   imports"). The function name `nash_can::imports::defaults()` mirrors
   Elm's `Imports.defaults`; rename if plan 04 chunk C1 picks another.
3. **Stdlib function names in hints** (`Int.toString`,
   `Option.withDefault`, `Int.rem`/`Int.mod`) are placeholders until
   `docs/stdlib.md` fixes them.
