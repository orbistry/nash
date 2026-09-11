# Vesting validator

Build both Plutus V3 scripts from the repository root:

```sh
cargo run -p nash-cli -- build examples/vesting --trace-level verbose --compiler-traces
```

The command writes `Vesting` and `VestingParam` as UPLC text, Flat bytes,
and a single CBOR byte string containing the Flat bytes under `build/`.
`VestingParam` takes a native integer minimum-lock parameter before the three
Data arguments. The regular validator takes only the three Data arguments.

`Cardano.Tx` is a test helper. Its context is `Constr 0 [I slot, B signer]`;
it does not decode the Cardano ledger's real script context. The executable
fixtures in `crates/nash-codegen/tests/vesting.rs` cover successful and failed
claims and cancellations and record the unoptimized CPU/memory baseline.
