# Recursive unused parameters — 5 October 2026

The recursive rule is integrated into production O1 after static lifting and
before its single ANF pass. It follows bare-variable forwarding dependencies to
a liveness fixed point within each group. Other parameter uses remain live,
including uses inside strict compound arguments. Unsupported call shapes leave
the entire group unchanged. Non-atomic arguments retain source evaluation order.

All-unused self and mutual workers become delayed values. The mutual dispatcher
executes their body only when forced; mixed delayed/function groups and returned
closures keep captures and repeated execution. Static parameter indices are
remapped, and call sites retain their own type views. No whole-program signature
fixed point, additional ANF pass or general recursive inlining was added.

## Measurements

Compared previous production O1 with this rule across 194 cases: the existing
178, 14 Core fixtures and two compiled source workloads. Six improve with no
regression and 188 are unchanged. Every O0 measurement, result and trace log is
unchanged. The original 178 O1 cases are all unchanged. The self-recursive source
case is unchanged because existing static lifting already removes that forwarding
parameter; the mutual source case benefits from the new dependency analysis.

| Case | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Mutual source | 3,196,554 → 2,876,554 | 16,542 → 14,542 | 93 → 81 |
| Mutual forwarding | 2,529,056 → 2,209,056 | 12,710 → 10,710 | 101 → 82 |
| Recursive trace order | 3,926,044 → 3,318,044 | 19,402 → 15,602 | 160 → 134 |
| Recursive failure order | 272,637 → 272,637 | 167 → 167 | 156 → 130 |
| All-unused mutual | 1,731,300 → 1,635,300 | 8,930 → 8,330 | 97 → 89 |
| Mixed delayed/function | 4,671,636 → 3,727,636 | 23,848 → 17,948 | 115 → 89 |

[Raw comparison](../../tools/optimizer-perf/trials/recursive-parameters.json)
embeds fixtures, previous/current parameter reduction and the recursion encoding
diff. The normal baseline now retains all 194 O0/O1 cases. Measurements use the
same Plutus V3/PV11, UPLC 1.1.0 and cost/budget settings as the prior baseline.

## Validation

Semantic snapshots cover forwarding and consumed parameters, permuted mutual
slots, strict traces/failures, cold/repeated all-unused calls, mixed groups,
compound dependencies, partial/escaping/oversaturated calls, static indices,
independent type views, returned closures and explicit divergence. Divergence is
rendered without execution. Independent checks cover outcomes/logs, types,
hygiene, ANF and pass pointer idempotence. Source snapshots cover self-recursive
static lifting and mutual forwarding.

Performance checks stay in the separate workspace and never run under normal
`cargo test` or root nextest commands.

Validation completed: all 3,852 workspace nextest tests pass, root and isolated
performance-workspace strict Clippy pass, formatting/whitespace checks pass, and
all 194 explicit performance cases match. Existing O0 snapshot sections are
unchanged. A read-only review found no correctness blocker or report mismatch.
