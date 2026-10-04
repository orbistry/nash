# Application fusion and native packing — case 1

The initial case below was measured on 5 October 2026 against `c2259d95` in an
isolated checkout. The user then requested production implementation under
Plan 08, including late UPLC binding cleanup. The integrated results follow the
initial evidence. O2 remains unimplemented.

The user clarified that application fusion includes the native `Case`/`Constr`
replacement. This trial therefore measures both steps separately and together.

## Source and generated code

The source defines a recursive function that returns an addition function:

```elm
choose : int -> int -> int -> int
choose n =
    if Builtin.equalsInteger n 0 then
        \x y -> Builtin.addInteger x y
    else
        choose (Builtin.subtractInteger n 1)

main : int
main =
    let
        p = choose 3 20
    in
    p 22
```

Current O1 retains the following call sequence after the recursive definition
(binder identifiers omitted here for readability):

```text
let anf : int -> int -> int = choose 3 in
let p : int -> int = anf 20 in
p 22
```

Two adjacent fusions produce `choose 3 20 22`. Ordinary lowering then emits
`(((choose 3) 20) 22)`. Native packing instead emits:

```text
case (constr 0 [3, 20, 22]) [choose]
```

The recursive implementation of `choose` remains present. This is compiled
source evidence, rather than a hand-built application example. The complete
module, exact generated Core, all before/after UPLC, costs, compiler settings and
reproduction patch are in the [raw trial](../../tools/optimizer-perf/trials/application-packing-case1.json).

## Measurements

Each measurement includes the same source inputs in the program. Size is raw
Flat bytes; CPU and memory are Plutus V3/PV11 ledger costs, not wall-clock time.

| Pipeline | CPU | Memory | Flat bytes |
| --- | ---: | ---: | ---: |
| O0 | 2,470,264 | 11,712 | 61 |
| Current O1 | 2,278,264 | 10,512 | 53 |
| O1 + packing only | 2,278,264 | 10,512 | 53 |
| O1 + fusion only | 2,182,264 | 9,912 | 48 |
| O1 + fusion + packing | 2,166,264 | 9,812 | 50 |

Fusion removes two binding/application wrappers. Packing alone cannot see across
the retained bindings. After fusion it packs exactly one three-argument call,
saving another 16,000 CPU and 100 memory units at a cost of two bytes compared
with ordinary fusion. The combined result saves 112,000 CPU, 700 memory units
and three bytes compared with current O1. No metric has been used as an automatic
acceptance or rejection gate.

## Scope and checks

The Core trial matches only an adjacent, single-use application binding followed
immediately by an application of that binder. It requires atomic operands and
matching RHS, binder and variable type views. The concatenated arguments retain
their original order. Hidden uses in lambda or delay bodies count against the
single-use requirement.

The lowering trial packs only calls whose function and arguments lower to UPLC
values: variables, constants, lambdas, delays or bare builtins. It does not treat
`Force(Builtin)` as a value. Evaluating the constructor fields before the function
and its application stages therefore crosses no effectful or divergent argument
computation in well-scoped code. Unknown function arity is allowed by this narrow
proof. This is an O1 candidate; it does not depend on disabling traces.

The trial's two-argument minimum is experimental, not an adopted arity policy.
Only the three-argument case was packed in this fixture. It does not establish
cost tradeoffs for other arities, first-class builtins or real validators.

All four variants return integer `42` with no logs, and their O0 results and costs
are identical. Independent checks pass for ANF, closed binder hygiene, result
type and fusion pointer idempotence. For this source, both trial pipelines produce
identical closed Flat bytes after one, two and three optimizer invocations.
Strict Clippy passes for the isolated trial workspace. With trial flags absent,
all 195 performance cases match the unchanged production baseline.

The initial prototype exists only in a temporary checkout. The reproduction patch is
embedded in the raw trial; it is not linked into normal Cargo tests or production
code. That trial did not update a permanent baseline. The subsequent integration below
adds trace/failure cases and updates the reviewed production baseline.


## Integrated O1 scope

All three operations are now part of Plan 08: adjacent application fusion in Core,
late UPLC binding cleanup, and values-only native application packing. The common
`lower_optimized` entry point serves production, shared snapshots, source fixtures
and performance measurements. It runs existing builtin/constant sharing, then
late cleanup and packing to a pointer fixed point. O0 and the isolated sharing
entry points are unchanged.

Cleanup removes identity applications while preserving argument evaluation,
substitutes single-use values, discards unused values and cancels force/delay.
It keeps computations strict, including shared forced builtin references and
partial builtin applications. Capture checks support shadowed names conservatively.
Multi-use lambda substitution is excluded, so recursive self applications do not
unfold. The pass requires closed, well-scoped named input.

Packing starts at three value arguments: three or more Apply nodes can be replaced
with two native nodes. This is a cost-derived choice, with no size-growth cutoff.
A four-argument spine extends an already packed prefix. A non-value fourth argument
stays outside the prefix so earlier applications still run before it.

### Corpus measurements

The baseline contains 199 cases: the existing 195 plus four compiled source cases.
Every previous O0 row is unchanged. All results and trace logs match O0.
Of the previous rows, 175 are unchanged and 20 change: 14 consume less CPU and
memory, and none consume more. Seven scripts shrink; 13 rows grow in size. Twelve
of those rows are scenarios for two vesting scripts, rather than twelve different
scripts. The remaining growth is an oversaturated recursive-parameter fixture.

| Existing case | CPU before → after | Memory before → after | Flat bytes before → after |
| --- | ---: | ---: | ---: |
| Mutual recursive source | 2,876,554 → 2,732,554 | 14,542 → 13,642 | 81 → 67 |
| Vesting claim after deadline | 9,093,501 → 8,581,501 | 48,616 → 45,416 | 821 → 822 |
| Vesting cancel signed | 7,915,016 → 7,467,016 | 42,850 → 40,050 | 821 → 822 |
| Parameterized vesting claim | 9,354,709 → 8,842,709 | 49,618 → 46,418 | 827 → 829 |
| Mutual consumed parameter | 2,529,056 → 2,193,056 | 12,710 → 10,610 | 99 → 74 |
| Oversaturated parameter fixture | 304,100 → 288,100 | 2,000 → 1,900 | 22 → 24 |

Failing vesting scenarios can have unchanged runtime costs while carrying the
same larger script. These size tradeoffs are retained explicitly; they are not
hidden by a growth gate. The [full before/after report](../../tools/optimizer-perf/trials/application-cleanup.json)
contains every changed row, both complete reports, fixture sources and cost settings.

| New source case | O1 result | Trace order | CPU | Memory | Flat bytes |
| --- | --- | --- | ---: | ---: | ---: |
| Staged application | 42 | none | 2,166,264 | 9,812 | 50 |
| Application stage traces | 42 | first, second | 2,541,260 | 11,476 | 79 |
| Later argument trace | 42 | first, argument, second | 2,856,758 | 13,108 | 100 |
| Failure in first application | explicit error | first | 572,554 | 142 | 85 |

The failing case does not execute the later argument's trace. Additional named
UPLC snapshots cover packing four values, an eligible three-value prefix before a
traced/failing fourth argument, strict unused computations, cold values, shadowing,
forced references and deliberate recursive divergence without evaluating it.

### Integrated validation

All 199 performance fixtures produce identical complete O1 Flat bytes after one,
two and three optimizer invocations. Their late named output is also pointer-stable
under repeated cleanup/packing. Normal fixture checks separately cover Core and
late-pass fixed points. Existing O0 snapshot content is preserved; snapshot changes
show optimized output and isolated-pass results.

All 3,876 workspace nextest tests pass. The 199-case performance baseline check,
formatting, root strict Clippy and isolated performance-workspace strict Clippy
also pass. Performance fixtures and cost assertions
remain in the separate workspace outside normal test discovery.
