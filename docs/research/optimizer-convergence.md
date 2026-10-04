# O1 convergence follow-up

The earlier removal of attempt and resource caps fixed two known cases, but did
not include static recursive parameter lifting in the cleanup fixed point.
A second O1 invocation could therefore still remove work.

## Reproduced causes

- A recursive argument such as `if True then x else 0` initially fails static
  parameter detection, which requires the recursive argument to be the parameter
  variable. Cleanup reduces it to `x`, but lifting had already finished.
- A recursive group with two members initially fails the singleton requirement
  for lifting. Pruning an unreachable member exposes a singleton after lifting.

The source regression is `cleanupExposesStatic` in the performance workloads.
An explicit `next` binding keeps the recursive call saturated after ANF:

```text
loop x n =
    if n == 0 then x
    else let next = n - 1 in loop (if True then x else 0) next
main = loop 42 3
```

Before this fix, one O1 invocation produced 46 Flat bytes and two produced 39.
This was a missed optimization caused by pass ordering, not an accepted limit.
The existing 194 performance cases did not expose it.

## Correction

Keep initial static lifting before the sole ANF normalization, then repeat static
lifting, unused-parameter removal, constant folding and cleanup until pointers
are unchanged. Each lift removes unchanged parameters from the recursive worker;
no rule restores them. This preserves the existing static-lifting eligibility
rules while revisiting opportunities exposed by other accepted rewrites.

For an all-static oversaturated self-call, bind the forced worker result before
applying extra arguments. That retains atomic call operands and keeps the force
at its original execution point. Existing cleanup flattens generated binding
prefixes. No second ANF pass is needed.

The shared executable fixture harness now checks closed Flat equality after one
and two O1 invocations, after rendering snapshots. New regressions cover both
exposure paths plus all-static oversaturation with traces and nested bindings.

## Measurements

Same Plutus V3 cost model and Flat measurement as the existing performance tool.
The source returns 42 without traces in all cases.

| Pipeline | CPU cost | Memory cost | Flat bytes |
| --- | ---: | ---: | ---: |
| O0 | 2,737,056 | 14,010 | 66 |
| Previous O1 | 2,161,056 | 10,410 | 46 |
| Corrected O1 | 1,921,056 | 8,910 | 39 |

All 194 existing rows are unchanged. The new source case extends the explicit
performance baseline to 195 cases. A separate audit applies O1 twice to each
performance input and compares complete closed Flat output, not just its size.

## Validation

All 3,864 workspace nextest tests pass. The final fixture-reuse cleanup and shared
assertion placement also pass all 557 codegen tests. Strict root and performance
workspace Clippy, formatting, and whitespace checks pass. All 195 performance
cases match the reviewed baseline, and the separate two-invocation audit reports
identical complete closed Flat output for every input. Existing fixture outputs
remain unchanged; the new regressions demonstrate the previously missed rewrites.

## Historical budget distinction

The removed 1,000,000 CPU / 10,000 memory allowance was specific to O1's automatic
constant-builtin folding. It reused `comptime::eval_closed_budget`, but explicit
`comptime` called `eval_closed` with the separate default of 10,000,000,000 CPU /
14,000,000 memory. Failed automatic folding retained the runtime call; failed
explicit `comptime` reported a compilation error. O1 now calls the unbudgeted
constant-builtin evaluator directly. Explicit `comptime` is unchanged.
