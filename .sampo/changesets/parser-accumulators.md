---
cargo/nash-parse: patch
---

Accumulate function arguments and binary operators without cloning partial chains. Keep parser arena allocation linear in operator-chain length.
