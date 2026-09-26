# Formatter

`nash format [PATH...]` (`nash fmt`) formats Nash files in place. Directories
are visited recursively; the default is the current directory. The command
skips `.git`, `.jj`, `target`, `build`, and `node_modules`, and does not follow
symlinks discovered during directory traversal. Explicit file symlinks resolve
to their targets. It formats source without loading a project, imports, or Base.
Filesystem and stream I/O is asynchronous. Files are processed concurrently,
with parsing, formatting, and diff construction on blocking worker threads.
Diagnostics appear as files finish; their order is unspecified. Overlapping
input paths are deduplicated before processing.

`--check` leaves files untouched and exits 1 when formatting differs or a file
cannot be parsed. A clean check is silent. Changed files have a contextual
red/green diff rendered through `nash-report` and the existing miette handler.
Diffs show old and new line numbers, visible trailing spaces/tabs, CRLF markers,
and missing final newlines. Global `--color` controls this output.

`--stdin` formats one module from stdin to stdout. It cannot be combined with
paths or `--check`. Parse errors use normal Nash diagnostics. Files with parse
errors remain unchanged; other selected files can still be formatted.

## Layout

The printer targets 80 columns with four-space indentation. Literal text and
comments can exceed the target; formatting never changes their contents.
Strings, byte literals, and numeric literals retain their source spelling.
Output uses LF layout and one final newline for a nonempty module.

The layout follows elm-format's collection and pipeline conventions and
Fourmolu's compact definitions and hanging `do` blocks. Nash syntax and its
layout-sensitive parser determine where those conventions apply.

- Short definitions remain on one line, including definitions with annotations.
  A `do` block follows `=` or `<-` on the same line; its statements indent once.
  A comment before the block keeps the block below that comment.
- `if`, `case`, `let`, and `do` use multiline bodies. Explicit continuations
  (`then`, `else`, `in`, and collection closers) may align with a `do`
  statement. Adjacent statements remain separate expressions.
- Collections and applications retain an existing multiline layout; otherwise
  they stay on one line when they fit. Broken collections use leading commas.
- Pipes begin continuation lines and retain an existing multiline layout.
  Other infix expressions stay on one line when they fit; when they wrap,
  their operators end the preceding lines. Parentheses retain precedence.
- Top-level declarations have two blank lines between them. Local declarations
  and trait/implementation methods have one.
- Imports, exposing entries, declarations, and fields retain source order.
  The formatter does not sort, deduplicate, inject imports, or expand macros.
- Parentheses preserve expression grouping and the distinction between labeled
  constructor arguments and positional record arguments.
- Ordinary comments retain their text and order. End-of-line comments remain
  line suffixes where an AST boundary identifies their owner. Other comments
  precede the next source node; comments before collection closers stay inside.
  Doc comments remain attached to their declarations.

## Library and tests

`nash-fmt::format(&str)` returns formatted source or an owned `nash-report`
diagnostic. Its document layer and printers are private to the crate.

Formatter unit tests use a shared snapshot macro: the snapshot description is
input Nash source and the body is formatted Nash source. The same helper
reparses the output, compares the complete source AST with source coordinates
removed, and checks idempotence. All shipped Base modules also undergo these
round-trip checks. Diff and parse-error snapshots live in `nash-fmt` as well.
There are no CLI subprocess tests for formatting.
