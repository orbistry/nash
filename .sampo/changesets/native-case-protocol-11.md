---
cargo/nash-codegen: minor
cargo/nash-plutus: minor
cargo/nash-cli: patch
---

Target protocol 11 and UPLC 1.1.0 for all supported ledger languages. Lower
conditionals and boolean/list matches to native case terms, and dispatch Data
branches through a chooseData tag and native case. Preserve branch laziness and
single evaluation of scrutinees.

Enable protocol-11 builtin and constant validation, correct constant-case branch
limits and UTF-8 string costing, and encode nested Data constants without panics.
Bundled evaluator costs remain estimates rather than live ledger parameters.
