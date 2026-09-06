---
cargo/nash-source: minor
cargo/nash-parse: minor
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: patch
cargo/nash-solve: patch
---

Retain inline type kind annotations through parsing, canonicalization and
substitution. Check their bounds and preserve annotated alias constructors.
Accept Big elements in listData so builtin-list equality can use its structural
Data route for Big elements and element equality for Const elements.
