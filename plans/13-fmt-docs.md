# Plan 13: `nash-fmt` and `nash-docs`

Goal: `nash format` (alias `fmt`), a layout-preserving pretty printer over
the surface AST including comments, and `nash docs` (static HTML and Markdown from
`{-| -}` doc comments plus solved interfaces).

Prerequisites: the surface AST and parser from plans/01, including traits,
tests, attributes, `do`, and macro calls. Documentation extraction also needs
the driver's interface map.

Crates touched: `nash-source`, `nash-parse`, new `nash-fmt`, new
`nash-docs`, `nash-cli`, and `nash-report`.

References:

- Comments: `nash-parse/src/space.rs` retains ordinary comments in a
  source-ordered `Module.comments` side table. Doc comments retain their
  region and source snippet, attach to declarations, and populate `Docs::YesDocs`
  when an explicit module header is followed by an overview.
- Elm: `Parse/Module.hs` `chompModuleDocCommentSpace`, `Elm/Docs.hs`
  (`Module`, `Union`, `Alias`, `Value`, `Binop`, `fromModule`, the
  `@docs` overview parser), `Elm/Compiler/Type/Extract.hs` (types for
  docs), `Reporting/Doc.hs` (the `Doc` combinators `nash-report` ports).
- elm-format, conceptually: parse to an AST that keeps comments and
  "was this multiline" flags, print with a Wadler-style document, decide
  single-line versus multi-line per node from the source's own choice, and
  normalize blank lines. No code is shared.

---

## Chunk 1: comments in the surface AST — complete

Implemented in `nash-source`, `nash-parse`, and the canonicalizer's synthetic
value initializer.

- `Module.comments` retains ordinary line and block comments in source order.
  Each `SourceComment` stores its kind, region, and exact inner text.
- Line-comment regions include `--` and exclude LF/CRLF; block-comment regions
  include both delimiters. Nested blocks remain part of the outer comment text.
- `Comment` stores the doc-comment region and its existing source `Snippet`.
  Values, unions, aliases, traits, and implementations retain attached docs.
- A doc comment after an explicit module header is the module overview.
  `Docs::YesDocs` indexes named declaration docs in source order. Headerless
  declaration docs stay on their declaration; implementation docs stay on the
  implementation because implementations have no unique declaration name.
- Parser save/restore includes the ordinary-comment count. Restoring a failed
  alternative truncates the collection, so lookahead does not duplicate comments.
- Internal `Decl` variants refer to the documented surface nodes directly;
  docs are no longer temporarily stored on wrappers and discarded during
  categorization.

Snapshot coverage includes leading/inline/trailing comments, nested blocks,
Unicode, CRLF and EOF, doc attachments (including attributes), module overviews,
comments in tests, comment markers in strings, and section/do backtracking.
Existing declaration/module snapshots now include the preserved metadata.
Validation: 462 parser tests pass; full workspace tests, strict all-targets/
all-features Clippy, and formatting checks pass.

Chunk 1 does not format source or attach ordinary comments to individual nodes;
those tasks belong to later chunks.

---

## Chunk 2: `nash-fmt` document layer — complete

Implemented in `crates/nash-fmt/src/doc.rs` with groups, four-space nesting,
soft/hard lines, and deferred line-comment suffixes. Groups stay flat when
they fit the 80-column target. Literal and comment contents are preserved.
The public entry point is `nash_fmt::format(&str)`; printer internals are private.

Unit tests cover flat/broken groups and nesting. Shared formatter snapshots
exercise the document layer through real Nash syntax.

---

## Chunk 3: expressions, patterns, types — complete

Implemented in `expr.rs` and `types.rs`. Every currently parsed source AST
variant has an explicit printer, including representation qualifiers,
operator sections, pair patterns, labeled records, attributes, macro calls,
and tests. Future quote/splice syntax awaits its own parser support.

- Layout-sensitive bodies use hard lines. Existing multiline collections and
  applications stay multiline; other groups break when they exceed the target.
- Broken collections use leading commas; pipes lead continuation lines.
- Parentheses preserve operator grouping, nested application shape, and
  positional record arguments. Record field values retain the term grouping
  required by the grammar.
- Original string, bytes, and number spelling comes from the source region.

The shared `assert_format_snapshot!` helper stores Nash input in the snapshot
metadata and formatted Nash in the body. Every test reparses the output,
compares the source trees with coordinates removed, and checks idempotence.
All 35 shipped Base modules undergo the same structural and idempotence checks.
The formatter crate currently has 34 unit tests, including report snapshots.

---

## Chunk 4: declarations, module header, comments — complete

Implemented in `declarations.rs` and `printer.rs`.

Headers, imports, exposing entries, infix declarations, traits, implementations,
and tests retain source order. Formatting does not sort or deduplicate imports
or inject Base/Prelude code. Declarations have two blank lines between them;
local definitions and methods have one.

The source-ordered comment cursor emits leading comments before the next node,
retains identifiable trailing comments as line suffixes, and keeps closing
collection comments inside their container. Documentation stays attached to
its declaration. The structural comparison includes exact comment/doc contents.

Formatter fixtures also exposed two parser whitespace defects: a doc comment
after a constructor could be mistaken for labeled fields, and a `fail`/`todo`
message did not consume following whitespace. Those parser paths are corrected. Explicit continuation tokens and collection
closers can also align with a `do` statement, while ordinary application
continuation remains strictly indented.

---

## Chunk 5: `nash format` command (alias `fmt`) — complete

Implemented in `crates/nash-cli/src/cmd/format.rs` and `cmd/mod.rs`.

- `nash format [PATH...]` formats files and recursively visits directories.
- `--check` writes nothing, is silent when clean, and exits 1 for differences.
- `--stdin` formats one module from stdin to stdout.
- Parse failures use Nash diagnostics and leave the affected file unchanged.
- `similar::TextDiff` computes contextual changes; `nash-report::format` owns
  gutters, file headings, colors, and integration with the existing miette
  handler. Whitespace-only edits and missing final newlines are visible.

Formatter, parse-error, and diff snapshots live only in `nash-fmt` unit tests.
No formatter CLI integration/subprocess tests are added. The formatter does not
rewrite the repository's Base sources as a side effect of this implementation.
See [formatter behavior](../docs/formatter.md) for command and layout details.

---

## Chunk 6: `nash-docs` extraction — complete

`nash-docs::extract` combines a parsed source module and its solved
`nash_can::Interface` into owned `ModuleDocs` plus documentation warnings.
It uses the existing report type printer, retains explicit trait constraints,
and includes public values, types, aliases, operators, traits and implementations.
Private declarations and hidden constructors are omitted. Type kinds remain in
the output. Unnamed implementations do not require separate documentation.

Overview prose and `@docs` groups retain their order. Missing comments,
unknown names and duplicate directives produce warnings without discarding
output. Undirected declarations follow in source order. Source-backed macro
declarations are not implemented by the parser/interface yet, so no synthetic
macro entries are invented.

The actual compiler catalogs supply separate `Builtin` and `Primitive` modules;
`coerce` belongs to Primitive. Public Base declarations now have source doc
comments. A library test compiles all 35 bundled modules and checks that
extraction produces no documentation warnings. Source-described snapshots cover
all supported declaration kinds, inferred/constrained signatures, ordering,
visibility, warning recovery and synthetic catalogs. No CLI processes are used.

Base has no separate project manifest. Chunk 7 provides `nash docs --base`
using the compiler-bundled sources, alongside normal project documentation.

---

## Chunk 7: rendering and `nash docs` command

**Files**

- `crates/nash-docs/src/html.rs`, `src/markdown.rs`, `src/assets/` (one CSS file, one JS file for search, embedded with `include_str!`)
- `crates/nash-cli/src/cmd/docs.rs`

**Change**

`nash docs [--format html|markdown] [--out docs/]` renders every exposed
module of the package (application projects render all modules) to
`out/<Module/Name>.html` plus an index and a JSON search index, or to
one Markdown file per module. Markdown in doc comments is rendered with
`pulldown-cmark`; code blocks tagged `nash` are syntax-highlighted by a
small token classifier over `nash-parse`'s lexer functions (keywords,
operators, strings, numbers) — no external highlighter.

```rust
#[derive(clap::Args)]
pub struct Args {
    #[arg(long, default_value = "html")]
    pub format: Format,
    #[arg(long, default_value = "docs")]
    pub out: PathBuf,
}
```

**Tests**

- Markdown snapshot for one module.
- HTML: snapshot with the CSS stripped; a smoke test that the index links
  to every module.

**Done when** `nash docs` on `crates/nash-driver/base/` produces a browsable site and CI
publishes it for the repo.

---

## Order

```
1 comments ─ 2 doc ─ 3 exprs ─ 4 decls+comments ─ 5 nash format
1 comments ─ 6 extract ─ 7 render + nash docs
```

Chunk 6 depends only on chunk 1 and can run in parallel with 2–5.
Future macro diagnostics can use `nash-fmt` once they emit surface source.
