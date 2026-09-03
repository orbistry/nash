# nash

A purely functional language with Elm/Haskell syntax that compiles to
Untyped Plutus Core. Successor to Aiken: type classes, higher-kinded types,
explicit Big (`Data`) vs little (native UPLC) representation types, macros,
compile-time evaluation, property-based tests.

- Design: [`docs/overview.md`](docs/overview.md) and [`docs/`](docs/)
- Plans: [`plans/`](plans/)
- Status: [`SPEC.md`](SPEC.md)

## Taste

```elm
validator module Vesting exposing (main)

type Datum = Datum { owner : Bytes, deadline : Int }

@derive(Eq, Show, ToData, FromData)
type Redeemer = Claim | Cancel

main : Datum -> Redeemer -> Data -> unit
main datum redeemer ctx =
    case redeemer of
        Claim -> assert (lower datum.deadline < currentSlot ctx)
        Cancel -> assert (signedBy ctx datum.owner)

tests
    import Fuzz exposing (int)

    prop "deadline is never negative" =
        let d via int in
        do
            assert (lift d >= lift 0)
```

## Development

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo insta test
```

### Changesets

We use [sampo](https://github.com/bruits/sampo) for versioning and releases.
When you make a notable change, add a changeset:

```sh
sampo add
```

Only list crates you actually changed; sampo bumps dependents.

### Release flow

1. Push to `main` with a changeset file
2. Sampo CI creates/updates a release PR that bumps versions and generates changelogs
3. Merge the release PR
4. Sampo publishes crates to crates.io and pushes version tags
5. Tags trigger cargo-dist to build and publish:
   - GitHub Releases with platform binaries
   - Homebrew formula (`brew install orbistry/tap/nash`)
   - npm package (`npx @nash-script/cli`)
