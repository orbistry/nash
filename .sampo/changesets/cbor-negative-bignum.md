---
cargo/nash-plutus: patch
---

Encode and decode negative Data integers below `-(2^64 - 1)` as the Haskell node does: CBOR tag 3 holds `-1 - n`, and `-2^64` is a plain negative integer.
