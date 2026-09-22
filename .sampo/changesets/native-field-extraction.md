---
cargo/nash-codegen: patch
cargo/nash-driver: patch
---

Use shared native list and pair cases for field extraction and Base traversal.
Reuse case-bound tails across adjacent accesses and use dropList for remaining
gaps of two or more, preserving evaluation order and record update suffixes.
