---
cargo/nash-constrain: minor
cargo/nash-solve: minor
cargo/nash-driver: patch
cargo/nash-report: patch
---

Infer directly from the canonical AST into the existing union-find and predicate engine. Remove the allocated constraint tree and intermediate inference Type, preserving schemes, evidence, rank ownership, recursive-group sequencing, and complete diagnostics. Pass canonical modules directly to the solver.
