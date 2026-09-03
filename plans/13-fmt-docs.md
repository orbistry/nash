# Plan 13: `nash-fmt` and `nash-docs`

Goal: `nash fmt` (a layout-preserving pretty printer over the surface AST,
comments included) and `nash docs` (static HTML and Markdown from
`{-| -}` doc comments plus solved interfaces).

Prerequisites: plans/01 (syntax: traits, tests block, attributes, `do`,
macros) so the printer covers the whole surface AST; plans/11 chunk 12's
`nash_source::print` is the non-preserving seed this plan replaces; the
driver's interface map for `nash docs`.

Crates touched: `nash-source`, `nash-parse`, new `nash-fmt`, new
`nash-docs`, `nash-cli`.

References:

- Comments today: the parser drops line and block comments in
  `eat_spaces` (`crates/nash-parse/src/space.rs:167`,
  `eat_line_comment` :212, `eat_multi_comment` :236). Doc comments are
  captured by `doc_comment` (:107) as `Comment(&Snippet { data, off_row, off_col })`
  (`crates/nash-source/src/lib.rs:257`), attached to `Decl::Value(Option<&Comment>, ..)`
  (`crates/nash-parse/src/declaration/mod.rs:19`), then discarded by
  `categorize_decls` (`crates/nash-parse/src/module.rs:258`). Module
  docs are always `Docs::NoDocs` (`module.rs:241`).
- Elm: `Parse/Module.hs` `chompModuleDocCommentSpace`, `Elm/Docs.hs`
  (`Module`, `Union`, `Alias`, `Value`, `Binop`, `fromModule`, the
  `@docs` overview parser), `Elm/Compiler/Type/Extract.hs` (types for
  docs), `Reporting/Doc.hs` (the `Doc` combinators `nash-report` ports).
- elm-format, conceptually: parse to an AST that keeps comments and
  "was this multiline" flags, print with a Wadler-style document, decide
  single-line versus multi-line per node from the source's own choice, and
  normalize blank lines. No code is shared.

---

## Chunk 1: comments in the surface AST

**Files**

- `crates/nash-source/src/lib.rs`
- `crates/nash-parse/src/space.rs`, `lib.rs`, `module.rs`, `declaration/mod.rs`

**Change**

Keep every comment with its region in a side table on `Module`, and keep
doc comments on declarations and on the module.

**Code**

```rust
// crates/nash-source/src/lib.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    Line,       // -- ...
    Block,      // {- ... -}
}

#[derive(Debug)]
pub struct SourceComment<'a> {
    pub region: Region,
    pub kind: CommentKind,
    /// Text between the delimiters, untrimmed.
    pub text: &'a str,
}

pub struct Module<'a> {
    // ...
    /// All non-doc comments in source order.
    pub comments: &'a [&'a SourceComment<'a>],
    pub docs: &'a Docs<'a>,                  // now filled: module overview + per-decl docs
}

pub struct Value<'a> { /* ... */ pub docs: Option<&'a Comment<'a>> }
pub struct Union<'a> { /* ... */ pub docs: Option<&'a Comment<'a>> }
pub struct Alias<'a> { /* ... */ pub docs: Option<&'a Comment<'a>> }
```

`Parser` gains `comments: Vec<&'a SourceComment<'a>>`. `eat_line_comment`
and `eat_multi_comment` record start position before advancing and push
after:

```rust
fn eat_line_comment(&mut self) {
    let start = self.get_position();
    let text_start = self.pos + 2;
    self.advance(); self.advance();
    let mut text_end = self.pos;
    loop {
        match self.peek() {
            Some(0x0A) => { self.advance(); break; }
            Some(_) => { self.advance(); text_end = self.pos; }
            None => break,
        }
    }
    self.push_comment(start, CommentKind::Line, text_start, text_end);
}

fn push_comment(&mut self, start: Position, kind: CommentKind, text_start: usize, text_end: usize) {
    let text = std::str::from_utf8(&self.src[text_start..text_end]).expect("source is UTF-8");
    let comment = self.alloc(SourceComment { region: Region::new(start, self.get_position()), kind, text });
    self.comments.push(comment);
}
```

`module()` moves `self.comments` into `Module.comments` at the end and
parses a module doc comment after the header (Elm's
`chompModuleDocCommentSpace`). `categorize_decls` stores the `Decl`'s
doc on the value/union/alias instead of dropping it.

Backtracking: `save_state`/`restore_state` (`module.rs:208`) must
truncate `comments` to the saved length so a failed alternative does not
leave duplicates.

**Elm/Aiken reference**

`Parse/Space.hs` `eatLineComment`, `eatMultiComment` (structure);
`Parse/Module.hs` `chompModuleDocCommentSpace`. Aiken keeps comments the
same way: `crates/aiken-lang/src/parser/token.rs` `Token::Comment` plus
`extra.comments` spans in `crates/aiken-lang/src/parser/extra.rs`.

**Tests** (`crates/nash-parse/src/space.rs`, `module.rs`)

- `comments_are_collected`: `-- a\n{- b -}\nx = 1` yields two `SourceComment`s with regions and texts `" a"`, `" b "`.
- `doc_comment_attaches_to_value`: `{-| doc -}\nx = 1` → `values[0].docs.is_some()`.
- `module_doc_comment`: `module M exposing (..)\n{-| overview -}\nx = 1` → `Docs::YesDocs`.
- `backtracking_does_not_duplicate_comments`.

**Done when** existing parser snapshots are unchanged except for the new
fields.

---

## Chunk 2: `nash-fmt` document layer

**Files**

- `crates/nash-fmt/Cargo.toml`, `src/lib.rs`, `src/doc.rs`

**Change**

A small Wadler/Leijen `Doc` with `group`, `nest`, `line`, `softline`,
`hardline`, `text`, rendered at width 80 with 4-space indentation. Nash
is layout-sensitive, so the renderer never joins lines that the layout
rules need separate: `let`, `case`, `if` bodies, and `do` blocks are
always broken (`hardline`), matching elm-format.

**Code**

```rust
pub enum Doc<'a> {
    Nil,
    Text(&'a str),
    Line,               // space when flat, newline when broken
    SoftLine,           // nothing when flat, newline when broken
    HardLine,           // always newline
    Nest(u16, &'a Doc<'a>),
    Group(&'a Doc<'a>),
    Concat(&'a [&'a Doc<'a>]),
}

pub struct Printer<'a> { bump: &'a Bump }

impl<'a> Printer<'a> {
    pub fn render(&self, doc: &'a Doc<'a>, width: usize) -> String
}
```

Rendering is the standard "fits" algorithm with a work stack; no
backtracking beyond one group.

**Tests**

`group(text a, line, text b)` flat at width 80, broken at width 3;
nesting under a broken group indents by 4.

**Done when** the doc tests pass.

---

## Chunk 3: expressions, patterns, types

**Files**

- `crates/nash-fmt/src/expr.rs`, `pattern.rs`, `typ.rs`, `layout.rs`

**Change**

Print every `Expr`, `Pattern`, and `Type` variant. Layout-preservation
rule: a node whose source region spans more than one line is printed
broken; a single-line node is printed as a `group` (flat if it fits).
That reproduces elm-format's "you chose multiline, we keep it" behaviour
without an extra AST flag: `Located.region` already says.

```rust
// crates/nash-fmt/src/layout.rs
pub fn is_multiline(region: Region) -> bool {
    region.start.line != region.end.line
}
```

Rules worth spelling out:

- Calls: `f a b`; if multiline, arguments each on their own line indented.
- Binary operator chains (`BinOps`): one operand per line when broken,
  operator leading (`|> f` style for `|>`, `<|`; operator trailing for
  arithmetic), following elm-format.
- `if`/`case`/`let`: always broken, `let` definitions separated by a
  blank line if the source had one (checked via comment/region gaps).
- Lists, records, tuples: `[ a, b ]` flat; broken form puts `, ` at line
  starts (elm-format style).
- Lambdas: `\x y -> body`; body broken on its own line if multiline.
- Strings: verbatim, including multi-line `"""`.
- Attributes, `MacroCall`, `Quote`, `Splice`, `Comptime`, `Do`: printed
  in their surface form; `do` blocks always broken.
- `VarGlobal` never appears in user source; `unreachable!`.

**Elm/Aiken reference**

elm-format's `ElmFormat/Render/Box.hs` `formatExpression` (rules only).
Aiken `crates/aiken-lang/src/format.rs` `Formatter::expr` (Rust code
with the same structure, useful for the operator-chain and `when`
layouts).

**Tests** (`crates/nash-fmt/src/snapshots`)

`assert_fmt_snapshot!(input)` snapshots the output; `assert_fmt_idempotent!(input)`
asserts `fmt(fmt(x)) == fmt(x)`. Inputs: every expression form in
`crates/nash-parse/src/expression/*` tests, flat and multiline, plus the
operator chains above.

**Done when** every parser expression snapshot input round-trips
idempotently.

---

## Chunk 4: declarations, module header, comment attachment

**Files**

- `crates/nash-fmt/src/module.rs`, `decl.rs`, `comments.rs`

**Change**

Print the module header (`module`/`validator module`, exposing list one
per line if multiline, sorted never), imports (sorted by module name,
deduplicated exposing lists), infix declarations, declarations in source
order, traits, impls, macros, and the `tests` block. Two blank lines
between top-level declarations, one inside `let`/`where`.

Comment attachment: comments are a side table with regions. The printer
walks declarations in order and, before printing a node, emits any
comments whose region ends before the node's start and after the
previous node's end ("leading" comments); comments on the same line
after a node's end are "trailing" and printed after it with one space.
Comments inside an expression are attached to the innermost enclosing
`Located` whose region contains them, by the same leading/trailing rule.
A comment the rules cannot place (inside a token gap with no enclosing
node, e.g. between `if` and its condition) is printed as a leading
comment of the node that follows it; nothing is ever dropped, which the
idempotency tests check by counting comments before and after.

**Code**

```rust
pub struct Comments<'a> {
    all: &'a [&'a SourceComment<'a>],
    next: usize,
}

impl<'a> Comments<'a> {
    /// Comments strictly before `pos` that have not been emitted yet.
    pub fn leading(&mut self, pos: Position) -> &'a [&'a SourceComment<'a>]
    /// Comments starting on `line` after `pos`.
    pub fn trailing(&mut self, pos: Position) -> Option<&'a SourceComment<'a>>
}
```

**Elm/Aiken reference**

elm-format `ElmFormat/Render/Box.hs` `formatModule`, `formatComment`;
Aiken `format.rs` `Formatter::definitions` and `pop_doc_comments` for
the side-table approach.

**Tests**

- Snapshot every declaration form with leading, trailing, and inner
  comments.
- `comments_preserved`: for each input, count of `--`/`{-` in output
  equals count in input.
- Idempotency over the whole `core/` tree once it exists.

**Done when** `nash fmt --check core/` reports no changes after one
`nash fmt core/`.

---

## Chunk 5: `nash fmt` command

**Files**

- `crates/nash-cli/src/cmd/fmt.rs`, `cmd/mod.rs`

**Change**

`nash fmt [paths...]` formats in place; `--check` exits 1 and lists
files that would change; `--stdin` reads one module from stdin and
writes to stdout (for editors). Parse errors are reported through
`nash-report` and the file is left untouched.

```rust
#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,
    #[arg(long)]
    pub check: bool,
    #[arg(long)]
    pub stdin: bool,
}
```

**Tests**

CLI integration test with a temp dir: `--check` exit codes, in-place
rewrite, `--stdin` round trip.

**Done when** CI runs `nash fmt --check` on `core/` and the repo's
examples.

---

## Chunk 6: `nash-docs` extraction

**Files**

- `crates/nash-docs/Cargo.toml`, `src/lib.rs`, `src/extract.rs`

**Change**

Port `Elm/Docs.hs`: from a module's surface docs (chunk 1) and its solved
`Interface`, produce a `Docs` value: overview text, `@docs` ordering, and
one entry per exported value, union, alias, binop, trait, impl, macro,
each with its doc comment and rendered type. Undocumented exports and
`@docs` names that do not exist are warnings, not errors (Elm errors;
Nash warns so `nash docs` always produces output).

```rust
pub struct ModuleDocs {
    pub name: String,
    pub overview: String,          // Markdown
    pub blocks: Vec<Block>,        // in @docs order, then leftovers
}

pub enum Block {
    Text(String),
    Value { name: String, typ: String, doc: String },
    Union { name: String, params: Vec<String>, ctors: Vec<(String, Vec<String>)>, kind: String, doc: String },
    Alias { name: String, params: Vec<String>, typ: String, doc: String },
    Binop { symbol: String, function: String, precedence: u16, assoc: String, doc: String },
    Trait { name: String, params: Vec<String>, supers: Vec<String>, methods: Vec<(String, String)>, doc: String },
    Impl { head: String, doc: String },
    Macro { name: String, shape: String, doc: String },
    Builtin { name: String, typ: String },   // for the synthetic Builtin module, from nash_ast::primitives::PRIMITIVES (types, plans/02 chunk 3) and BUILTINS (functions, plans/12 chunk 3)
}

pub fn extract(module: &SourceModule<'_>, interface: &Interface<'_>) -> Result<ModuleDocs, Vec<DocsWarning>>
```

Type rendering reuses `nash-report`'s type pretty printer (Elm's
`Reporting/Render/Type.hs`, already ported for diagnostics).

**Elm/Aiken reference**

`Elm/Docs.hs` `fromModule`, `parseOverview`, `checkNames`;
`Elm/Compiler/Type/Extract.hs` `fromType`. Aiken
`crates/aiken-project/src/docs.rs` `generate_all` (module listing, search
index).

**Tests**

Snapshot `ModuleDocs` for a module with `@docs`, one of each block kind,
and a missing name (warning present, docs still produced).

**Done when** `extract` runs over `core/` without warnings.

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

**Done when** `nash docs` on `core/` produces a browsable site and CI
publishes it for the repo.

---

## Order

```
1 comments ─ 2 doc ─ 3 exprs ─ 4 decls+comments ─ 5 nash fmt
1 comments ─ 6 extract ─ 7 render + nash docs
```

Chunk 6 depends only on chunk 1 and can run in parallel with 2–5.
Plans/11 chunk 11 (macro diagnostics) switches its generated-code
excerpts from `nash_source::print` to `nash-fmt` after chunk 4.
