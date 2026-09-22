---
cargo/nash-driver: minor
cargo/nash-test: minor
---

Use little prng state with native bytes, integers, and lists for property testing.
Pass native constructor terms between generators and the runner, removing Data
wrapping from seeded draws and replay.
