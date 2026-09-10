# Frontend hardening verification

The six frontend cleanup changes and direct AST inference are complete. The
replacement preserves the union-find and predicate engines, removes the
allocated Constraint tree and intermediate inference Type, and passes all
adoption gates. Plan 07 and code generation are outside this change.

## Commit sequence

Each row is a separate, verified jj change. Change IDs remain stable across
rebases; use `jj log -r <change-id>` for the current commit hash.

| jj change | Change |
|---|---|
| `lrnzrllu` | Remove parser accumulator copies |
| `tsqsspqw` | Require valid UTF-8 input |
| `wplqvsku` | Widen source coordinates |
| `rnvunqnt` | Bound parser nesting and iterate flat sequences |
| `otpvxozr` | Remove inactive interface-cache machinery |
| `ovwmkvpo` | Borrow canonical local scopes |
| `npksuwvn` | Bound trait candidate traversal |
| `lssnqtpu` | Infer directly from the canonical AST |

An external rebase placed these changes on release commit `046cb63c`. Its
changes from the saved baseline are release metadata and changelogs; no Rust
source changed. The release changes are preserved.

## Validation

Each completed implementation chunk passed formatting, workspace check with all
targets and features, clippy with all targets/features and warnings denied,
full workspace tests, and relevant scratch checks. Final commands were:

```sh
cargo fmt --all
cargo check --workspace --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo run -p nash-cli -- check scratch
cargo test --release -p nash-parse
```

The parser tests cover Unicode and escapes, coordinates beyond 65,535,
arbitrary lookahead, oversized Unicode escapes, nesting on a 2 MiB stack, and
long flat sequences. Scope regressions cover shadowing and error recovery.
Trait traversal compares candidate keys and payload identity with the original
ordered full-map filter, including missing traits and empty heads.

Seven additional negative CLI scratch projects check recursive infinite types,
annotated if/case branches, Cons tails, and alias bindings. Their diagnostics
remain present. A qualified recursive-group project with a trait default
compiles successfully.

## Parser allocation

The measurement is retained arena bytes after parsing an operator chain. Tests
also verify operand count and complete source consumption.

| Operands | Original accumulator | After accumulator change | Final, wider Region |
|---:|---:|---:|---:|
| 1,000 | 4,192,960 | 130,048 | 261,056 |
| 2,000 | 16,775,744 | 261,056 | 523,136 |
| 4,000 | — | 523,136 | 1,047,360 |

Both optimized columns were measured in debug and release and gave identical
results. Final allocation grows approximately twofold per input doubling.
Region now uses usize coordinates, increasing its size from 8 to 32 bytes on
the tested 64-bit host.

## Inference parity

The saved original engine is based on `59668452`. Two original-engine runs
produce identical transcripts. The final replacement produces **426 matching
records**, with no missing or changed records. Each record contains either the
complete ordered diagnostics or complete annotations and all four SolvedTypes
maps, including scheme binders, type arguments, predicate contexts, and nested
evidence.

The recorder assigns structural canonical-AST paths to pointer-backed NodeIds
and sorts map entries. It does not sort diagnostics, free variables, contexts,
type arguments, or evidence. An unmapped NodeId fails the recording.

All 411 original records remain. Three synthetic tests moved to immediate
solver operations; their full errors are compared using the same test names.
The metadata test now checks actual solved schemes/evidence and is also run
against the original engine. Seven new original-engine recovery fixtures add
14 records, including their support-module inference. Baseline runs pass 205
tests each; the candidate passes 209, including four migrated low-level tests.
Twenty-two additional targeted original/candidate probes match as well.

The added regressions check recursive occurs checks before generalization,
separate canonical expectations for annotated branches, separate Cons tail
structures, and canonical alias headers. Each failed against the first direct
implementation before its correction. Their snapshots come from the original
engine; existing snapshots were not changed to accommodate the replacement.

## Rust size

Counts are physical Rust lines, including comments and blank lines. A syn-based
tool excludes complete cfg(test) items and external test-only modules while
retaining production below embedded tests. All replacement helpers are counted.

| Inference pipeline crate | Production before | Production after | Delta |
|---|---:|---:|---:|
| nash-constrain | 3,045 | 1,016 | -2,029 |
| nash-solve | 6,197 | 7,593 | +1,396 |
| nash-driver | 1,554 | 1,553 | -1 |
| nash-report | 10,412 | 10,412 | 0 |
| **Total** | **21,208** | **20,574** | **-634** |

Test Rust grows from 14,757 to 14,908 lines (+151); total Rust decreases from
35,965 to 35,482 lines (-483). Snapshots and documentation are excluded from
these Rust counts. Across all frontend changes from `b7ff823b`, production Rust
decreases by 625 lines, tests grow by 310, and total Rust decreases by 315.
The unchanged nash-plutus crate does not affect these deltas.

## Local evidence

The retained local audit root is
`/tmp/nash-frontend-hardening-20260910`. It contains:

- `08b-results.json` and `08b-*.log`: final required checks and parser allocation.
- `08b-scratch-regressions.json`: additional CLI results and diagnostic titles.
- `inference-audit/final7-comparison.json`: complete-record parity and source hash checks.
- `inference-audit/README.md`, `audit.py`, and `recorder.rs`: reproduction instructions and recorder.
- `loc-audit/README.md`, `final7.tsv`, and `final7-all.tsv`: the parsed line-count method and totals.
- `occurs-review/`: targeted fixtures and complete original/candidate outputs.

The seven recovery snapshots are committed with the inference tests. External
audit copies contain the original engine only for verification; the production
tree has one direct inference path.
