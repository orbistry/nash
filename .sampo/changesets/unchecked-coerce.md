---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
---

Add `Builtin.coerce : 'a -> 'b` as an unchecked, representation-preserving function. Core `FromData.fromData` now defaults to unchecked coercion; use `validate` to reject malformed scalar and nested collection data.
