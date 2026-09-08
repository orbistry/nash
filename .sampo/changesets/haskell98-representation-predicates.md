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

Infer Haskell 98 kinds with occurs checks and defaulting. Check storage
requirements through separate representation predicates and inline
representation annotations. Infer datatype contexts with a terminating SCC
worklist and enforce them at declarations and local or imported uses.

Preserve higher-kinded and partial alias applications, captured variables,
head-only impl coherence, superclass evidence and literal defaulting. Export
closed kinds and ordered predicate contexts through interfaces. Use
elementwise builtin-list Eq, structural Big Eq and reflexive Lift.
