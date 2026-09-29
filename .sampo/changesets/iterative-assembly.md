---
cargo/nash-ir: patch
cargo/nash-codegen: patch
cargo/nash-plutus: patch
---

Replace recursive Core and UPLC assembly traversals with heap work lists. Remove the O1 stack enlargement while preserving traversal order, generated code, traces, and lowering diagnostics.
