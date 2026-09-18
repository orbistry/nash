---
cargo/nash-config: minor
cargo/nash-plutus: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
---

Complete validator builds with project and CLI target/trace settings, production
test-block exclusion, protocol-10 target validation, and verified script hashes.
Write single-wrapped CBOR as hex text instead of binary and track generated
artifacts for safe stale-output cleanup. Existing output directories without an
ownership manifest must be cleared of colliding artifacts or replaced with a
fresh output directory. Optimizer settings remain unavailable.
