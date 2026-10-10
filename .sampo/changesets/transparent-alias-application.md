---
cargo/nash-solve: patch
---

Unify a type-variable application such as `'f 'a` with a transparent alias through the alias body when the alias's own parameters cannot supply the applied arguments. `type alias Xs = List Int` now works with `List.length`; record aliases remain nominal.
