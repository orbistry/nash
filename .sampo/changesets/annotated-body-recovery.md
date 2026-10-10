---
cargo/nash-solve: patch
---

Keep checking callers of an annotated definition against its annotation when its body has a type error. Callers now report their own mismatches and no longer receive spurious ambiguity errors. An annotation whose type variables escape still blocks its uses.
