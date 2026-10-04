# Proofs with Lean-blaster

A module can end with a `proof` block alongside its `tests` block. Both have
private imports and access to the module's private functions. Proofs compile
through Nash's ordinary specialization, pattern compilation, recursion rewrite
and UPLC lowering. The generated UPLC is imported by PlutusCoreBlaster and
symbolically evaluated with its CEK step semantics. A local wrapper preserves
unfinished states at zero fuel, rather than using the upstream convention that
turns fuel exhaustion into an error. Lean-blaster checks the resulting
obligation. No independently translated Nash semantics are introduced.

The source and canonical ASTs keep proofs separate from tests: `ProofBinder`
names a symbolic `domain`, and `ProofObligation` distinguishes execution claims
from successful-return postconditions. Partial-correctness obligations cannot
carry failure modifiers. Shared parser and formatter helpers preserve the common
syntax; proof budgets are rejected during parsing and invalid domains or partial
failure modifiers during canonicalization.

```elm
module Identity exposing (identity)

identity : int -> int
identity x = x + 0

proof
    import Proof

    test "closed fact" = do
        assert (identity 42 == 42)

    prop "every integer" =
        let x via Proof.int in
        do
            assert (identity x == x)

    prop "division by zero" fail =
        let x via Proof.int in
        do
            assert (x / 0 == 0)

    prop "find a negative integer" fail once =
        let x via Proof.int in
        do
            assert (x >= 0)
```

## Input domains

`via` in a proof declares a symbolic domain; it does not draw from a generator.
Only references to the compiler-bundled `Proof` module are accepted (including
import aliases and unqualified exposed names). User functions, generator
expressions, `Prop.int`, and ordinary values are rejected before type checking.
The domain values in Base supply Nash type information and are never executed
by proof compilation.

| Domain | Nash type | Lean input and UPLC encoding |
|---|---|---|
| `Proof.int` | `int` | arbitrary mathematical integer; native integer constant |
| `Proof.integer` | `Int` | arbitrary mathematical integer; `Data.I` |
| `Proof.bool` | `bool` | arbitrary Boolean; native Boolean constant |
| `Proof.bytes` | `bytes` | PlutusCore `ByteString`; native bytes constant |
| `Proof.byteString` | `Bytes` | PlutusCore `ByteString`; `Data.B` |
| `Proof.string` | `string` | arbitrary string; native string constant |
| `Proof.data` | `Data` | arbitrary Plutus Data, including malformed application encodings |
| `Proof.spendingV1/V2/V3` | `Data` | script context subject to the matching ledger spending predicate |
| `Proof.mintingV1/V2/V3` | `Data` | script context subject to the matching ledger minting predicate |

Ledger domain versions must match the configured Plutus version. V3 domains
quantify over `ScriptContext`. V1/V2 quantify over the library's `SpendingInput`
or `MintingInput`, apply its validity predicate, and pass **only its encoded
context** to the Nash binder. They do not automatically supply datum/redeemer
arguments to a validator. For correlated datum/redeemer/context properties or
custom inductive domains, edit an exported Lean project using the library's
full input builders. Generic symbolic list/product/custom ADT domains are not
implemented in this first version.

The validity predicates are assumptions from CardanoLedgerApiBlaster, not a
claim that Nash enforces those rules. Primitive Data domains add no ledger
assumptions. Results inherit the pinned libraries' models and limitations,
including their ByteString representation and admitted supporting declarations.

Multiple `via` binders are quantified independently. Conditions can be written
as ordinary Nash control flow, for example:

```elm
prop "positive input" =
    let x via Proof.int in
    do
        if x > 0 then
            assert (x + 1 > x)
        else
            ()
```

There is no `assume` keyword or implicit precondition inference. Conditions
that return unit in the excluded branch mean implication for ordinary passing
properties. For `fail` properties that branch succeeds and therefore refutes
the universal failure claim. A successfully halting excluded branch cannot supply
a `fail once` rejection witness; an excluded branch that exhausts the configured
execution limit still rejects.

## Execution and expected outcomes

| Declaration | Obligation |
|---|---|
| `test` | closed body successfully halts |
| `test ... fail` | closed body rejects through a script error or fuel exhaustion |
| `prop` | every input in the domain successfully halts |
| `prop ... fail` | every input in the domain rejects through a script error or fuel exhaustion |
| `prop ... fail once` | find an input that rejects through a script error or fuel exhaustion |

As with Nash validators, successful execution means halting without an error.
A returned false Boolean is not rejection. Bodies are checked as `unit` and use
ordinary strict assertion/sequence semantics without power-assert test tracing.
`within (cpu ..., mem ...)` is rejected: proof fuel is an evaluation-step limit,
not the ledger's execution cost model. Continue using `tests` for measured
budgets, labels, randomized generators and shrinking.

Fuel is part of the property's execution limit. Exhaustion is rejection, so both
`fail` and `fail once` accept it. A passing property must halt successfully within
that limit. This is bounded execution semantics, not an unbounded termination
claim. Solver timeouts and unknown results are inconclusive and never count as
rejection witnesses. `no_witness` means every modeled input succeeds within the
configured limit.

The command uses Lean-blaster's structured result API and emits no admitted
Nash theorem. Results are explicitly **SMT verification under the pinned
models**, not independently kernel-certified proofs. Counterexamples are the
library's textual SMT models; automatic decoding, replay and shrinking are not
yet implemented.

## Partial correctness for recursive functions

Use `Proof.returns computation (\result -> condition)` as the sole expression
in a passing proof body. The compiler emits two separate UPLC programs: the
computation and its Boolean postcondition. For example:

```elm
proof
    import Proof

    prop "square root is correct when it returns" =
        let value via Proof.int in
        do
            Proof.returns (Int.isqrt value) (\root ->
                root >= 0 && root * root <= value && value < (root + 1) * (root + 1))
```

This checks the returned root against adjacent squares, without calling `isqrt`
again in the specification. The condition can refer to every original symbolic
input as well as the returned value. Multiple inputs use a single `let`:

```elm
let left via Proof.int
    right via Proof.int
in
```

A `verified_partial` result means every successful return **within `--fuel`**
satisfies the condition. An error or exhaustion supplies no successful return;
it remains rejection under Nash's execution-limit semantics. Partial correctness
does not establish termination, acceptance, or successful return for any input.
In particular, an insufficient execution limit can make the implication vacuous.
It also does not establish correctness of returns beyond that limit.

The postcondition has its own `--postcondition-fuel` limit. A false condition or
an evaluation error refutes the property. If the checker exhausts on a successful
computation, the result is `postcondition_exhausted` and the command fails; it
cannot verify the property. Solver unknowns and timeouts also remain inconclusive.
The computation, checker completion, and condition are checked independently so
an assertion failure cannot disappear behind a successful-return guard.

Returned values can be native integers, Booleans, bytes, strings, unit, or
Data-represented types. Native lists, tuples, custom ADTs and functions cannot
cross this boundary yet. `fail` and `fail once` modifiers are rejected for
`Proof.returns`. Wrapping it in another function or placing it inside other
statements does not select partial correctness: only the direct body call is
recognized. Outside that position it is an ordinary strict assertion helper.

## Command and generated artifacts

```sh
nash proof .
nash verify . --match "integer identity" --fuel 10000 --timeout 30
nash proof . --fuel 1000 --postcondition-fuel 2000
nash proof . --emit-only --output build/exported-proofs
nash proof . --lean-project /path/to/prebuilt/lean-project --json
```

`verify` is an alias for `proof`. Project/member Plutus settings are honored,
with an optional `--plutus-version v1|v2|v3` override. Match filters use the same
module/property matching rules as `nash test`.

The command exports a portable Lake project: `lakefile.lean`, `lean-toolchain`,
`proofs.json`, and one `ProofN.lean` per selected declaration. Script flat bytes
are embedded in the Lean sources. The source is preserved on solver failures,
with stdout/stderr beside it when executed. Dependency revisions are pinned:

- Lean 4.24.0;
- Lean-blaster `bafdd4f7976037cd7bd8e443df04af096dc5b96e`;
- PlutusCoreBlaster `41fe7eadf460dc66bef22b85656bc36408638bdc`;
- CardanoLedgerApiBlaster `3f9f7c6b6ecf0012e1397bab0a814daa36e70ba2`.

The default command initializes and builds that project's Lake dependencies on
first use; this needs network access and an installed Lean/Lake toolchain.
`--emit-only` needs neither Lean nor an SMT solver. `--lean-project` uses an
already built project without updating it; it must provide the pinned API.
An appropriate Z3 must be on `PATH` for obligations requiring SMT reasoning.

Defaults: 10,000 computation steps, 10,000 postcondition steps, 30 seconds per
SMT query, 120 seconds total per property (including preprocessing). Backend setup is outside that per-property
limit. On Unix, a timed-out run terminates the Lake/Lean/solver process group.
The command exits unsuccessfully on refutation, unknown, timeout,
backend failure, no witness, or an empty selection.

Exports refuse unrelated output directories. Repeated runs reuse the pinned Lake
project and its dependency cache, while preserving each run in its own
`run-...` directory with its Lean sources and `proofs.json`. Existing generated
sources remain available for review or direct `lake env lean run-.../ProofN.lean` use.

`examples/proofs` demonstrates the complete flow:

```sh
nash proof examples/proofs --fuel 80 --postcondition-fuel 150
```

Increasing fuel can substantially increase preprocessing time, even when a
smaller execution limit already sufficed. A timeout is an unverified result,
not a refutation.

An optional integration test runs the real backend and covers expected outcomes and execution exhaustion:

```sh
NASH_PROOF_LEAN_PROJECT=/path/to/prebuilt/lean-project \
    cargo test -p nash-driver --test proofs live_backend -- --ignored
```

Partial-correctness regressions exercise all six supported return representations,
incorrect results, computation errors/exhaustion, checker errors, and checker
exhaustion:

```sh
NASH_PROOF_LEAN_PROJECT=/path/to/prebuilt/lean-project \
    cargo test -p nash-driver --test proofs live_partial_correctness -- --ignored
```
