---
cargo/nash-driver: patch
cargo/nash-codegen: patch
---

Define `toData` and `fromData` directly in their blanket impls using inline Big bounds, instead of trait defaults. Conversion behavior is unchanged.
