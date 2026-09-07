---
cargo/nash-source: minor
cargo/nash-ast: minor
cargo/nash-parse: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-driver: minor
cargo/nash-cli: patch
---

Replace the representation-kind lattice, retained kind obligations and
value-kind machinery with Haskell 98 kind unification and separate
representation predicates. Remove the old public kind types and metadata.
Reject infinite kinds and arrow-bound syntax; infer datatype contexts with a
terminating SCC worklist and enforce them at declaration and inference uses.

Preserve partial applications, transparent alias substitution, recursive impl
patterns, head-only coherence, superclass proofs, literal defaulting and
cross-module evidence. Export closed kinds and ordered predicate contexts in
interfaces. Use one elementwise builtin-list Eq implementation while retaining
compiler-owned structural Big Eq and reflexive Lift.

This supersedes the earlier unreleased representation-kind and value-kind
changesets. Sampo must update dependent requirements and publish source before
AST/parser, then canonicalization, constraints, solver, driver and CLI.
