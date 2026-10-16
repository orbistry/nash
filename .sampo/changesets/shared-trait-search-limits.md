---
cargo/nash-ast: minor
---

Add `nash_ast::head::WORK_LIMIT` and `nash_ast::head::DEPTH_LIMIT`, the one definition of the trait search limits. Canonicalization, inference and code generation now use them.
