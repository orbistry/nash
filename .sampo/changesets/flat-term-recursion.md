---
cargo/nash-plutus: patch
cargo/nash-test: minor
cargo/nash-cli: patch
---

Restore the recursive Flat term encoder and decoder. `nash_test::Config` gains `worker_stack`, and `nash test` runs its workers on the compiler stack.
