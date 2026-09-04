---
cargo/nash-driver: patch
---

Preserve module dependency order while loading sources so imports compile after their dependencies. Retain failed reads in their original positions.
