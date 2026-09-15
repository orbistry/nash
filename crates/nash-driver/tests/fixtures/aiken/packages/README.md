# Offline Aiken acceptance packages

The official archives contain unchanged `lib/`, `aiken.toml`, `LICENSE`, and
`aiken.lock` where the release has one. Each has a single archive-root directory.

| Package | Tag | Source commit | ZIP SHA-256 |
|---|---|---|---|
| aiken-lang/stdlib | v3.1.0 | 7d5cee54b2bb4eea211ae3bd806c7c39e5fd899d | 1a42f0b057e7ca147def09f4d7969701ae64bbcf5176f9d184dd9152dd5d6c92 |
| aiken-lang/fuzz | v2.2.0 | 06874926ec70747f3fc4e2b9364ee9e1393441cc | 6044ed28941b5917e944ca8d25478e9632def1f45b85262cdd5519594e382eb7 |

The fuzz release deliberately retains manifest version `main`, compiler
`v1.1.17`, and its own stdlib `v2.2.0` declaration. The root's flat resolved lock
selects stdlib `v3.1.0`; dependency manifests do not override it in Aiken 1.1.23.
The `sample/direct` and `sample/transitive` archives are small local test packages.

`cargo test -p nash-driver --test aiken_projects` prepares these packages offline
in temporary normal Aiken build directories, then uses the public project and
compiler paths. The standard-library check is not ignored.
