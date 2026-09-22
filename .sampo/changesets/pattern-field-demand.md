---
cargo/nash-codegen: patch
---

Skip ignored Big constructor and record field projections during pattern
lowering. Reuse list tails between required fields and avoid reconstructing
unused Big-list tails while preserving strict scrutinee evaluation.
