---
cargo/nash-can: patch
---

Check every impl context together with its superclasses for contradictory representations. An impl that defines no method, such as `impl (Foo 'a, Little 'a) => Sub 'a` where `Foo` requires `Big`, skipped this check and crashed the superclass entailment check; it now gets the contradictory-representation error.
