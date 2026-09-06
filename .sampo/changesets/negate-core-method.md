---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: minor
cargo/nash-solve: patch
---

Desugar prefix negation to the checked core Num.negate method with evidence on its generated method node. Remove the canonical Negate variant, fabricated method scheme, and dedicated negation constraints. Report a missing core Num method during canonicalization.
