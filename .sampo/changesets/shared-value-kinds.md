---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: minor
---

Retain value-annotation kinds under one shared binder across definitions, constructors, trait methods, and impl specialization. Preserve correlations between constructor kinds and their argument kinds, including owning-trait restrictions after removing the impl's dictionary predicate.

Carry declared kind roots through constraints and solver bindings in annotation order, preserving captured variables and including these roots in scheme copying and quantifier discovery.

Infer shared value kinds over solver type equivalence classes and prove rigid requirements without narrowing the declaration's kind binder.

Check instantiated kinds on declared local and imported value uses before publishing solved output. Report source-located BadKind errors for invalid concrete type arguments.
