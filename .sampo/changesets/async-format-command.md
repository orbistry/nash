---
cargo/nash-cli: patch
---

Process format inputs concurrently with asynchronous filesystem and stream I/O, and keep formatting work off Tokio executor threads.
