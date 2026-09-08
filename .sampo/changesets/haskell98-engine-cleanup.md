---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-parse: minor
cargo/nash-solve: patch
cargo/nash-driver: patch
---

Finish the Haskell 98 engine replacement by removing the obsolete overlap
compatibility callback, ignored kind-checking arguments, unused diagnostic
variants, obsolete kind-arrow parser error branches, redundant primitive arity metadata and empty solver error plumbing.
Primitive arity comes from its closed kind. Infinite-kind mismatches no
longer expose an unused inference-variable payload.

Preserve head-only coherence, representation predicates, datatype contexts,
kind contracts and Plan 03 resolution behavior. Update regression names and
explicit error assertions, and remove obsolete kind APIs from later-plan
design contracts.
