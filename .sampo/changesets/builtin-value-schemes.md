---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-solve: patch
---

Export the specified Builtin value schemes with checked representation predicates, including
the Storable list and pair APIs. Keep backend identifiers symbolic so the AST
does not depend on the Plutus runtime. Normalize builtin unit annotations and
impl heads to the same unit type as ().
