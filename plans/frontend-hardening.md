# Frontend hardening before Plan 07

Implement the six Nash/Alder comparison findings, then evaluate a direct inference replacement. Preserve Nash language semantics. Do not begin Plan 07 or add compatibility paths.

## Chunks

- [x] Remove expression-parser accumulator copies. A successful attempt returns one new argument or operator; only then does the loop change its accumulators. Existing parsing and error snapshots stay unchanged.
- [x] Require valid UTF-8 at the parser boundary and remove unchecked conversions.
- [x] Widen coordinates with a safe source bound.
- [x] Guard mutually recursive parsing with a measured nesting limit and remove flat-sequence recursion.
- [x] Delete unused driver interface-cache machinery and orphaned dependencies.
- [x] Separate canonical module data from local scopes without cloning the whole environment.
- [x] Bound trait selection and evidence lookup using existing map ordering.
- [x] Replace the constraint tree and intermediate inference Type with direct AST inference, subject to the adoption gates below.

## Verification and commits

Use a separate reviewed jj commit for each verified logical chunk. Before each commit run cargo fmt --all, cargo check --workspace --all-targets --all-features, cargo clippy --all-targets --all-features -- -D warnings, and cargo test. Add focused regressions first, inspect snapshots, and run relevant scratch cases. Update this record and Sampo changesets with each chunk. Do not push.

Parser allocation regression: before the first change, 1,000 and 2,000 operands retained 4,192,960 and 16,775,744 arena bytes. Afterward 1,000 / 2,000 / 4,000 operands retain 130,048 / 261,056 / 523,136 bytes in both debug and release probes. The test checks complete consumption, operand count, and bounded growth. Function application and negative-argument paths now append a single parsed argument instead of copying the accumulated list.

After widening Region, the final measurements are 261,056 / 523,136 / 1,047,360 bytes for the same 1,000 / 2,000 / 4,000 operands, identical in debug and release. Each doubling uses approximately twice the arena memory. The earlier measurements describe the smaller Region representation at chunk 1.

Chunk 1 verification passed: formatting, workspace check (all targets/features), clippy (all targets/features, warnings denied), full workspace tests, `nash check scratch`, and the release allocation test. Existing snapshots were unchanged.

Chunk 2 requires `Parser::new` source text to be `&str`; all callers are updated directly, with no byte-input adapter. All seven unchecked UTF-8 conversions are replaced with checked conversions. Added snapshots preserve raw Unicode, mixed Unicode/escapes, and CRLF normalization. A compile-fail doctest rejects arbitrary bytes. Formatting, workspace check, clippy, full tests (including the doctest), and `nash check scratch` passed. Only the three reviewed new Unicode snapshots were added.

Chunk 3a uses usize coordinates and indentation, bounded by the source string allocation (at most isize::MAX bytes). This avoids a fallible constructor and removes coordinate casts; Region grows to 32 bytes on 64-bit hosts. LSP explicitly reports positions beyond its 32-bit range. Source bounds, arbitrary lookahead, and oversized Unicode escape widths are covered by regressions. The Unicode numeric accumulator saturates only as an invalid-code marker, preventing overflow while retaining full diagnostic width. Larger error payloads prompted removal of a trivial header adapter and arena storage of the rare irregular-recursion reference; no lint was suppressed. Formatting, check, clippy, all workspace tests, the positive scratch project, and all 441 parser tests plus doctests in release passed. CLI JSON negative scratch cases report the exact missing-name positions at line 65,539 and column 65,544.

Chunk 3b bounds combined recursive expression, pattern, and type entries at 64. Exhaustion remains committed across backtracking and the counter resets after returning. The original 512-parenthesis input aborted on a 2 MiB stack; guarded parsing reports excessive nesting. Fixed-stack regressions cover ordinary and negative parentheses, lists, lambdas, constructor patterns, type parentheses, and arrows. Flat access chains, lambda arguments, else-if branches, let definitions, case arms, and union variants now iterate without accumulator copies; 2,000-element cases and 65,536 nested comments use bounded stack.

Chunk 3b verification passed: formatting, workspace check, clippy, full tests, all 444 parser tests plus doctests in release, positive scratch compilation, and a CLI JSON negative scratch case reporting EXCESSIVE NESTING. Existing snapshots are unchanged; the new nesting report was reviewed.

Chunk 4 removes InterfaceCache, ModuleMeta, interface load/save, the cache-only serialization error, serde derives, and two dedicated cache tests. No active build caller used these APIs. In-memory Interface/Export and fingerprint tests remain. The driver no longer depends on serde or bincode; bincode is removed from workspace dependencies and the lockfile.

Chunk 4 verification passed: formatting, workspace check, clippy, full tests including interface-contract regressions, and positive scratch compilation. No snapshots changed and the lockfile removes only bincode and the two driver dependency edges.

Chunk 5 removes Env cloning and the local-variable variant from module lookup. Five scope entry points now borrow module data and binding maps through a parent chain. Shadowing checks, let-group visibility, generated section names, free-variable bookkeeping, and sorted suggestions preserve their prior behavior. Three new snapshots captured against the original implementation verify sibling reuse, recovery after missing names, and recovery after rejected shadowing; all canonicalizer tests pass unchanged after replacement.

Chunk 5 verification passed: formatting, workspace check, clippy, full tests, positive scratch compilation, and a negative CLI JSON case retaining both independent missing-name diagnostics after scope exit. The three new baseline snapshots were reviewed individually; existing snapshots stayed unchanged.

Chunk 6 replaces full-map trait filters in selection, evidence, canonical entailment, and missing-impl suggestions with a single ordered range helper. ImplKey compares trait identity before its head slice; the empty slice is its lower bound. An equivalence regression compares every candidate key and payload identity against the old filter across 18 package/module/name combinations, prefix-adjacent trait names, multiple head keys, empty heads, and missing traits.

Chunk 6 verification passed: formatting, workspace check, clippy, full tests (including ordered candidate equivalence and existing matching/evidence/ambiguity regressions), and scratch compilation. Existing snapshots are unchanged.

## Direct inference adoption gates

Retain the existing union-find and predicate engines. Preserve ranks, generalization, recursive groups, annotations, aliases, higher-kinded applications, representation predicates, deferred fields, complete diagnostics in order, independent-error recovery, and SolvedTypes/evidence contracts. Keep an external baseline for differential tests; compare successful outputs and complete diagnostics with incidental allocation IDs normalized.

Remove both Constraint and the intermediate inference Type without introducing a delayed replacement IR. Count all production Rust changes across the affected pipeline, including helpers and moved code; tests are counted separately. Adopt only with demonstrated behavioral parity and a net production-code reduction. If a gate fails, keep the original engine and document concrete evidence. Do not retain dual engines in the final implementation.

The replacement passes these gates. The driver now passes the canonical module to the solver. Expressions, patterns, definitions, recursive groups, and annotated branches infer directly into the existing union-find and predicate engine. Both old representations and their construction/conversion paths are deleted. Canonical annotation structures remain available for independent equations; only their variables share the lexical substitution.

The differential audit compares 426 complete records byte for byte, with two identical original-engine runs. It preserves all 411 original records, adds a complete scheme/evidence metadata fixture, and adds seven original-engine diagnostic regressions. Only pointer NodeIds and unordered map iteration are normalized. Twenty-two additional targeted probes also match. The new regressions were observed failing before the fixes; they preserve recursive occurs-check timing, separate annotated branch expectations, Cons tail expectations, and canonical alias headers. Existing snapshots remain unchanged. Representation-level tests were migrated to direct operations and solved-output assertions.

Production Rust across nash-constrain, nash-solve, nash-driver, and nash-report decreases from 21,208 to 20,574 physical lines (634 removed), including all replacement helpers. Test Rust increases from 14,757 to 14,908 lines (151 added); total Rust decreases by 483 lines. The count parses cfg(test) items and includes production below test modules. See [the verification record](../docs/frontend-hardening-verification.md) for per-crate counts, artifacts, and scope.

## Final verification

Check Unicode and escape behavior, coordinates in debug and release, mixed recursive forms on a fixed stack, scope errors and shadowing, trait-selection equivalence, and inference differential cases. Re-run workspace checks against the final state. Report jj commits, measurements, parity results, net code changes, and any unmet adoption gate. Leave no unintended or uncommitted task changes.

Final formatting, all-target/all-feature workspace check, clippy with warnings denied, full workspace tests, positive scratch compilation, and the full release parser suite passed. Refreshed debug/release allocation probes passed. Seven negative CLI scratch projects retain their expected diagnostics, and a qualified recursive-group scratch project compiles. All adoption gates are met; Plan 07 remains untouched.
