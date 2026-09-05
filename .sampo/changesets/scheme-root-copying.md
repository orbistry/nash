---
cargo/nash-solve: patch
---

Copy multiple scheme roots with one shared variable map and restore every
touched original directly. Preserve sharing within an instantiation, freshen
generalized variables between uses, and retain outer variables unchanged.
