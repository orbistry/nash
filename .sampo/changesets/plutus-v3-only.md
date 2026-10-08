---
cargo/nash-config: minor
cargo/nash-codegen: minor
cargo/nash-test: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
---

Remove the Plutus V1 and V2 targets. Nash compiles validators against the V3 script context only, so a V1 or V2 language tag produced an invalid validator. The `plutusVersion` project setting, the `--plutus-version` option, `nash_config::PlutusVersion`, `assemble_core_for_version`, `TestProgram::plutus_version` and the version parameters of `assemble_core_with_options` and `compile_tests*` are gone. Script hashes always use the V3 language tag.
