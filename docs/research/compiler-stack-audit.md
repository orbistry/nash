# Compiler stack audit — 2026-09-29

## Stack model — 2026-10-08

This audit planned one explicit work stack for each recursive path. That
direction is superseded. Compiler work now runs on threads with one large
reserved stack (`nash_driver::stack::run`, 128 MiB), and a traversal recurses in
Elm's shape. rustc made the same change in rust-lang/rust#160535; the reasons
are in rust-lang/compiler-team#1011. The parser nesting limit of 64 is removed.
There is no stack check: input that is too deep ends the process with a stack
overflow.

The sections below are the record of 2026-09-29. The candidates and the
follow-up order are not planned work. The work stacks under "Reproduced
failures" stay until a later change converts them back to recursion.

Converted back so far: Flat term encoding and decoding (`encode_term`,
`decode_term`) recurse again, as in the imported code. `nash test` decodes
programs on its worker pool, so the CLI gives those workers the compiler stack.
The UPLC term printer, De Bruijn conversion, and target validation
(`pretty::term`, `to_debruijn`, `validate_*`) recurse again. So do the Core
traversals of `nash-ir`: `walk`, `map`, occurrence and free-variable analysis,
discard checks, hygiene, ANF, beta splicing, and Core printing; and in
`nash-codegen` lowering, constant-sharing rewriting, and recursion rewriting.

Default O1 exposed a stack overflow in the driver `ledger_contexts` fixture.
This audit distinguishes reproduced failures from recursive paths that still
need depth regressions. Passing the fixture does not establish a universal
compiler depth bound.

## Reproduced failures

- O1 cleanup after ANF overflowed on the ordinary driver blocking thread.
  The temporary 32 MiB stack has been removed. Core walk/map, occurrence and
  free-variable analysis, discard checks, hygiene/substitution, ANF, beta binding
  splicing, recursion rewriting, lowering, and constant sharing now use heap
  work lists. De Bruijn conversion and term/type/constant target validation also
  use work lists. This applies to O0 and O1 assembly without changing their
  optimization policies. A 4,096-level trace tree assembles and evaluates on a
  256 KiB stack at both levels, with equivalent results and logs; a separate
  20,000-node Core test exercises walk/map on a 128 KiB stack.
- After assembly succeeded, `nash_plutus::pretty::program` overflowed while
  rendering the named UPLC. Driver phase probes isolated this call, before
  Flat encoding. The term printer now uses explicit visit/finish frames,
  preserving child order and lexical scope. A small-stack regression exercises
  1,024 nested delays. Constant/type/data printing is still recursive.

Core term printing was also converted to an explicit work list on 30 September.
A 20,000-level term renders on a 128 KiB stack; existing formatting snapshots
remain the compatibility check. Binder type and constant payload printing are
still covered by the nested-type/constant follow-ups below.

## Remaining candidates, not reproduced

| Area | Recursive path | Why the parser limit does not settle it |
| --- | --- | --- |
| Flat source chains | `nash-can/src/expression.rs` field access and `build_tree_rec`; `nash-solve/src/solve/expressions.rs` | Parser nesting limit 64 does not limit flat operator/access chains. Equal-precedence operator construction also repeatedly scans the remaining chain. |
| IR type metadata | Derived type equality and `Builder::plutus_type` | Deeply nested types are independent of term-tree depth. |
| Nested constants | Flat `encode_type` / `type_from_tags`, constant equality and pretty printing | Iterative Flat term traversal does not cover nested constant types or data. |
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
2. Exercise deeply nested type metadata, including its diagnostic rendering.
   Prefer explicit work stacks for straightforward traversals.
3. Probe deeply nested constant types and long import cycles independently.
4. Use valid generated programs to investigate type-unification depth.

Do not replace these checks with a global test-only `RUST_MIN_STACK` setting:
that would hide the production stack boundary.
