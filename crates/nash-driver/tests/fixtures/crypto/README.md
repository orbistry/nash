# Crypto reference vectors

`../base-traits/Crypto.nash` runs the public Nash helpers against these vectors:

- Hashes: empty input and ASCII `abc`. Python's standard `hashlib` independently
  computes SHA-256, SHA3-256, Blake2b-224/256 and RIPEMD-160 (`hashlib.new`).
- Keccak: the existing Plutus conformance empty and 200-bit input vectors from
  `crates/nash-plutus/tests/conformance/builtin/semantics/keccak_256`. The nonempty
  vector originates in the Keccak round-3 ShortMsgKAT_256 dataset, as recorded in
  that fixture. It is not the standardized SHA3 variant.
- Ed25519: Plutus conformance verifyEd25519Signature vectors 01 and 05.
- ECDSA: verifyEcdsaSecp256k1Signature vectors 01 and 03. Those fixtures explicitly
  SHA-256 the empty message before verification; the Nash fixture uses the same
  precomputed digest. Vector 03 checks high-S rejection.
- Schnorr: verifySchnorrSecp256k1Signature vectors 16 and 06 (BIP340), including
  a valid one-byte message and a signature with odd-Y R that must not verify.

The existing conformance suite executes all 83 signature vectors through the
Plutus evaluator. Run it with:

```sh
cargo test -p nash-plutus --test conformance builtin_semantics_verify
```

Public-helper snapshots use the shared driver test runner and call compiler
functions directly. They do not invoke the Nash CLI.
