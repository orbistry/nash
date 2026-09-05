---
cargo/nash-solve: patch
cargo/nash-constrain: minor
---

Preserve declared quantifiers when instantiating typed recursive calls.
Report PolymorphicRecursion when a direct or mutual recursive call wraps
evidence from its own group in impl evidence.
