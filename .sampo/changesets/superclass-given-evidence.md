---
cargo/nash-solve: minor
cargo/nash-driver: patch
---

Pass canonical trait tables into inference and discharge wanted predicates
through superclass givens. Preserve original context indexes and projection
paths, substitute trait parameters, and prefer explicit given evidence.
