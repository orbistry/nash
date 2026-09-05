---
cargo/nash-solve: minor
---

Discharge exact wanted predicates from enclosing annotation contexts in every
Let path. Preserve use provenance and record the owning binder and context
index. Match existing types without unification, retaining nominal aliases
and normalizing equivalent record extension chains.
