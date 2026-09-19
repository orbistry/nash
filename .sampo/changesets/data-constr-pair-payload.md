---
cargo/nash-ast: minor
cargo/nash-codegen: minor
cargo/nash-driver: patch
---

Make Data.Constr accept one `pair int (list Data)` payload in expressions and patterns. Require explicit pair destructuring for the tag and fields, lowered with native UPLC case. Update Base helpers and source snapshots.
