---
cargo/nash-driver: minor
cargo/nash-cli: patch
---

Carry discovered package ownership into canonicalization and imported
interfaces so core literal defaulting works through the CLI. Preserve
application identities and reject conflicting ownership of a source URI.
