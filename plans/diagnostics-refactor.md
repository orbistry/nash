# Concise, source-aware diagnostics

## Objective

Use concise messages, expected/actual type comparisons, and at most one useful
hint. Preserve error coverage, structural type differences, independent-error
recovery, and consistent terminal, JSON, and LSP output.

## Work

- [x] Replace fixed snippet shapes with arbitrary source labels and related reports.
- [x] Give diagnostics explicit stable codes independent of display titles.
- [x] Retain and label expectation origins (annotations, earlier elements/branches).
- [x] Label opening delimiters for missing or incorrect closing syntax.
- [x] Rewrite syntax, naming, type, pattern, and warning reports in concise prose.
- [x] Expose labels, related locations, and suggestions in structured JSON.
- [x] Remove redundant source drawing and verbose formatting machinery while retaining type diffs.
- [x] Update diagnostics documentation, snapshots, and Sampo changesets.
- [x] Verify terminal/JSON/LSP parity, source locations, recovery, and all required checks.

## Verification

Add focused tests before changing each behavior. Inspect rendered reports and
snapshot changes, including Unicode/EOF spans and multiple related locations.
Run `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`,
and `cargo test`. This work does not change inference/recovery semantics.

## Result

Completed with regression coverage for annotation and sibling origins, imported
function aliases, arbitrary labels and related sources, stable codes, structured
JSON/LSP conversion, malformed and underindented closers (including comments),
unclosed literals, and Unicode/CRLF/tab/EOF rendering. Independent-error counts
and recovery tests remain intact.

Validation: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test`, and `cargo insta test --unreferenced delete` passed. Snapshot updates
were reviewed and accepted; no pending snapshots remain.
