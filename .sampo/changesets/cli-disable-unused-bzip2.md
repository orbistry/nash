---
cargo/nash-cli: patch
---

Disable unused Bzip2 ZIP support to avoid building bzip2-sys. Retain all other existing ZIP features and XZ compiler-release extraction.
