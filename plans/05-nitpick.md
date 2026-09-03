# Plan 05 — `nash-nitpick`: exhaustiveness and redundancy checking

## Goal

Port Elm's `Nitpick/PatternMatches.hs` (Maranget, *Warnings for Pattern
Matching*) into a new crate `nash-nitpick`. It runs once per module after
type solving, walks the canonical AST, and reports:

- `Incomplete` — a `case`, a function argument, or a `let` destructure does
  not cover every value of its type, with concrete example patterns that are
  missing.
- `Redundant` — a `case` branch can never match because earlier branches
  already cover it.

Nash additions over Elm: `bytes` literal patterns, `Data` constructor
patterns (`Constr tag fields | Map kvs | List xs | I n | B bs`), and Big /
little ADTs (identical for this check — representation never changes the
set of constructors).

## Prerequisites

- Plans 01–04 (syntax, kinds, traits, representation) so test inputs type
  check. The algorithm itself only needs `nash-ast` as it is today; chunks
  1–3 compile against the current crates.
- `nash-ast` gains `Pattern::Bytes(&'a [u8])` in plan 01. Chunk 4 adds the
  arm for it.
- `Expr::Negate` is removed by plan 03 (`Num.negate` call) and
  `Expr::Record` becomes `Record { alias, annotation, fields }` in plan 04
  chunk A3; chunk 3's `expr` walk is written against those shapes. `bool`
  constructors are `False | True` (`docs/representation.md`).

## Crates touched

- new: `crates/nash-nitpick`
- `crates/nash-driver/src/compile.rs` (driver hook)
- `Cargo.toml` workspace member list (glob picks it up), `.sampo/changesets`

## Reference

- Elm: `elm/compiler/src/Nitpick/PatternMatches.hs` (all of it)
- Elm: `elm/compiler/src/Reporting/Error/Pattern.hs` (`patternToDoc`,
  `delist`, `Structure`) — the pattern printer lives here in nitpick so the
  report crate (plan 06) only supplies prose.
- Elm driver: `elm/compiler/src/Compile.hs` — `nitpick` runs after
  `typeCheck`, before `Nitpick.Main`.
- Nash canonical AST: `crates/nash-ast/src/lib.rs:229-262` (`Pattern`,
  `PatternCtor`, `PatternCtorArg`), `:80-104` (`Union`, `Ctor`, `CtorOpts`).

## Design notes

**Pattern language.** Elm's simplified pattern is a three-way sum:

```haskell
data Pattern = Anything | Literal Literal | Ctor Can.Union Name.Name [Pattern]
data Literal = Chr ES.String | Str ES.String | Int Int
```

Nash drops `Chr` and adds `Bytes`. Every structural pattern becomes a
`Ctor` over a *synthetic* union: unit (`#0`), pairs (`#2`), triples (`#3`),
and lists (`[]` / `::`). These unions are `static` values; nitpick never
reads their `arguments`, but they are filled in like Elm's so the value is
honest.

**`Data`.** `Data` is an ordinary five-constructor union in the canonical
environment, with little fields: `Constr int (list Data) | Map (list (pair
Data Data)) | List (list Data) | I int | B bytes`. `Constr tag fields` has
an `int` sub-pattern for the tag and a `list Data` sub-pattern for the
fields. Nothing special is needed: the tag column is a literal column, the
fields column is a list column, and Maranget's algorithm handles both.

**Big vs little.** A Big ADT is a `Data` `Constr` at runtime and a little
ADT is a UPLC `constr`. Exhaustiveness only looks at `Union.ctors` and
`Union.alternatives`, so the two are indistinguishable here. Likewise Big
`List` and little `list` both use the synthetic list union.

**Rows.** Elm's matrix is a list of persistent lists. Here a row is a
`Vec<Pattern<'a>>` (`Pattern` is `Copy`, pointing into the module arena)
and the matrix is `Vec<Row>`. Rows are scratch memory; only the missing
patterns reported in `Error::Incomplete` are bump-allocated so they outlive
the check.

**Row order.** Elm prepends accepted rows (`nextRow : checkedRows`); this
port appends. `isUseful` and `isExhaustive` are order-insensitive over the
matrix (`collectCtors` picks by name via a `BTreeMap`, mirroring `Map.findMin`).

**Error order.** Elm builds the error list with `foldr`, which yields
source order. The port pushes into a `Vec` while visiting in source order:
scrutinee, then the `case`'s own error, then branch bodies; function
arguments before bodies; destructure pattern before value before body.
Record and update fields are visited in source order rather than Elm's
field-name order.

---

## Chunk 1 — crate skeleton, pattern types, `simplify`, printer

**Files**

- `crates/nash-nitpick/Cargo.toml` (new)
- `crates/nash-nitpick/src/lib.rs` (new)
- `crates/nash-nitpick/src/pattern.rs` (new)
- `crates/nash-nitpick/src/render.rs` (new)
- `.sampo/changesets/nitpick-skeleton.md` (new)

**Change**

Create the crate with the simplified `Pattern` / `Literal` types, the
`Error` / `Context` types, the synthetic unions, `simplify`, and the
pattern printer from `Reporting/Error/Pattern.hs`.

**Code**

`Cargo.toml`:

```toml
[package]
name = "nash-nitpick"
version = "0.1.0"
edition.workspace = true
description = "Exhaustiveness and redundancy checking for Nash pattern matches"
homepage.workspace = true
repository.workspace = true
license.workspace = true

[dependencies]
bumpalo.workspace = true
hex.workspace = true
nash-ast = { path = "../nash-ast", version = "0.3.1" }
nash-region = { path = "../nash-region", version = "0.2.0" }

[dev-dependencies]
indoc.workspace = true
insta.workspace = true
nash-can = { path = "../nash-can", version = "0.3.1" }
nash-constrain = { path = "../nash-constrain", version = "0.2.1" }
nash-parse = { path = "../nash-parse", version = "0.2.2" }
nash-solve = { path = "../nash-solve", version = "0.2.1" }
```

`src/lib.rs`:

```rust
//! Exhaustiveness and redundancy checking for pattern matches, ported from
//! Elm's `Nitpick/PatternMatches.hs`. The algorithm is Maranget's
//! "Warnings for Pattern Matching" (http://moscova.inria.fr/~maranget/papers/warn/warn.pdf).
//!
//! Runs on the canonical AST after type solving, which guarantees that
//! every column of a pattern matrix holds patterns of a single type: a
//! column is all constructors of one union, all literals, or wildcards.
//! Big and little ADTs are identical here; only `Union::ctors` matters.

mod check;
mod matrix;
mod pattern;
pub mod render;

pub use check::check;
pub use pattern::{Context, Error, Literal, Pattern};
```

`src/pattern.rs`:

```rust
use bumpalo::Bump;
use nash_ast::{Ctor, CtorOpts, ModuleName, Pattern as CanPattern, PatternCtor, QualifiedName, Type, Union};
use nash_region::{Located, Region};

/// Elm's `Nitpick.PatternMatches.Pattern`.
#[derive(Clone, Copy, Debug)]
pub enum Pattern<'a> {
    Anything,
    Literal(Literal<'a>),
    Ctor {
        union: &'a Union<'a>,
        name: &'a str,
        args: &'a [Pattern<'a>],
    },
}

/// Elm's `Literal` without `Chr`, plus `Bytes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Literal<'a> {
    Int(i128),
    Str(&'a str),
    Bytes(&'a [u8]),
}

/// Elm's `Nitpick.PatternMatches.Error`.
#[derive(Debug)]
pub enum Error<'a> {
    Incomplete {
        region: Region,
        context: Context,
        unhandled: &'a [Pattern<'a>],
    },
    Redundant {
        case_region: Region,
        pattern_region: Region,
        /// 1-based position of the redundant branch.
        index: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    BadArg,
    BadDestruct,
    BadCase,
}

// BUILT-IN UNIONS

pub(crate) const UNIT_NAME: &str = "#0";
pub(crate) const PAIR_NAME: &str = "#2";
pub(crate) const TRIPLE_NAME: &str = "#3";
pub(crate) const NIL_NAME: &str = "[]";
pub(crate) const CONS_NAME: &str = "::";

static VAR_A: Located<Type<'static>> = Located::at(Region::zero(), Type::Var("a"));
static VAR_B: Located<Type<'static>> = Located::at(Region::zero(), Type::Var("b"));
static VAR_C: Located<Type<'static>> = Located::at(Region::zero(), Type::Var("c"));
static LIST_A: Located<Type<'static>> = Located::at(
    Region::zero(),
    Type::Named {
        reference: QualifiedName {
            home: ModuleName { package: None, name: "List" },
            name: "List",
        },
        args: &[&VAR_A],
    },
);

static UNIT_LOCATED: Located<&str> = Located::at(Region::zero(), UNIT_NAME);
static UNIT_CTOR: Ctor<'static> = Ctor { name: UNIT_NAME, index: 0, arity: 0, arguments: &[] };
pub(crate) static UNIT: Union<'static> = Union {
    name: &UNIT_LOCATED,
    parameters: &[],
    ctors: &[&UNIT_CTOR],
    alternatives: 1,
    options: CtorOpts::Normal,
};

static PAIR_LOCATED: Located<&str> = Located::at(Region::zero(), PAIR_NAME);
static PAIR_CTOR: Ctor<'static> = Ctor { name: PAIR_NAME, index: 0, arity: 2, arguments: &[&VAR_A, &VAR_B] };
pub(crate) static PAIR: Union<'static> = Union {
    name: &PAIR_LOCATED,
    parameters: &["a", "b"],
    ctors: &[&PAIR_CTOR],
    alternatives: 1,
    options: CtorOpts::Normal,
};

static TRIPLE_LOCATED: Located<&str> = Located::at(Region::zero(), TRIPLE_NAME);
static TRIPLE_CTOR: Ctor<'static> =
    Ctor { name: TRIPLE_NAME, index: 0, arity: 3, arguments: &[&VAR_A, &VAR_B, &VAR_C] };
pub(crate) static TRIPLE: Union<'static> = Union {
    name: &TRIPLE_LOCATED,
    parameters: &["a", "b", "c"],
    ctors: &[&TRIPLE_CTOR],
    alternatives: 1,
    options: CtorOpts::Normal,
};

static LIST_LOCATED: Located<&str> = Located::at(Region::zero(), "List");
static NIL_CTOR: Ctor<'static> = Ctor { name: NIL_NAME, index: 0, arity: 0, arguments: &[] };
static CONS_CTOR: Ctor<'static> = Ctor { name: CONS_NAME, index: 1, arity: 2, arguments: &[&VAR_A, &LIST_A] };
pub(crate) static LIST: Union<'static> = Union {
    name: &LIST_LOCATED,
    parameters: &["a"],
    ctors: &[&NIL_CTOR, &CONS_CTOR],
    alternatives: 2,
    options: CtorOpts::Normal,
};

const NIL: Pattern<'static> = Pattern::Ctor { union: &LIST, name: NIL_NAME, args: &[] };

// CREATE SIMPLIFIED PATTERNS

/// Elm's `simplify`.
pub(crate) fn simplify<'a>(bump: &'a Bump, pattern: &Located<CanPattern<'a>>) -> Pattern<'a> {
    match &pattern.value {
        CanPattern::Anything | CanPattern::Var(_) | CanPattern::Record(_) => Pattern::Anything,
        CanPattern::Unit => Pattern::Ctor { union: &UNIT, name: UNIT_NAME, args: &[] },
        CanPattern::Tuple { first, second, rest } => {
            // Canonicalization rejects tuples above three (`TupleLargerThanThree`),
            // so `rest` is empty or one element; the walk is generic anyway.
            let union: &'a Union<'a> = if rest.is_empty() { &PAIR } else { &TRIPLE };
            Pattern::Ctor {
                union,
                name: union.ctors[0].name,
                args: bump.alloc_slice_fill_with(2 + rest.len(), |i| {
                    simplify(bump, match i { 0 => first, 1 => second, i => rest[i - 2] })
                }),
            }
        }
        CanPattern::Constructor(PatternCtor { reference, union, arguments, .. }) => Pattern::Ctor {
            union,
            name: reference.name,
            args: bump.alloc_slice_fill_iter(arguments.iter().map(|arg| simplify(bump, arg.pattern))),
        },
        CanPattern::List(entries) => entries
            .iter()
            .rev()
            .fold(NIL, |tail, head| cons(bump, head, tail)),
        CanPattern::Cons { head, tail } => cons(bump, head, simplify(bump, tail)),
        CanPattern::Alias { pattern, .. } => simplify(bump, pattern),
        CanPattern::Int(n) => Pattern::Literal(Literal::Int(*n)),
        CanPattern::Str(s) => Pattern::Literal(Literal::Str(s)),
        CanPattern::Bool { union, value } => Pattern::Ctor {
            union,
            name: if *value { "True" } else { "False" },
            args: &[],
        },
    }
}

/// Elm's `cons`.
fn cons<'a>(bump: &'a Bump, head: &Located<CanPattern<'a>>, tail: Pattern<'a>) -> Pattern<'a> {
    Pattern::Ctor {
        union: &LIST,
        name: CONS_NAME,
        args: bump.alloc_slice_copy(&[simplify(bump, head), tail]),
    }
}
```

`src/render.rs` — port of `patternToDoc` / `delist` from
`Reporting/Error/Pattern.hs`. Plain text, no styling; plan 06 wraps the
block in `dullyellow` and indents it.

```rust
//! Text form of missing patterns, from Elm's `Reporting/Error/Pattern.hs`
//! (`patternToDoc`, `delist`).

use crate::pattern::{CONS_NAME, Literal, NIL_NAME, PAIR_NAME, Pattern, TRIPLE_NAME, UNIT_NAME};

/// Elm's `Context` in `Reporting.Error.Pattern`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderContext {
    Arg,
    Head,
    Unambiguous,
}

pub fn pattern_to_string(context: RenderContext, pattern: Pattern<'_>) -> String {
    match delist(pattern, Vec::new()) {
        Structure::NonList(Pattern::Anything) => "_".to_string(),
        Structure::NonList(Pattern::Literal(literal)) => literal_to_string(literal),
        Structure::NonList(Pattern::Ctor { name: UNIT_NAME, .. }) => "()".to_string(),
        Structure::NonList(Pattern::Ctor { name: PAIR_NAME | TRIPLE_NAME, args, .. }) => {
            format!("( {} )", join(args, RenderContext::Unambiguous, ", "))
        }
        Structure::NonList(Pattern::Ctor { name, args, .. }) => {
            let mut doc = name.to_string();
            for arg in args {
                doc.push(' ');
                doc.push_str(&pattern_to_string(RenderContext::Arg, *arg));
            }
            if context == RenderContext::Arg && !args.is_empty() {
                format!("({doc})")
            } else {
                doc
            }
        }
        Structure::FiniteList(entries) => {
            format!("[{}]", join(&entries, RenderContext::Unambiguous, ","))
        }
        Structure::Conses(conses, last) => {
            let doc = conses.iter().rev().fold(
                pattern_to_string(RenderContext::Unambiguous, last),
                |tail, head| format!("{} :: {}", pattern_to_string(RenderContext::Head, *head), tail),
            );
            if context == RenderContext::Unambiguous {
                doc
            } else {
                format!("({doc})")
            }
        }
    }
}

fn join(patterns: &[Pattern<'_>], context: RenderContext, sep: &str) -> String {
    patterns
        .iter()
        .map(|p| pattern_to_string(context, *p))
        .collect::<Vec<_>>()
        .join(sep)
}

fn literal_to_string(literal: Literal<'_>) -> String {
    match literal {
        Literal::Int(n) => n.to_string(),
        Literal::Str(s) => format!("\"{s}\""),
        Literal::Bytes(bytes) => format!("#\"{}\"", hex::encode(bytes)),
    }
}

enum Structure<'a> {
    FiniteList(Vec<Pattern<'a>>),
    Conses(Vec<Pattern<'a>>, Pattern<'a>),
    NonList(Pattern<'a>),
}

/// Elm's `delist`. Entries come back in source order (Elm leaves finite
/// lists reversed; that only shows for literal entries, which we fix).
fn delist<'a>(pattern: Pattern<'a>, mut rev_entries: Vec<Pattern<'a>>) -> Structure<'a> {
    match pattern {
        Pattern::Ctor { name: NIL_NAME, args: [], .. } => {
            rev_entries.reverse();
            Structure::FiniteList(rev_entries)
        }
        Pattern::Ctor { name: CONS_NAME, args: [head, tail], .. } => {
            rev_entries.push(*head);
            delist(*tail, rev_entries)
        }
        _ if rev_entries.is_empty() => Structure::NonList(pattern),
        _ => {
            rev_entries.reverse();
            Structure::Conses(rev_entries, pattern)
        }
    }
}
```

Changeset `.sampo/changesets/nitpick-skeleton.md`:

```markdown
---
cargo/nash-nitpick: minor
---

Add the `nash-nitpick` crate: simplified patterns and the missing-pattern printer.
```

**Elm reference**

- `Pattern`, `Literal` → `pattern.rs` types
- `simplify`, `cons`, `nil`, `unit`, `pair`, `triple`, `list`, `*Name` →
  `pattern.rs`
- `Error`, `Context` → `pattern.rs`
- `Reporting.Error.Pattern.patternToDoc`, `delist`, `Structure`,
  `Context` → `render.rs`

**Tests** (`render.rs` `mod tests`, hand-built patterns in a `Bump`)

- `render_anything` → `_`
- `render_unit` → `()`
- `render_pair_of_anything` → `( _, _ )`
- `render_finite_list_two` (`1 :: 2 :: []`) → `[1,2]`
- `render_cons_tail_anything` (`1 :: _`) → `1 :: _`
- `render_nested_ctor_in_arg` (`Just (Just _)`) → `Just (Just _)`
- `render_bytes_literal` → `#"00ff"`
- `render_data_constr` (`Constr 0 _`) → `Constr 0 _`

Each is `insta::assert_snapshot!(pattern_to_string(Unambiguous, p))`.

**Done when**

`cargo test -p nash-nitpick` passes, `cargo clippy --all-targets -- -D
warnings` is clean, the workspace still builds.

---

## Chunk 2 — matrix algorithms

**Files**

- `crates/nash-nitpick/src/matrix.rs` (new)

**Change**

Port `isExhaustive`, `isUseful`, `isComplete`, `collectCtors`, the three
`specializeRowBy*` functions, `isMissing`, `recoverCtor`.

**Code**

```rust
//! Maranget's usefulness and exhaustiveness over simplified patterns, from
//! Elm's `Nitpick/PatternMatches.hs`.

use std::collections::BTreeMap;

use bumpalo::Bump;
use nash_ast::{Ctor, Union};

use crate::pattern::{Literal, Pattern};

pub(crate) type Row<'a> = Vec<Pattern<'a>>;

// EXHAUSTIVE PATTERNS

/// Elm's `isExhaustive`: rows of missing patterns, empty when exhaustive.
///
/// Invariant: every row of `matrix` has length `n`; every result row has
/// length `n`.
pub(crate) fn is_exhaustive<'a>(bump: &'a Bump, matrix: &[Row<'a>], n: usize) -> Vec<Row<'a>> {
    if matrix.is_empty() {
        return vec![vec![Pattern::Anything; n]];
    }
    if n == 0 {
        return vec![];
    }

    let ctors = collect_ctors(matrix);
    let Some((_, &alts)) = ctors.first_key_value() else {
        return is_exhaustive(bump, &specialize_all(matrix, specialize_row_by_anything), n - 1)
            .into_iter()
            .map(|rest| prepend(Pattern::Anything, rest))
            .collect();
    };

    if ctors.len() < usize::from(alts.alternatives) {
        let rest = is_exhaustive(bump, &specialize_all(matrix, specialize_row_by_anything), n - 1);
        alts.ctors
            .iter()
            .filter(|ctor| !ctors.contains_key(ctor.name))
            .flat_map(|ctor| {
                let missing = Pattern::Ctor {
                    union: alts,
                    name: ctor.name,
                    args: bump.alloc_slice_fill_copy(usize::from(ctor.arity), Pattern::Anything),
                };
                rest.iter().map(move |tail| prepend(missing, tail.clone()))
            })
            .collect()
    } else {
        alts.ctors
            .iter()
            .flat_map(|ctor| {
                let arity = usize::from(ctor.arity);
                let specialized =
                    specialize_all(matrix, |row| specialize_row_by_ctor(ctor.name, arity, row));
                is_exhaustive(bump, &specialized, arity + n - 1)
                    .into_iter()
                    .map(move |row| recover_ctor(bump, alts, ctor.name, arity, row))
            })
            .collect()
    }
}

/// Elm's `recoverCtor`.
fn recover_ctor<'a>(
    bump: &'a Bump,
    union: &'a Union<'a>,
    name: &'a str,
    arity: usize,
    patterns: Row<'a>,
) -> Row<'a> {
    let (args, rest) = patterns.split_at(arity);
    prepend(
        Pattern::Ctor { union, name, args: bump.alloc_slice_copy(args) },
        rest.to_vec(),
    )
}

fn prepend<'a>(head: Pattern<'a>, mut rest: Row<'a>) -> Row<'a> {
    rest.insert(0, head);
    rest
}

fn specialize_all<'a>(
    matrix: &[Row<'a>],
    specialize: impl Fn(&[Pattern<'a>]) -> Option<Row<'a>>,
) -> Vec<Row<'a>> {
    matrix.iter().filter_map(|row| specialize(row)).collect()
}

// REDUNDANT PATTERNS

/// Elm's `isUseful`: does `vector` match something no row of `matrix` does?
pub(crate) fn is_useful<'a>(matrix: &[Row<'a>], vector: &[Pattern<'a>]) -> bool {
    if matrix.is_empty() {
        return true;
    }
    let Some((first, patterns)) = vector.split_first() else {
        return false;
    };
    match *first {
        Pattern::Ctor { name, args, .. } => {
            let specialized = specialize_all(matrix, |row| specialize_row_by_ctor(name, args.len(), row));
            is_useful(&specialized, &[args, patterns].concat())
        }
        Pattern::Anything => match is_complete(matrix) {
            Complete::No => is_useful(&specialize_all(matrix, specialize_row_by_anything), patterns),
            Complete::Yes(alts) => alts.iter().any(|ctor| {
                let arity = usize::from(ctor.arity);
                let specialized =
                    specialize_all(matrix, |row| specialize_row_by_ctor(ctor.name, arity, row));
                let mut vector = vec![Pattern::Anything; arity];
                vector.extend_from_slice(patterns);
                is_useful(&specialized, &vector)
            }),
        },
        Pattern::Literal(literal) => {
            is_useful(&specialize_all(matrix, |row| specialize_row_by_literal(literal, row)), patterns)
        }
    }
}

/// Invariant: `row.len() == N` implies `result.len() == arity + N - 1`.
fn specialize_row_by_ctor<'a>(ctor_name: &str, arity: usize, row: &[Pattern<'a>]) -> Option<Row<'a>> {
    match row {
        [Pattern::Ctor { name, args, .. }, patterns @ ..] => {
            (*name == ctor_name).then(|| [*args, patterns].concat())
        }
        [Pattern::Anything, patterns @ ..] => {
            let mut out = vec![Pattern::Anything; arity];
            out.extend_from_slice(patterns);
            Some(out)
        }
        [Pattern::Literal(_), ..] => {
            unreachable!("constructors and literals never share a column after type checking")
        }
        [] => unreachable!("empty rows are never specialized"),
    }
}

/// Invariant: `row.len() == N` implies `result.len() == N - 1`.
fn specialize_row_by_literal<'a>(literal: Literal<'a>, row: &[Pattern<'a>]) -> Option<Row<'a>> {
    match row {
        [Pattern::Literal(lit), patterns @ ..] => (*lit == literal).then(|| patterns.to_vec()),
        [Pattern::Anything, patterns @ ..] => Some(patterns.to_vec()),
        [Pattern::Ctor { .. }, ..] => {
            unreachable!("constructors and literals never share a column after type checking")
        }
        [] => unreachable!("empty rows are never specialized"),
    }
}

/// Invariant: `row.len() == N` implies `result.len() == N - 1`.
fn specialize_row_by_anything<'a>(row: &[Pattern<'a>]) -> Option<Row<'a>> {
    match row {
        [Pattern::Anything, patterns @ ..] => Some(patterns.to_vec()),
        _ => None,
    }
}

// ALL CONSTRUCTORS ARE PRESENT?

pub(crate) enum Complete<'a> {
    Yes(&'a [&'a Ctor<'a>]),
    No,
}

/// Elm's `isComplete`.
fn is_complete<'a>(matrix: &[Row<'a>]) -> Complete<'a> {
    let ctors = collect_ctors(matrix);
    match ctors.first_key_value() {
        Some((_, union)) if ctors.len() == usize::from(union.alternatives) => Complete::Yes(union.ctors),
        _ => Complete::No,
    }
}

// COLLECT CTORS

/// Elm's `collectCtors`: constructor names seen in the first column.
fn collect_ctors<'a>(matrix: &[Row<'a>]) -> BTreeMap<&'a str, &'a Union<'a>> {
    matrix
        .iter()
        .filter_map(|row| match row.first() {
            Some(Pattern::Ctor { union, name, .. }) => Some((*name, *union)),
            _ => None,
        })
        .collect()
}
```

**Elm reference**

`isExhaustive`, `isMissing` (folded into the `filter`/`flat_map` in
`is_exhaustive`), `recoverCtor`, `toNonRedundantRows` (chunk 3),
`isUseful`, `specializeRowByCtor`, `specializeRowByLiteral`,
`specializeRowByAnything`, `Complete`, `isComplete`, `collectCtors`,
`collectCtorsHelp`.

**Tests** (`matrix.rs` `mod tests`, hand-built rows over `LIST`, `PAIR`,
and a two-constructor test union built in the `Bump`)

- `exhaustive_empty_matrix_is_anything` — `is_exhaustive(&[], 1)` is `[[_]]`
- `exhaustive_nil_and_cons` — rows `[[]]`, `[_ :: _]` → empty
- `exhaustive_only_nil` — rows `[[]]` → `[[_ :: _]]`
- `exhaustive_nested_cons` — rows `[[]]`, `[_ :: []]` → `[[_ :: _ :: _]]`
- `exhaustive_literal_column_needs_wildcard` — rows `[1]`, `[2]` → `[[_]]`
- `exhaustive_pair_partial` — rows `( True, _ )` → `[( False, _ )]`
- `useful_wildcard_after_all_ctors_is_not` — matrix `[True]`, `[False]`;
  vector `[_]` → false
- `useful_wildcard_after_one_ctor_is` — matrix `[True]`; vector `[_]` → true
- `useful_ctor_after_wildcard_is_not` — matrix `[_]`; vector `[True]` → false
- `useful_literal_after_other_literal_is` — matrix `[1]`; vector `[2]` → true

Assertions with `assert_debug_snapshot!` on the returned rows rendered
through `render::pattern_to_string`, or plain `assert!` for booleans.

**Done when**

All chunk 2 tests pass; the module is `pub(crate)` and unused-warning free
(clippy is `-D warnings`, so `check.rs` must land in the same PR or the
functions get `#[allow(dead_code)]` for one chunk — prefer landing chunk 3
in the same PR if that is simpler).

---

## Chunk 3 — AST traversal and the `check` entry point

**Files**

- `crates/nash-nitpick/src/check.rs` (new)
- `crates/nash-nitpick/src/lib.rs` (tests)

**Change**

Port `check`, `checkDecls`, `checkDef`, `checkArg`, `checkTypedArg`,
`checkExpr`, `checkField`, `checkIfBranch`, `checkCases`,
`checkCaseBranch`, `checkPatterns`, `toNonRedundantRows`,
`toSimplifiedUsefulRows` against `nash_ast` (`crates/nash-ast/src/lib.rs`:
`Decls` :41, `Def` :55, `Expr` :117, `CaseBranch` :199, `FieldUpdate` :205,
`FieldValue` :211).

**Code**

```rust
//! Walk the canonical AST and check every pattern match, from Elm's
//! `Nitpick/PatternMatches.hs` (`check` .. `checkPatterns`).

use bumpalo::Bump;
use nash_ast::{CaseBranch, Decls, Def, Expr, Module, Pattern as CanPattern};
use nash_region::{Located, Region};

use crate::matrix::{Row, is_exhaustive, is_useful};
use crate::pattern::{Context, Error, simplify};

/// Elm's `check`. `Err` is never empty.
pub fn check<'a>(bump: &'a Bump, module: &Module<'a>) -> Result<(), Vec<Error<'a>>> {
    let mut checker = Checker { bump, errors: Vec::new() };
    checker.decls(module.decls);
    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}

struct Checker<'a> {
    bump: &'a Bump,
    errors: Vec<Error<'a>>,
}

impl<'a> Checker<'a> {
    /// Elm's `checkDecls`.
    fn decls(&mut self, decls: &Decls<'a>) {
        match decls {
            Decls::Declare { definition, next } => {
                self.def(definition);
                self.decls(next);
            }
            Decls::DeclareRec { definition, following, next } => {
                self.def(definition);
                for def in *following {
                    self.def(def);
                }
                self.decls(next);
            }
            Decls::Empty => {}
        }
    }

    /// Elm's `checkDef`.
    fn def(&mut self, def: &Def<'a>) {
        match def {
            Def::Def { args, body, .. } => {
                for arg in *args {
                    self.arg(arg);
                }
                self.expr(body);
            }
            Def::TypedDef { args, body, .. } => {
                for arg in *args {
                    self.arg(arg.pattern);
                }
                self.expr(body);
            }
        }
    }

    /// Elm's `checkArg` / `checkTypedArg`.
    fn arg(&mut self, pattern: &'a Located<CanPattern<'a>>) {
        self.patterns(pattern.region, Context::BadArg, &[pattern]);
    }

    /// Elm's `checkExpr`.
    fn expr(&mut self, expr: &Located<Expr<'a>>) {
        match &expr.value {
            Expr::VarLocal(_)
            | Expr::VarTopLevel(_)
            | Expr::VarForeign { .. }
            | Expr::VarConstructor { .. }
            | Expr::VarOperator { .. }
            | Expr::Str(_)
            | Expr::Int(_)
            | Expr::Accessor(_)
            | Expr::Unit => {}
            Expr::List(entries) => {
                for entry in *entries {
                    self.expr(entry);
                }
            }
            // `Expr::Negate` is gone after plan 03 (`-x` is a `Num.negate`
            // call); until then it is checked like any unary `Call`.
            Expr::Binop { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Lambda { parameters, body } => {
                for parameter in *parameters {
                    self.arg(parameter);
                }
                self.expr(body);
            }
            Expr::Call { function, arguments } => {
                self.expr(function);
                for argument in *arguments {
                    self.expr(argument);
                }
            }
            Expr::If { branches, final_else } => {
                for branch in *branches {
                    self.expr(branch.condition);
                    self.expr(branch.then_branch);
                }
                self.expr(final_else);
            }
            Expr::Let { definition, body } => {
                self.def(definition);
                self.expr(body);
            }
            Expr::LetRec { definitions, body } => {
                for definition in *definitions {
                    self.def(definition);
                }
                self.expr(body);
            }
            Expr::LetDestruct { pattern, value, body } => {
                self.patterns(pattern.region, Context::BadDestruct, &[pattern]);
                self.expr(value);
                self.expr(body);
            }
            Expr::Case { scrutinee, branches } => {
                self.expr(scrutinee);
                self.cases(expr.region, branches);
            }
            Expr::Access { record, .. } => self.expr(record),
            Expr::Update { base, fields, .. } => {
                self.expr(base);
                for field in *fields {
                    self.expr(field.value);
                }
            }
            Expr::Record { fields, .. } => {
                for field in *fields {
                    self.expr(field.value);
                }
            }
            Expr::Tuple { first, second, rest } => {
                self.expr(first);
                self.expr(second);
                for entry in *rest {
                    self.expr(entry);
                }
            }
        }
    }

    /// Elm's `checkCases` + `checkCaseBranch`.
    fn cases(&mut self, region: Region, branches: &[CaseBranch<'a>]) {
        let patterns: Vec<_> = branches.iter().map(|branch| branch.pattern).collect();
        self.patterns(region, Context::BadCase, &patterns);
        for branch in branches {
            self.expr(branch.body);
        }
    }

    /// Elm's `checkPatterns`.
    fn patterns(&mut self, region: Region, context: Context, patterns: &[&'a Located<CanPattern<'a>>]) {
        match self.to_non_redundant_rows(region, patterns) {
            Err(error) => self.errors.push(error),
            Ok(matrix) => {
                let missing = is_exhaustive(self.bump, &matrix, 1);
                if !missing.is_empty() {
                    // Every missing row has length 1 (Elm's `map head`).
                    let unhandled = self.bump.alloc_slice_fill_iter(missing.iter().map(|row| row[0]));
                    self.errors.push(Error::Incomplete { region, context, unhandled });
                }
            }
        }
    }

    /// Elm's `toNonRedundantRows` / `toSimplifiedUsefulRows`. Every row has
    /// length 1.
    fn to_non_redundant_rows(
        &self,
        case_region: Region,
        patterns: &[&'a Located<CanPattern<'a>>],
    ) -> Result<Vec<Row<'a>>, Error<'a>> {
        let mut checked: Vec<Row<'a>> = Vec::with_capacity(patterns.len());
        for pattern in patterns {
            let next_row = vec![simplify(self.bump, pattern)];
            if !is_useful(&checked, &next_row) {
                return Err(Error::Redundant {
                    case_region,
                    pattern_region: pattern.region,
                    index: checked.len() + 1,
                });
            }
            checked.push(next_row);
        }
        Ok(checked)
    }
}
```

**Elm reference**

One method per Haskell function, named in the doc comments above.

**Tests** (`lib.rs` `mod tests`)

Helper running the real pipeline so inputs are known to be well typed:

```rust
fn check_source<'a>(bump: &'a Bump, source: &'a str) -> Result<(), Vec<Error<'a>>> {
    let module = nash_parse::Parser::new(bump, source.as_bytes()).module().expect("parse");
    let can = nash_can::canonicalize(bump, nash_can::Context::default(), &module).expect("canonicalize");
    let mut uf = nash_constrain::UnionFind::new();
    let constraint = nash_constrain::constrain(bump, &mut uf, &can.module);
    // Final signature in plans/03-traits.md chunk 5 "Solver API":
    // `run(bump, uf, constraint, tables, fields, mode)
    //   -> Result<(Annotations, SolvedTypes), Vec<Error>>`.
    nash_solve::run(bump, &mut uf, &constraint, &can.tables, &can.fields, nash_can::Mode::Strict)
        .expect("type check");
    check(bump, &can.module)
}

macro_rules! assert_nitpick_snapshot {
    ($input:expr) => {{
        let input = indoc!($input);
        let bump = Bump::new();
        let errors = check_source(&bump, input).expect_err("expected nitpick errors");
        let rendered: Vec<String> = errors.iter().map(describe).collect();
        insta::with_settings!({ description => format!("Code:\n\n{}", input), omit_expression => true }, {
            insta::assert_debug_snapshot!((rendered, errors));
        });
    }};
}

macro_rules! assert_nitpick_ok {
    ($input:expr) => {{
        let bump = Bump::new();
        check_source(&bump, indoc!($input)).expect("expected no nitpick errors");
    }};
}

/// One line per error: context, region, and the missing patterns.
fn describe(error: &Error<'_>) -> String {
    match error {
        Error::Incomplete { region, context, unhandled } => format!(
            "Incomplete {:?} {}:{}-{}:{}: {}",
            context, region.start.line, region.start.column, region.end.line, region.end.column,
            unhandled.iter().map(|p| render::pattern_to_string(render::RenderContext::Unambiguous, *p)).collect::<Vec<_>>().join(" | ")
        ),
        Error::Redundant { case_region, pattern_region, index } => format!(
            "Redundant #{index} pattern {}:{} in case {}:{}",
            pattern_region.start.line, pattern_region.start.column, case_region.start.line, case_region.start.column
        ),
    }
}
```

Test cases (module header `module Main exposing (..)` on every input):

- `case_missing_ctor`
  ```elm
  type color = Red | Green | Blue

  name : color -> int
  name c =
      case c of
          Red -> 1
          Green -> 2
  ```
  expect `Incomplete BadCase ...: Blue`
- `case_missing_nested`
  ```elm
  first : list (option int) -> int
  first xs =
      case xs of
          [] -> 0
          (Some x) :: _ -> x
  ```
  expect `None :: _`
- `case_int_literals_need_wildcard`
  ```elm
  f : int -> int
  f n =
      case n of
          0 -> 1
          1 -> 1
  ```
  expect `_`
- `case_string_literals_need_wildcard` — same with `"a"`, `"b"`
- `case_redundant_wildcard_then_ctor`
  ```elm
  f : bool -> int
  f b =
      case b of
          _ -> 0
          True -> 1
  ```
  expect `Redundant #2`
- `case_redundant_duplicate_ctor` — `True -> 1`, `False -> 0`, `True -> 2`
- `case_tuple_partial` — `case ( a, b ) of ( True, _ ) -> 1` → `( False, _ )`
- `case_exhaustive_ok` (`assert_nitpick_ok`) — all three colors
- `case_wildcard_after_partial_ok` — `Red -> 1`, `_ -> 0`
- `arg_pattern_unsafe`
  ```elm
  head : list int -> int
  head (x :: _) = x
  ```
  expect `Incomplete BadArg ...: []`
- `destruct_unsafe`
  ```elm
  f : option int -> int
  f o =
      let
          (Some x) = o
      in
      x
  ```
  expect `Incomplete BadDestruct ...: None`
- `errors_in_source_order` — a module with an unsafe argument, then a
  `case` whose scrutinee has a nested incomplete `case`, then a redundant
  branch; snapshot shows the three errors in source order.
- `lambda_arg_unsafe` — `map (\(x :: _) -> x) xs`
- `let_def_body_checked` — incomplete `case` inside a `let` definition body
- `record_and_update_fields_checked` — incomplete `case` inside a record
  field value and inside `{ r | f = case ... }`

**Done when**

Snapshots accepted; error order matches the description above; `nash-nitpick`
has no `#[allow(dead_code)]`.

---

## Chunk 4 — `Data`, bytes, Big ADTs, and the driver hook

**Files**

- `crates/nash-nitpick/src/pattern.rs` (`CanPattern::Bytes` arm)
- `crates/nash-nitpick/src/lib.rs` (tests)
- `crates/nash-driver/src/compile.rs` (`compile_module`)
- `crates/nash-driver/Cargo.toml`
- `.sampo/changesets/nitpick-driver.md`

**Change**

1. Add the bytes literal arm to `simplify` once plan 01 lands
   `Pattern::Bytes`:

   ```rust
   CanPattern::Bytes(bytes) => Pattern::Literal(Literal::Bytes(bytes)),
   ```

2. Hook the check into the driver after solving, mirroring Elm's
   `Compile.hs` (`typeCheck` then `nitpick`). In
   `crates/nash-driver/src/compile.rs:198-206`, after
   `nash_solve::run(...)` succeeds:

   ```rust
   // Final signature in plans/03-traits.md chunk 5 "Solver API".
   let (annotations, solved) =
       match nash_solve::run(&bump, &mut uf, &constraint, &can_result.tables, &can_result.fields, mode) {
           Ok(solved) => solved,
           Err(errors) => return failed(format!("{:?}", errors)),
       };

   if let Err(errors) = nash_nitpick::check(&bump, &can_result.module) {
       return failed(format!("{:?}", errors));
   }
   ```

   `failed(format!("{:?}", ..))` is the driver's current placeholder for
   every error kind; plan 06 replaces all of them with reports at once.

   `nash-driver/Cargo.toml` gains
   `nash-nitpick = { path = "../nash-nitpick", version = "0.1.0" }`.

3. Traits (plan 03) run their own checks on the same module before this;
   nitpick reads only patterns and unions, so trait evidence slots do not
   affect it.

**Code** — no new types. `Data` needs nothing: its union in the canonical
environment has five constructors and `simplify`'s `Constructor` arm
recurses into the tag (`Literal::Int` or `Anything`) and the fields (a list
pattern).

**Elm reference** — `Compile.hs` `compile` (the `nitpick` step).

**Tests** (`lib.rs`)

- `data_case_missing_ctors`
  ```elm
  tag : Data -> int
  tag d =
      case d of
          Constr n _ -> n
          List _ -> 0
  ```
  expect `Map _ | I _ | B _`
- `data_constr_tag_literals_need_wildcard`
  ```elm
  f : Data -> int
  f d =
      case d of
          Constr 0 _ -> 0
          Constr 1 _ -> 1
          Map _ -> 2
          List _ -> 3
          I _ -> 4
          B _ -> 5
  ```
  expect `Constr _ _`
- `data_constr_fields_list_partial`
  ```elm
  f : Data -> Data
  f d =
      case d of
          Constr _ [] -> d
          Constr _ (x :: _) -> x
          _ -> d
  ```
  `assert_nitpick_ok`
- `data_constr_redundant_after_wildcard_tag` — `Constr _ _` then
  `Constr 0 _` → `Redundant #2`
- `bytes_literals_need_wildcard` — `#"00"`, `#"ff"` → `_`
- `big_adt_same_as_little`
  ```elm
  type Redeemer = Claim | Cancel
  type step = Go | Stop

  f : Redeemer -> step -> int
  f r s =
      case ( r, s ) of
          ( Claim, Go ) -> 1
          ( Cancel, _ ) -> 2
  ```
  expect `( Claim, Stop )`
- driver: `crates/nash-driver/src/compile.rs` test
  `test_nitpick_error_fails_module` — a module with a missing pattern
  yields `ModuleResult::Failed` and the message contains `Incomplete`.

Changeset:

```markdown
---
cargo/nash-nitpick: minor
cargo/nash-driver: minor
---

Run exhaustiveness and redundancy checks after type solving.
```

**Done when**

`nash check` on a project with a non-exhaustive `case` fails with the
`Incomplete` debug output; all snapshots accepted; CI checks green.

---

## Open questions

1. **Elm's reversed finite lists.** Elm's `delist` prints finite-list
   entries in reverse (`FiniteList revEntries`). This port prints them in
   source order. If exact Elm parity of output text is preferred, drop the
   `reverse` in the `NIL_NAME` arm.
