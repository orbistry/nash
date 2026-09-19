---
cargo/nash-codegen: patch
---

Use dropList for Big record and constructor field offsets of two or more, retaining tailList for offset one. Share repeated constant-offset projections within their evaluation scope.
