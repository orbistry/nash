---
cargo/nash-constrain: minor
cargo/nash-solve: minor
---

Retain declared contexts on local bindings, including monomorphic annotations
and annotated recursive declarations published before their bodies. Copy
constructed context arguments with the function type at each use, and keep
annotation provenance distinct from call-site wanteds.
