# Plan 05 — `nash-nitpick`: exhaustiveness and redundancy checking

## Goal

Port Elm's `Nitpick/PatternMatches.hs` (Maranget's pattern-matrix algorithm)
into `nash-nitpick`. Run once per module after successful type solving and
before producing any interface or retained solved module.

- `Incomplete`: a case, function/lambda argument, or let destructure does not
  cover its type. Carry its region, context, and missing-pattern examples.
- `Redundant`: a case branch is covered by preceding branches. Carry the case
  region, branch-pattern region, and one-based branch index.

Use the local Elm source as the primary implementation reference:

- `elm/compiler/src/Nitpick/PatternMatches.hs`: simplified patterns,
  specialization, usefulness, missing-pattern examples, and AST traversal.
- `elm/compiler/src/Reporting/Error/Pattern.hs`: `patternToDoc`, `delist`, and
  rendering precedence. This crate owns plain pattern text; styled reports
  and diagnostic prose remain Plan 06.
- `elm/compiler/src/Compile.hs`: type checking before nitpick.

Code generation and pattern decision trees remain Plan 07.

## Reconciliation with Plans 01–04

The old code sketches predated the completed AST and solver. Implementation
must use the current declarations rather than copying those sketches:

- `Union` has `kind` and `context`; `Ctor` has optional ordered `labels`.
  Synthetic unit/tuple/list unions include these fields and valid builtin
  types. The builtin module owns little `list`, not Elm's `List.List`.
- `Pattern::Bytes` already exists. `Expr::Bytes` and `VarMethod` are leaves;
  `Expr::Negate` no longer exists.
- `Expr::Record { alias, annotation, fields }` stores fields in declaration
  order, which can differ from their source order.
- Labeled patterns are already expanded to full positional constructor
  arguments in declaration order; omitted labels become wildcards. Preserve
  that order for matching. Positional constructor witness text is valid for
  labeled constructors too.
- Executable roots include ordinary declarations, trait method defaults,
  and impl method definitions. Checking only `module.decls` is insufficient.
- The solver signature is `run(bump, &mut uf, &constraint, &tables)` and
  returns annotations and solved types. There are no `fields` or `mode`
  arguments. Hook nitpick before `from_module` in the driver.
- Use actual independently versioned crate dependencies. At the start of
  this plan: AST 0.6, canonicalizer 0.5, parser 0.4, constrain/solve/driver 0.3,
  region 0.2; the new nitpick crate starts at 0.1.0.

## Algorithm and invariants

Simplified patterns are `Anything`, `Literal` (int/string/bytes), and `Ctor`
with a union, constructor name, and positional arguments. Unit, pairs,
triples and list syntax use synthetic unions. Records are irrefutable.
Aliases simplify their inner pattern. Bool uses its canonical union.

`Data` is its ordinary five-constructor union: `Constr`, `Map`, `List`, `I`,
`B`. Tags are int patterns and fields are list patterns. Big and little ADTs
have the same coverage rules; runtime representation never changes coverage.

Use `Vec<Pattern>` rows and a `Vec<Row>` scratch matrix. Persistent pattern
arguments and missing-pattern results live in the module bump arena; never
put owning/drop-requiring collections into bump allocations.

Preserve Elm's base cases, specialization rules and constructor order:

- Empty matrix and zero columns returns one empty missing row; a nonempty
  zero-column matrix is exhaustive. An empty matrix makes any vector useful.
- Literal domains require a wildcard. Missing-pattern examples may contain
  `_`, representing an uncovered part of a literal domain; these examples
  are not an exact, disjoint complement of preceding patterns.
- Constructor names can be compared within a column because successful type
  solving guarantees a single type per column. Do not run on untyped input.
- Keep the first redundant branch per case and suppress that case's
  incomplete error, matching Elm. Still visit all nested branch bodies.

Visit definitions in source order despite canonical dependency ordering,
including trait defaults and impl methods. Within expressions, visit:
arguments before bodies; case scrutinee before the case's own error before
branch bodies; destructure pattern before value before body; field values in
source order. Preserve source order when labeled construction reorders call
arguments. Diagnostics must remain deterministic.

The pattern printer preserves finite/open list head order, parentheses for
nested constructors/cons patterns, and Nash string escapes. The old Rust
sketch incorrectly reversed a Vec already accumulated in source order; do
not retain either reversal. Unicode escapes require four to six hex digits.

## Chunk 1 — patterns, simplification and rendering

- [x] Add the crate, arena-backed pattern/error/context types, valid synthetic
  unions, simplification for every current pattern form, and plain printer.
- [x] Granular renderer snapshots: wildcard, unit, pair, triple, finite/open
  multi-element lists, nested cons heads, nested constructor arguments,
  `Data` constructors, and bytes. Round-trip escaped strings through Nash.
- [x] Verify the chunk with formatting, strict Clippy and workspace tests;
  add a changeset and create the first incremental commit.

## Chunk 2 — pattern matrices

- [ ] Port `isExhaustive`, `isUseful`, constructor/literal/wildcard
  specialization, completeness and missing-constructor recovery.
- [ ] Test empty/zero-column matrices, nil/cons, partial nested lists,
  literal domains, partial tuples, constructor recovery and wildcard
  usefulness both before and after complete constructor coverage.
- [ ] Independently compare finite-domain usefulness/exhaustiveness against
  direct enumeration, including multi-column correlations.
- [ ] Keep runtime functions warning-free when this lands with traversal.

## Chunk 3 — all executable AST roots and expressions

- [ ] Add `check`, definition/argument checking, complete expression traversal
  and source-order diagnostics, including traits, impls and recursive groups.
- [ ] Source tests must parse, canonicalize and solve successfully before
  nitpick. Use separate success/error snapshot helpers.
- [ ] Test safe/unsafe typed and untyped arguments, lambda arguments,
  destructures, nested case scrutinees/bodies, redundancy and its precedence,
  recursive definitions, call function/arguments, if conditions/branches,
  lists/tuples, record/update values and access receivers.
- [ ] Test source order across dependency-sorted declarations, method bodies,
  reordered record fields and labeled constructor arguments.
- [ ] Test record patterns and labeled subset patterns, including reordered
  labels, omitted-label wildcards and multi-constructor unions.

## Chunk 4 — Nash unions, cross-module behavior and driver

- [ ] Test every `Data` constructor, partial literal tags, partial field lists,
  and a redundant specific tag after a wildcard tag.
- [ ] Test bytes needing a wildcard, literal duplicates, Bool, Big/little
  coverage, aliases and imported/qualified constructor unions.
- [ ] Hook into the driver after solving, before interface publication. CLI
  checks must reject incomplete and redundant matches with useful witnesses.
- [ ] Test failed-module propagation: no public/canonical interface or solved
  module from a rejected module, including importing dependents.
- [ ] Build the real core package and investigate newly rejected patterns.
- [ ] Complete `cargo fmt --all`, strict all-target/all-feature Clippy,
  `cargo test`, `cargo insta test`, review/accept snapshots and check snapshot
  hygiene. Update Sampo changesets with compatible dependency ranges and
  preserve publication order. Mark SPEC complete only after these gates pass.
- [ ] Create incremental `jj` commits. Keep scratch examples out of the
  implementation history and leave ignored local scratch files intact.

## Validation record

Chunk 1 (2026-09-09 UTC): `cargo fmt --all`,
`cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`
pass (2,105 tests, including 13 nitpick tests). Reviewed and accepted the
escaped-string snapshot; the other renderer expectations are inline snapshots.
