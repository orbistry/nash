---
cargo/nash-solve: patch
---

Do not select an impl for a trait constraint whose argument is an infinite type; the occurs check of the binding reports the type, as in Elm. Two impls that are disjoint on finite types can both match an infinite type, which crashed the compiler, and an impl that matches it at every depth gave a false impl-resolution-limit error.
