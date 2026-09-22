---
cargo/nash-driver: minor
cargo/nash-test: minor
cargo/nash-codegen: patch
---

Use nested Choice/Group traces for property generation, strict replay, and reduction. Compose generator functions through ordinary Functor, Applicative, and Monad instances in Nash. Preserve reduced replay trees in runner outcomes. Retain concrete little type layouts when specializing generic trait helpers.
