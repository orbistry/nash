---
cargo/nash-solve: minor
cargo/nash-constrain: patch
---

Infer shared kind requirements at value generalization, retain captured kind roots for local instantiation, and preserve declared and inferred signatures in solved schemes and module interfaces. Reject invalid type arguments through inferred wrappers and imports.

Check body requirements together while preserving declared kind promises, including conflicting requirements on hidden local variables.

Allow representation-independent polymorphic calls on records while leaving their carrier kind unresolved until nominal record checking.

Use Builtin.bool for if conditions, as specified, instead of the ported Basics.Bool identity.
