---
cargo/nash-codegen: patch
cargo/nash-ir: patch
---

Revisit static recursive parameter lifting after cleanup and recursive pruning so O1 reaches the same closed program in one invocation. Preserve atomic operands when lifting all-static oversaturated recursive calls and check optimizer idempotence across executable fixtures.
