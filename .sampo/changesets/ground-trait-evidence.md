---
cargo/nash-can: minor
cargo/nash-solve: minor
---

Resolve ground canonical trait predicates into bounded impl and reflexive Lift
evidence. Reuse canonical kind inference to prove Big without narrowing inferred
record carriers, and preserve nominal aliases and ordered impl arguments.
