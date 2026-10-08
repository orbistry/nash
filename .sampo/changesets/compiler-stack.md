---
cargo/nash-driver: minor
cargo/nash-parse: minor
cargo/nash-report: patch
cargo/nash-cli: patch
---

Run compiler work on threads with a 128 MiB stack (`nash_driver::stack`) and remove the parser nesting limit of 64 with its `Space::TooDeep` error. Input that is too deep now ends the process with a stack overflow.
