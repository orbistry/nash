---
cargo/nash-driver: minor
cargo/nash-codegen: minor
---

Provide unchecked `FromData` conversion for every Big type through a blanket coercion impl. Move checked `validate` into the separate `Validate` trait; migrate validation impls and trait imports to `Validate`. Unchecked conversion no longer requires validation instances for a type or its fields.
