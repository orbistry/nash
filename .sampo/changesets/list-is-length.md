---
cargo/nash-driver: minor
---

Add List.isLength for Big or little lists and integer counts, using dropList
and a singleton pattern rather than traversing the full list to count it.
