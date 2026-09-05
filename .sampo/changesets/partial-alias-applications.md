---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: patch
cargo/nash-solve: patch
---

Retain unsupplied alias parameters and normalize known applications during type substitution. Preserve alias binders across interface copies and substitute free variables in filled alias bodies. Keep unresolved partial aliases at the higher-kinded inference boundary until delayed applications are implemented.
