---
cargo/nash-constrain: minor
cargo/nash-solve: patch
---

Add predicate IDs to inference descriptors. Preserve and deduplicate pending
obligations through unification, including descriptor changes during recursive
unification and type errors. Preserve original descriptor obligations during
scheme copy bookkeeping and restoration.
