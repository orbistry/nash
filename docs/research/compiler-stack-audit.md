# Compiler stack audit — 2026-09-29

Default O1 exposed a stack overflow in the driver `ledger_contexts` fixture.
This audit distinguishes reproduced failures from recursive paths that still
need depth regressions. Passing the fixture does not establish a universal
compiler depth bound.

## Reproduced failures

- O1 cleanup after ANF overflowed on the ordinary driver blocking thread.
  `program::assemble_core_with_options` now runs O1 assembly on a 32 MiB
  stack. This covers optimization, recursion encoding, lowering, De Bruijn
  conversion and target validation. It raises the limit; it does not make
  these recursive algorithms depth independent. O0 remains unchanged.
- After assembly succeeded, `nash_plutus::pretty::program` overflowed while
  rendering the named UPLC. Driver phase probes isolated this call, before
  Flat encoding. The term printer now uses explicit visit/finish frames,
  preserving child order and lexical scope. A small-stack regression exercises
  1,024 nested delays. Constant/type/data printing is still recursive.

## Remaining candidates, not reproduced

| Area | Recursive path | Why the parser limit does not settle it |
| --- | --- | --- |
| Flat source chains | `nash-can/src/expression.rs` field access and `build_tree_rec`; `nash-solve/src/solve/expressions.rs` | Parser nesting limit 64 does not limit flat operator/access chains. Equal-precedence operator construction also repeatedly scans the remaining chain. |
| Core passes | `Core::walk`, analysis collectors, hygiene, ANF, recursion rewriting, lowering and constant-sharing rewriting | ANF and generated bindings can make trees deeper than source syntax. Public pass APIs also run outside the O1 assembly stack. |
| Core diagnostics | `nash-ir/src/pretty.rs` | Snapshot and diagnostic rendering recurse independently of assembly. |
| UPLC consumers | `debruijn.rs`, `script.rs` | Public conversion and validation accept arbitrarily deep hand-built terms; O0 has no enlarged assembly stack. |
| Nested constants | Flat `encode_type` / `type_from_tags`, constant validation and pretty printing | Iterative Flat term traversal does not cover nested constant types or data. |
| Module graph | `nash-driver/src/graph.rs` cycle reporting | Import graph depth is independent of source expression nesting. |
| Type unification | `nash-solve/src/unify.rs` structural traversal | Compiler-generated types can be deeper than source types. Lower confidence until a valid input reproduces it. |

Flat term encoding and decoding already use explicit stacks and have
20,000-delay regressions. Canonicalizer SCC traversal is also iterative.
Driver compilation runs in `spawn_blocking`; the name `tokio-rt-worker`
is not evidence that it ran on an async executor thread.

## Follow-up order

1. Compile long flat operator and field-access chains through canonicalization
   and solving, not just parsing. Run overflow probes in subprocesses because
   a stack overflow can abort the process.
2. Exercise public UPLC conversion/validation and Core rendering with generated
   deep chains. Prefer explicit work stacks for straightforward traversals.
3. Probe deeply nested constant types and long import cycles independently.
4. Use valid generated programs to investigate type-unification depth.

Do not replace these checks with a global test-only `RUST_MIN_STACK` setting:
that would hide the production stack boundary.
