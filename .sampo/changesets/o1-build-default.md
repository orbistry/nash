---
cargo/nash-config: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
cargo/nash-ir: patch
cargo/nash-plutus: patch
---

Make trace-preserving O1 optimization the default for builds and tests, with explicit O0/O1 CLI and project settings. Share the accepted optimizer with snapshots and measurements, and include direct integer representation cancellation before ANF.

Print deep UPLC term trees with an explicit work stack.
