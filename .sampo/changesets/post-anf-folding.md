---
cargo/nash-codegen: patch
---

Run constructor folding and representation cancellation only in the post-ANF cleanup loops, following generated bindings and opportunities exposed by inlining. Remove the early direct-expression passes.
