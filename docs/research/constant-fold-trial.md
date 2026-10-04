# Constant builtin evaluation trial — 4 October 2026

Status: approved and integrated into production O1 on 5 October 2026, including
the reported shared-prefix size tradeoffs. The original trial measurements below
are historical: they used `08f0831f` before the post-ANF placement correction.

## Production integration — 5 October 2026

`optimizer::optimize_with` now calls the private codegen `constant_fold` pass after
known-case cleanup. The pass folds constants and runs cleanup until stable under
one 128-attempt allowance, without another ANF pass. Evaluation and Flat sizing
use O0 assembly; explicit comptime retains its original policy. The old public
trial entrypoint and `constant-trial` command are removed.

The refreshed [148-case baseline](../../tools/optimizer-perf/baseline.json) compares
O0 with production O1. Against the previous 108-case O1 baseline, 26 cases improve
and 82 are unchanged, with identical results/logs. The additional 40 fixtures
cover the approved folding behavior, including the two shared-prefix size
tradeoffs. Reproduce with normal `measure`/`check` in the optimizer-perf workspace.
The pair-projection candidates remain outside production O1.

## Result

148 deterministic cases compare **accepted O1** with **O1 plus the trial**:
36 improve without a metric regression, 110 are unchanged, and two save CPU/memory
but increase Flat size. All results and trace logs agree. Measurements use Plutus
V3/PV11, UPLC 1.1.0, the bundled V3 default cost model, and raw Flat bytes. Runtime
evaluation is capped at 100,000,000 CPU and 2,000,000 memory units.

| Fixture | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Source list traversal | 4,764,660 → 3,225,764 | 21,472 → 15,216 | 93 → 41 |
| Source validation success | 364,689 → 111,399 | 2,033 → 732 | 47 → 10 |
| Constant addition | 181,308 → 16,100 | 602 → 200 | 10 → 6 |
| Shared prefix, distinct appended suffixes | 1,401,326 → 48,100 | 2,126 → 400 | **70 → 87** |
| Repeated literal prefix, distinct suffixes | 1,369,326 → 48,100 | 1,926 → 400 | **68 → 87** |

The size regressions are real. A 36-byte prefix previously shared by two appends
becomes two distinct 37-byte literals. The isolated-call size check approves each
fold, but it does not account for sharing across calls. The CPU/memory reduction
does not satisfy a zero-loss size policy. The initial recommendation was to refine the profitability
rule before adoption. The user subsequently accepted these tradeoffs; accounting
for shared constants remains a possible follow-up. Do not copy the candidate measurements into the accepted baseline.

[Raw measurements and fixture sources](../../tools/optimizer-perf/trials/constant-fold.json)
contain all 148 rows and tool/revision provenance. The historical trial command has been retired. Check current production with:

```sh
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- measure
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- check
```

## Rewrite and limits

`nash_ir::constant_fold::reduce` calls a supplied evaluator for exactly saturated
builtins with literal arguments, including aliases of literal bindings. The IR
has no dependency on codegen. It retains original strict bindings for cleanup,
preserves the outer type view, and requires scoped, globally unique names.

The original `nash_codegen::constant_trial` started with accepted O1 and repeated folding and
existing ANF cleanup while the tree changes. It never repeats ANF. Each invocation
allows 128 evaluation attempts, including failures. Each call gets 1,000,000 CPU
and 10,000 memory units. Input/output preflight allows at most 4,096 payload bytes,
1,024 nodes and depth 64 across constants, Data and type annotations. This runs
before recursive runtime costing and Flat encoding. List/pair payloads must
match their declared types.

The trial covers integer arithmetic/comparisons, byte append/slice/length and
comparisons, string append/equality, UTF-8 conversion, list/pair operations, and
selected Data construction/decoding/equality/serialization. It excludes Trace,
cryptography, version-sensitive ConsByteString, panic-prone IndexByteString and
ConstrData, and newer array/value/bit operations. The exact allowlist is in
`crates/nash-codegen/src/constant_fold.rs`.

A failed, unsupported, over-budget or oversized evaluation leaves the original
call intact. Runtime errors never become compiler errors. A successful replacement
must not increase the isolated expression's Flat encoding; this gate needs the
sharing refinement described above. Explicit user `comptime` keeps its existing
budget and diagnostic behavior through the same closed-term evaluator.

## Existing serializer limitation

`nash-plutus/src/flat/data.rs` encodes multi-limb negative Data integers with CBOR
tag 3 and their absolute magnitude, while tag 3 denotes `-1 - magnitude`. Its
matching decoder hides the discrepancy in local round-trip checks. In particular,
folding `IData (-(2^64 + 1))` into a serialized literal could change ledger meaning.
The trial rejects negative Data integers with more than 64 magnitude bits, including
nested input/output Data. This serializer defect needs a separate correction with
an external-format oracle; this trial does not modify serialization.

## Historical trial validation

- 607 IR/codegen nextest tests pass, including existing explicit-comptime tests.
- 40 trial fixtures have paired accepted O0/O1 and candidate snapshots with
  differential results/logs, type, scope and ANF checks.
- Four boundary snapshots cover exhausted CPU, memory, attempt and input-size
  limits. Structural checks reject malformed containers, excessive type depth and
  unsupported large negative Data. The expanding Data-list fixture stays unchanged.
- Root and isolated performance-workspace strict Clippy pass; formatting and diff
  checks pass.
- The accepted 108-row performance baseline still matches exactly. No existing
  accepted snapshots or baseline rows were replaced.

The preceding validation section describes the historical trial; current
integration validation is recorded below.

## Integration validation

The final full workspace run passes 3,834 tests. Root and performance-workspace
strict Clippy pass, formatting checks pass, and the refreshed 148-case performance
baseline matches. All 170 changed existing snapshots with O0/O1 sections retain
identical unoptimized Core and UPLC. Added boundary fixtures verify that failures
consume the shared attempt allowance across cleanup iterations and unsafe negative
Data results remain runtime calls. A separate read-only review confirms there is
no optimizer re-entry and ANF normalization still runs once.
