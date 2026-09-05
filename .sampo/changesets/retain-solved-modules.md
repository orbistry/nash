---
cargo/nash-can: minor
cargo/nash-driver: patch
---

Retain canonical module nodes and solved trait evidence together for the duration of a build. Separate the canonicalizer's interface lookup lifetime from arena data so dependent modules borrow interfaces without copying nodes.
