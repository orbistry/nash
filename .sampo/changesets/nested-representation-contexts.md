---
cargo/nash-solve: patch
---

Report a local annotation whose representation constraints contradict an enclosing annotation, such as `Little 'a` inside `Big 'a => ...`, as a contradictory-representation error. The two annotations share `'a`, and their combined constraints used to reach impl selection and crash the compiler. The error no longer lists a requirement that another listed requirement implies, such as `Storable` next to `Big`.
