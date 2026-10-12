---
cargo/nash-constrain: minor
cargo/nash-solve: patch
---

Restore Elm's constraint tree: `nash-constrain` generates it from the canonical AST and `nash-solve` walks it, in place of direct inference. An annotated definition now publishes its declared type: a call with more arguments in the same module no longer replaces an alias in its annotation, so importers find the impls of that alias.
