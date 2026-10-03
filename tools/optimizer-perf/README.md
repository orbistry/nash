# Explicit optimizer performance checks

This unpublished package has its own Cargo workspace and lockfile. Root
`cargo test`, `cargo test --workspace --all-features`, and
`cargo nextest run --workspace --all-features` do not discover it. There are no
benchmark targets, ignored performance tests, feature gates or CI jobs.

Run from the repository root:

```sh
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- check
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- measure
```

`check` compares every row, including inputs, results, trace logs, CPU, memory and
raw Flat size, against `baseline.json`. Both improvements and regressions require
review; none of the metrics has priority. It exits nonzero on drift, a semantic
mismatch, a compilation failure or an exhausted budget. `measure` prints JSON
without changing any baseline. Debug/release profiles produce the same ledger
budgets: these are CEK costs, not Rust wall-clock benchmarks.

The 108 rows include the original 20 covering list traversal, static recursion, Data matching and
misses, field decoding, validation success/failure, the real base Logic helpers,
and six ledger scenarios each for the existing Vesting and VestingParam source
fixtures, plus constant-prefix two-use, cold and loop regressions and 85 representation
cancellation cases shared with semantic tests. Validators receive one V3 ScriptContext containing TxInfo, redeemer and spending
datum; the optional minimum-lock parameter is applied off-chain first. Times are
POSIX milliseconds from the validity-range lower bound. Validator CPU/memory
include parameter and context application;
validator bytes exclude those arguments. Ordinary expression fixtures include
their inputs in the measured program.

The `before` pipeline is O0 (recursion rewrite and lowering). The `after` pipeline
is the accepted static lifting, pre-ANF unused-parameter removal, direct native-constructor folding and representation inverse cancellation, one ANF normalization and rules 1+2+3+4 plus safe dead-binding, recursive-reachability, representation cancellation, force/delay, known-Boolean/integer/bytes, bound-constructor, known-constructor field, known-list, literal-Data and IData/BData/ListData/MapData/ConstrData producer cleanup,
then recursion rewrite, binder freshening and lowering with both Chunk 5 sharing steps. No second normalization
or ANF-dependent cleanup runs after recursion rewriting. Rule 3 was accepted on 27 September 2026. These figures record current behavior, including overhead
from ANF; they are not a claim that the incomplete optimizer beats O0 everywhere.

## Explicit baseline updates

```sh
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- record /tmp/proposed-perf.json
diff -u tools/optimizer-perf/baseline.json /tmp/proposed-perf.json
```

`record` requires a new path and refuses to overwrite any existing file. Review
changed inputs, outcomes and costs before explicitly copying the proposal over
`baseline.json`. Snapshot acceptance never updates this file. `check PATH` can
check a proposed or deliberately corrupted baseline without changing the original.
The initial baseline records measured accepted-pass behavior, not regression limits
selected from a performance policy.

Reports embed all fixture source inputs and record the Plutus/UPLC versions, bundled default V3 cost model, evaluation
limits, repository revision and Rust version. The nested Cargo.lock pins dependency
versions. Keep source and lockfile with a report to reproduce it. Revision and Rust
version are provenance, not equality gates: exact row/settings comparison detects
cost drift without rejecting unrelated commits or toolchain updates.

## Temporary experiments

```sh
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- experiment /tmp/Experiment.nash
```

Supply a standalone Nash module with a monomorphic, ground-valued `main` (or one
that fails). It can import Primitive, Builtin, the integration fixture Literal and
Lift modules, base Logic and Eq, and the integration fixture Cardano.Tx. It is not a full
project loader. Apply any returned functions inside `main`; the runner rejects
opaque function/delay results instead of pretending to compare them semantically.
The JSON embeds the experiment source. Nothing adds it to permanent baselines.

For alternate optimization algorithms, temporarily edit `accepted` in
`src/main.rs`, run an experiment, record the decision and remove the temporary
change/source. Do not update permanent baselines to bless a trial. Use the same
explicit runner for threshold sweeps; keep sweep scripts outside Cargo discovery.

Each invocation has a 120-second process watchdog covering source compilation,
optimization and evaluation. Each evaluation is capped at 100,000,000 CPU and
2,000,000 memory units. Experiment source is limited to 64 KiB. Budget exhaustion
is a harness failure, even when both pipelines exhaust the budget. These limits
do not bound Cargo's build step; that step does not execute experiment source.

## Maintainer checks

The isolated package needs its own formatting and lint commands:

```sh
cargo fmt --manifest-path tools/optimizer-perf/Cargo.toml --check
cargo clippy --locked --manifest-path tools/optimizer-perf/Cargo.toml --all-targets --all-features -- -D warnings
```

Verify isolation after changing workspace manifests using root `cargo metadata
--no-deps --format-version 1`, `cargo test -- --list`,
`cargo test --workspace --all-features -- --list`, and
`cargo nextest list --workspace --all-features --message-format json`. None may
include the `nash-optimizer-perf` package/binary. Run root tests normally as well.

## Completed experiments

Prior per-optimization experiment executables were removed on 28 September 2026.
Their measured findings and keep/defer decisions remain in `plans/08-optimizer.md`;
accepted behavior retains semantic snapshots and the explicit budget baseline.
Use the temporary experiment runner above for future trials, then remove trial
source once its decision is recorded.

The Core pipeline is shared with production O1 in `nash-codegen::optimizer`.
Builds and tests default to O1; these measurements still compare explicit O0 and O1.
