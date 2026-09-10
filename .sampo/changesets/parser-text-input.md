---
cargo/nash-parse: minor
cargo/nash-driver: patch
---

Require UTF-8 text at the parser boundary instead of arbitrary bytes. Remove unchecked string conversions and pass source text directly from the driver.
