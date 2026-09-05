---
cargo/nash-solve: patch
---

Reject growing trait evidence across nested helper calls. Track individual
context slots so closed evidence and calls that reset a slot remain valid.
