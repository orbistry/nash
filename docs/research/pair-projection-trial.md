# Single-field pair projection trial — 5 October 2026

Status: the restricted rule was accepted and integrated into production O1 on
5 October 2026. The general rule was rejected and removed. The original trial
measurements below predate constant-folding and pair integration.

## Production integration

The restricted rule now runs inside the existing constructor/wrapper cleanup
loop before inverse cancellation, without another ANF pass. Both producer
bindings remain strict. The trial CLI was removed; normal `measure`/`check` cover
178 O0/O1 fixtures, including the 30 pair fixtures. Against the preceding 148-case
O1 baseline with constant folding, source decoding improves from 94 to 45 bytes,
1,681,280 to 878,161 CPU and 8,756 to 4,392 memory. The other 147 cases are unchanged.
Results/logs match.

## Results

Thirty synthetic and eleven compiled source fixtures compare current O1 with
O1 plus each candidate and existing cleanup. Both sides use the same lowering
mode, tested separately with ordinary lowering and production builtin/constant
sharing. That gives 164 measurements: 41 inputs × two rules × two lowering modes.
All evaluated results and trace logs match. Failed execution remains failed.

| Rule / lowering | Improve without regression | Unchanged | Regress only | Tradeoffs |
| --- | ---: | ---: | ---: | ---: |
| General / ordinary | 7 | 20 | 10 | 4 |
| General / shared | 5 | 16 | 18 | 2 |
| Cancellation only / ordinary | 5 | 36 | 0 | 0 |
| Cancellation only / shared | 5 | 36 | 0 | 0 |

Representative measurements use production sharing:

| Fixture / rule | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Literal first field / general | 80,100 → 269,995 | 600 → 932 | 13 → 15 |
| Larger body / general | 668,304 → 906,199 | 2,966 → 3,598 | 49 → 53 |
| Source decoding / either rule | 1,681,280 → 878,161 | 8,756 → 4,392 | 94 → 45 |

The general rewrite pays the runtime cost of `fstPair`/`sndPair`, including their
forces and any sharing binding. A native pair case already transfers both fields
to its branch efficiently. A shorter-looking Core expression is not necessarily
cheaper. Some ordinary-lowering fixtures save a few bytes or memory units while
using more CPU; that does not justify the general rule.

## The useful exception

Source fixture:

```text
decode value =
    case value of
        Constr pair(_, [I n, _]) -> n
        _ -> 0

decoding = decode (Builtin.constrData 0 [I 42, I 7])
```

Generated Core binds `fields`, then `d = constrData 0 fields`, then
`p = unConstrData d`. The pair case only uses the fields component. Introducing
`sndPair p` lets the existing inverse pass replace it with `fields`. List/Data
cleanup can then resolve the remaining matches to `42`. The original constructor
and decoder bindings remain strict, preserving validation and effects.

The restricted rule requires all of the following:

- A valid one-branch native pair case, two binders, no default, and matching field
  types. Exactly one field binder is used; it may be used multiple times.
- The subject is a variable resolving through aliases to saturated `unConstrData`.
- Its Data operand is a variable resolving to saturated `constrData`.
- The selected constructor operand is already a variable.

These are the existing constructor inverse rule's preconditions. The candidate
emits a strict projection let at the original case point; cleanup removes that
projection. It does not extract and re-evaluate a constructor argument. The four
synthetic constructor round-trip fixtures also improve: two valid cases save CPU,
memory and bytes; two malformed-tag-shape failures save bytes with the same errors
and logs. All other fixtures remain unchanged under the restriction.

This is measured evidence, not a proof that every program's Flat size decreases.
Reusing an earlier variable can change De Bruijn indices and encoded size.

## Safety and scope

Native `CaseKind::Pair` lowers to generic UPLC `case`, which can also handle other
runtime shapes. Replacing a tag-zero constructor case with `fstPair` would change
a successful evaluation into a failure. The general candidate therefore requires
runtime pair evidence from a literal pair, saturated `mkPairData` or `unConstrData`,
following aliases/traces. Type annotations alone are insufficient. Unknown pair
parameters are deliberately excluded.

Fixtures cover first/second fields, repeated use, both/neither fields, larger
bodies, returned lambdas/delays, discarded closures with failing subjects, traces,
ignored failing fields, invalid decoder inputs, forged pair annotations and
constructor round trips. Six guard snapshots cover invalid branch tables,
inconsistent field types and an unknown parameter. Negative constructor tags
are not executed: the current evaluator panics while converting them to `u64`.
The malformed-tag fixture instead checks a wrong runtime shape. This trial does
not change that evaluator limitation.

The rewrite requires globally unique, scoped names. It uses binding facts and
keeps subject evaluation and the projection strict. It does not repeat ANF.

## Reproduction and validation

[Raw measurements](../../tools/optimizer-perf/trials/pair-projection.json) include
fixture sources, candidate source, pipeline source, per-row Core before/after,
revision and toolchain. Measurements use Plutus V3/PV11, UPLC 1.1.0, bundled V3
costs, 100,000,000 CPU and 2,000,000 memory limits, and raw Flat bytes.

```sh
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- measure
cargo nextest run -p nash-ir -p nash-codegen
cargo run --locked --manifest-path tools/optimizer-perf/Cargo.toml -- check
```

Snapshots show the shared O0/O1 pipeline followed by each candidate and isolated
Core rewrite. Differential checks verify outcomes/logs, root type, scope and ANF.
Malformed tables preserve lowering errors. Those trial measurements left the then-current 108-row baseline unchanged.
The production integration above now uses the expanded baseline.

Historical trial validation: all 609 IR/codegen tests pass, the 108-case accepted
performance baseline matches, root and performance-workspace strict Clippy pass,
and both formatting checks pass. A separate read-only review confirmed the
report counts and embedded sources match the final code.
