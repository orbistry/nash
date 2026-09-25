---
cargo/nash-driver: minor
cargo/nash-can: minor
cargo/nash-plutus: patch
---

Consolidate Data conversion and checking in traits. Allow explicit little-type
ToData and FromData implementations alongside Big-only identity blankets. Add
independent optional Decode instances, including Cardano V3 types, and remove
the separate encoder and decoder combinator modules.

Encode Flat terms iteratively so large context decoders do not exhaust the host stack.
