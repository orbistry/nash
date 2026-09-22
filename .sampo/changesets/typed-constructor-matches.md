---
cargo/nash-codegen: patch
---

Trust solved constructor layouts when compiling patterns. Remove Data variant
checks for typed Big unions, omit dispatch for single-constructor types, and use
native integer case dispatch for dense constructor tags. Preserve strict
scrutinee evaluation and explicit matches on unrestricted Data. Add Nash source,
Core, UPLC, and execution snapshots for Bool, Unit, and nested constructors.
