---
cargo/nash-constrain: minor
cargo/nash-solve: patch
---

Report ambiguous trait variables that disappear from a definition's full
type. Check both inferred and annotated bodies at their generalization
boundary, preserve outer captures, and identify the innermost definition.
