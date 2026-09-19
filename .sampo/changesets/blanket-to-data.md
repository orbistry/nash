---
cargo/nash-can: minor
cargo/nash-solve: minor
cargo/nash-report: patch
cargo/nash-codegen: minor
cargo/nash-driver: minor
---

Use one coercion-based `ToData` implementation for every Big type, including user-defined types and nested collections. Remove redundant recursive encoding implementations. Permit blanket impls in the trait's defining module while retaining overlap checks; existing concrete `ToData` impls must be removed because they overlap the blanket.
