---
cargo/nash-ast: minor
cargo/nash-can: patch
cargo/nash-report: patch
cargo/nash-solve: patch
---

Reject contradictory representation contexts on type applications, such as `(Big ('f 'a), Little ('f 'a))`, with the existing contradictory-representation error. They used to reach impl selection, where an empty proven set matched every disjoint impl. Export `nash_ast::same_type` for location-free type comparison.
