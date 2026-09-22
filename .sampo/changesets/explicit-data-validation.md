---
cargo/nash-codegen: patch
cargo/nash-driver: patch
---

Skip payload decoding for ignored Data-pattern fields. Use direct Data
unwrappers in Base validation implementations, preserving their failure behavior
and existing element validation without redundant Data variant dispatch.
