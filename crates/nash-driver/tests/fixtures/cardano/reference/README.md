# Plutus V3 reference fixtures

`Main.hs` uses `PlutusLedgerApi.V3`, `PlutusTx.toData`, and
`Codec.Serialise.serialise` from IntersectMBO/plutus revision
`39981dd733ae276975958e40b0291ce1b24781d5` (plutus-ledger-api 1.70.0.0).
It was executed with GHC 9.6.7 in Docker through OrbStack.

These synthetic contexts exercise the actual Haskell codecs. Short fixture
hashes are intentional; the fixtures are not ledger-valid transactions.

Run `./regenerate.sh` to rebuild the committed `../golden/*.cbor` files.
It downloads the pinned upstream source and uses its locked Nix dependencies.
The `nash-cardano-nix` Docker volume retains the build cache. This generator
is a development tool; Cargo tests use the committed bytes and require no
Haskell, Docker, network, or Nash CLI process.

The seven contexts cover all ScriptInfo variants and both spending datum
cases, all certificate and governance-action tags, both credentials,
staking hashes and pointers, output datum variants, optional scripts,
treasury options, votes, nested maps, and negative mint amounts. Nash checks
its independently constructed typed values against the decoded contexts and
compares `serialiseData` directly with the original Haskell bytes.

The interval oracle enumerates five endpoints (negative infinity, -1, 0, 1,
positive infinity), both closure flags, and every endpoint pair. It emits
100 emptiness results, 500 membership results, and 10,000 containment results.

The value oracle compares quantities, signed scaling, missing assets, addition,
and nonnegative containment with Haskell. Native Nash values remove zero
entries and reject negative containment; Haskell's map-based Value preserves
zero entries and permits negative comparisons. Separate Nash snapshots cover
those native builtin contracts.
