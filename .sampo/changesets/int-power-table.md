---
cargo/nash-driver: patch
---

Use a compile-time constant array for small powers of two in Int.pow2 and the base-two Int.pow path. Budget snapshots compare runtime arrays, constant arrays and modular exponentiation; constant lookup uses less CPU and memory without runtime table construction.
