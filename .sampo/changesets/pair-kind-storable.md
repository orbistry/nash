---
cargo/nash-ast: minor
cargo/nash-can: patch
---

Relax the `pair` kind to `Storable -> Storable -> Const` so `unConstrData : Data -> pair int (list Data)` kind-checks; construction stays restricted to `mkPairData`.
