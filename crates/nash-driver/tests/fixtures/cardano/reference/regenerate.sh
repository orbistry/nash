#!/usr/bin/env bash
set -euo pipefail
reference=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
checkout=$(mktemp -d)
trap 'rm -rf "$checkout"' EXIT
revision=39981dd733ae276975958e40b0291ce1b24781d5
git -C "$checkout" init -q
git -C "$checkout" fetch --depth 1 https://github.com/IntersectMBO/plutus.git "$revision"
git -C "$checkout" checkout -q FETCH_HEAD
docker run --rm --platform linux/amd64 \
  -v nash-cardano-nix:/nix \
  -v "$checkout:/plutus:ro" \
  -v "$reference/..:/fixtures" \
  nixos/nix@sha256:617d914dba5384bf75adf17081583b69371031ec7defce36c34c5fa14fc819b0 \
  nix --extra-experimental-features 'nix-command flakes' \
  --option build-users-group '' \
  --option system x86_64-linux --option extra-platforms x86_64-linux \
  --option accept-flake-config true \
  --option substituters 'https://cache.nixos.org https://cache.iog.io' \
  --option trusted-public-keys 'cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= hydra.iohk.io:f/Ea+s+dFdN+3Y/G+FDgSq+a5NEWhJGzdjvKNGv0/EQ=' \
  shell --impure --expr 'let f = builtins.getFlake "path:/plutus"; p = f.__internal.x86_64-linux.project; in p.ghcWithPackages (ps: [ ps.plutus-ledger-api ps.plutus-tx ps.serialise ])' \
  --command runghc /fixtures/reference/Main.hs /fixtures/golden
