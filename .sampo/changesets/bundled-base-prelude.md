---
cargo/nash-ast: minor
cargo/nash-can: minor
cargo/nash-constrain: patch
cargo/nash-solve: patch
cargo/nash-ir: patch
cargo/nash-nitpick: patch
cargo/nash-codegen: minor
cargo/nash-report: patch
cargo/nash-driver: minor
cargo/nash-language-server: patch
cargo/nash-cli: patch
---

Embed compiler-versioned Base sources in the driver and make Prelude available automatically without a declared dependency, download, or installed source directory. Keep implicit imports out of source syntax and snapshots.

Reserve `Builtin` for actual Plutus functions. Move compiler-owned types, constructors, representation traits, and unchecked `coerce` to `Primitive`, with the foundation package renamed to `nash/base`.

Check bundled Base through in-process compilation tests and remove CLI subprocess tests.
