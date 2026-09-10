# Frontend hardening before Plan 07

Implement the six Nash/Alder comparison findings, then evaluate a direct inference replacement. Preserve Nash language semantics. Do not begin Plan 07 or add compatibility paths.

## Chunks

- [x] Remove expression-parser accumulator copies. A successful attempt returns one new argument or operator; only then does the loop change its accumulators. Existing parsing and error snapshots stay unchanged.
- [ ] Require valid UTF-8 at the parser boundary and remove unchecked conversions.
- [ ] Widen coordinates with a safe input bound; guard mutually recursive parsing with a measured nesting limit.
- [ ] Delete unused driver interface-cache machinery and orphaned dependencies.
- [ ] Separate canonical module data from local scopes without cloning the whole environment.
- [ ] Bound trait selection and evidence lookup using existing map ordering.
- [ ] Replace the constraint tree and intermediate inference Type with direct AST inference, subject to the adoption gates below.

## Verification and commits

Use a separate reviewed jj commit for each verified logical chunk. Before each commit run cargo fmt --all, cargo check --workspace --all-targets --all-features, cargo clippy --all-targets --all-features -- -D warnings, and cargo test. Add focused regressions first, inspect snapshots, and run relevant scratch cases. Update this record and Sampo changesets with each chunk. Do not push.

Parser allocation regression: before the first change, 1,000 and 2,000 operands retained 4,192,960 and 16,775,744 arena bytes. Afterward 1,000 / 2,000 / 4,000 operands retain 130,048 / 261,056 / 523,136 bytes in both debug and release probes. The test checks complete consumption, operand count, and bounded growth. Function application and negative-argument paths now append a single parsed argument instead of copying the accumulated list.

Chunk 1 verification passed: formatting, workspace check (all targets/features), clippy (all targets/features, warnings denied), full workspace tests, `nash check scratch`, and the release allocation test. Existing snapshots were unchanged.

## Direct inference adoption gates

Retain the existing union-find and predicate engines. Preserve ranks, generalization, recursive groups, annotations, aliases, higher-kinded applications, representation predicates, deferred fields, complete diagnostics in order, independent-error recovery, and SolvedTypes/evidence contracts. Keep an external baseline for differential tests; compare successful outputs and complete diagnostics with incidental allocation IDs normalized.

Remove both Constraint and the intermediate inference Type without introducing a delayed replacement IR. Count all production Rust changes across the affected pipeline, including helpers and moved code; tests are counted separately. Adopt only with demonstrated behavioral parity and a net production-code reduction. If a gate fails, keep the original engine and document concrete evidence. Do not retain dual engines in the final implementation.

## Final verification

Check Unicode and escape behavior, coordinates in debug and release, mixed recursive forms on a fixed stack, scope errors and shadowing, trait-selection equivalence, and inference differential cases. Re-run workspace checks against the final state. Report jj commits, measurements, parity results, net code changes, and any unmet adoption gate. Leave no unintended or uncommitted task changes.
