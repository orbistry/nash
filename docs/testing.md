# Testing

Tests live in a `tests` block at the end of a module. `nash test` compiles
every `test` and `prop` into a standalone UPLC program, runs it on the CEK
machine in `nash-plutus`, and reports the result with budgets, traces, labels
and, for properties, a shrunk counterexample.

The design follows Aiken: the PRNG is a Plutus value that the generator
threads through on-chain code, the runner only sees the sequence of random
choices, and shrinking is choice-sequence shrinking in Rust (MiniThesis). Nash
uses ordinary generation functions returning a value and next PRNG state.
They can sequence draws with `Option` `do` notation. A power-assert
`assert` that prints the value of every sub-expression on failure.

## Surface syntax

```elm
module Order exposing (compare, invert)

...

tests
    import Prop exposing (int, listOf)

    test "lt is strict" = do
        assert (not (lt 1 1))

    prop "compare is antisymmetric" =
        let
            a via int
            b via int
        in
        do
            label (if a < b then "lt" else "ge")
            assert (compare a b == invert (compare b a))

    prop "division by zero fails" fail =
        let x via int in
        do
            assert (x / 0 == 0)

    test "sort stays within budget" within (cpu 2000000, mem 10000) = do
        assert (sort [3, 1, 2] == [1, 2, 3])
```

Grammar (same notation as [syntax.md](syntax.md)):

```ebnf
module        ::= module_header { import } { declaration } [ tests_block ]

tests_block   ::= "tests" INDENT { import } { test_decl } DEDENT

test_decl     ::= "test" string [ "fail" ] [ within ] "=" do_block
                | "prop" string [ "fail" [ "once" ] ] [ within ] "=" prop_body
within        ::= "within" "(" budget [ "," budget ] ")"
budget        ::= "cpu" integer | "mem" integer

prop_body     ::= "let" INDENT via_binder { via_binder } DEDENT "in" do_block
via_binder    ::= pattern_term "via" expr

do_block      ::= "do" INDENT statement { statement } DEDENT
statement     ::= lower_name "<-" expr
                | "let" INDENT { def } DEDENT
                | expr
```

Rules the parser and canonicalizer enforce:

- The `tests` block is the last thing in the module. Its imports are visible
  only inside the block, in addition to the module's own imports. Everything
  in the enclosing module is in scope, including values it does not expose.
- `test` names and `prop` names are strings, unique within the module.
- `via` binders are allowed only at the top of a `prop` body, and a `prop`
  body must start with them (a `prop` that draws nothing is a parse error;
  use `test`). The left side is a pattern: `(a, b) via tuple2 int int` is
  fine. Irrefutable patterns only; a refutable pattern is the usual
  exhaustiveness error.
- Generators resolve in the module and test-import scope; they do not refer to
  other via-bound values. Use direct draws inside a generation function for dependent values.
- The `let ... in` that holds `via` binders holds nothing else. Ordinary
  `let` follows in the body.
- `within` takes one or two budgets in either order, at most one of each.
- `fail once` is a `prop` modifier; on a `test` it is a parse error.
- `do_block` is the general `do` from [syntax.md](syntax.md). What changes in
  a test body is its desugaring, described below.

## Semantics

### Test body

A test body has type `unit` and is always a `do` block: `= do` for a `test`,
`in do` after the `via` binders for a `prop`. A one-statement body is
`= do` followed by one `assert`. This `do` is a **sequencing block**, not a
monadic `do`: there is no monad, and `Monad.bind` is never involved. The
`do` keyword is reused because the layout is the same; the desugaring is:

```
do { e; rest }        ==>  let () = e in rest
do { x <- e; rest }   ==>  let x = e in rest
do { let d; rest }    ==>  let d in rest
do { e }              ==>  e
```

Every expression statement must have type `unit`. `x <- e` binds the value of
`e` (any type) to `x`. The last statement is the result and must be `unit`.
UPLC is strict, so `let () = e in rest` evaluates `e` before `rest`; the
statements run in order.

Only the `do` in test-body position (right after `=`, or after `in` for a
`prop`) is a sequencing block. A `do` anywhere else, including nested
inside a test body or inside a generator, is the monadic `do` of
[syntax.md](syntax.md). There is no `test` monad.

Statements that tests use:

| Statement | Type | Meaning |
|---|---|---|
| `assert e` | `e : bool`, statement `: unit` | Keyword. Fails the test when `e` is `False`. Power-assert, see below. |
| `label s` | `string -> unit` | Stdlib `Test.label`. Records a label for the coverage table. Compiles to a trace with a `\0label\0` prefix. |
| `trace s ()` | `trace s : 'a -> 'a` | Keyword. Emits a trace line, shown under `· with traces`. |
| `fail s` | `: 'a` | Keyword. Fails with a message. |

`assert`, `fail`, `todo` and `trace` are keywords with their own expression
nodes (`Expr::Assert` and friends, [syntax.md](syntax.md)), not functions.
`label` and `assertFailed` are ordinary functions in the stdlib `Test`
module ([stdlib.md](stdlib.md)). Only `assert` gets the compiler treatment
below, and only inside a `tests` block.

### Outcome of a unit test

A `test` runs once. Evaluation either completes (any value, but the body is
typed `unit` so it is `()`) or errors. With no modifier, completion passes.
With `fail`, an error passes and completion fails.

### Outcome of a property

A `prop` runs its body for up to `--max-success` (default 100) drawn inputs.
Each iteration completes or errors.

| Modifier | Iteration counts as counterexample when | Prop passes when |
|---|---|---|
| none | body errors | no counterexample in `n` iterations |
| `fail` | body completes | no counterexample in `n` iterations (every input fails) |
| `fail once` | body errors | a counterexample is found; it is reported as `★ counterexample` |

The first counterexample stops the loop and is shrunk. This is Aiken's
`OnTestFailure` table: `FailImmediately`, `SucceedEventually`,
`SucceedImmediately` (`test_framework.rs:421-446`, `1155-1171`).

### `within (cpu N, mem M)`

The test fails when the consumed budget exceeds a given limit; a missing
limit is unbounded. For a `prop` the
limit applies to every iteration, and the report shows the maximum. Budgets
are measured with the bundled cost model of the configured `plutusVersion`
after the same Core passes `nash build` runs. The evaluator does not query live
protocol parameters; these measurements do not establish current mainnet costs.
`within` and
`fail` compose: a `fail` test that errors *and* stays within budget passes.

The budget check is done by the runner from `EvalResult.info.consumed_budget`;
the program is evaluated with the machine maximum budget
(`ExBudget::max()`), never with `N`/`M` as the limit, so a failure is reported
with the real consumption.

### Traces

Every evaluation collects the trace log. The runner splits it:

- lines starting with `\0label\0` are labels;
- lines starting with `\0assert\0` are power-assert payloads;
- everything else is shown verbatim under `· with traces`.

Only failing tests show traces by default. `--trace-level` follows the same
values as `nash build` (default `verbose` for tests).

## Power-assert

Codegen captures operands and supplies static assertion IDs and capture prefixes.
Nash `Test.assertAt` and `Test.assertCapture` compose the trace messages and call
the next failure step. Rendered captures stay inside the failure branch, and
continuations preserve trace order without evaluating later captures early.
These helpers use `Builtin.trace` explicitly, so reserved test messages survive
silent user tracing without a module-name exception in codegen.

`assert e` with `e : bool`. The compiler rewrites the call so that, when `e`
is `False`, the value of every sub-expression of `e` whose type has a `Show`
impl is traced, then evaluation fails. The runner lays the values out under
the source text.

For

```elm
assert (compare a b == invert (compare b a))
```

with `a = 1`, `b = 0` and an `invert` that leaves `LT` unchanged, the report is

```
FAIL compare is antisymmetric [after 4 tests]
× counterexample
│ a = 1
│ b = 0
× assert (compare a b == invert (compare b a))
          │       │ │    │       │       │ │
          │       │ │    │       │       │ 1
          │       │ │    │       │       0
          │       │ │    │       LT
          │       │ │    LT
          │       │ 0
          │       1
          GT
  Order.nash:31:13
```

Rules:

- **Captured sub-expressions.** Variables, calls, operator applications,
  field accesses, negations, and `if`/`case`/`let` expressions as a whole.
  Not captured: literals, lambdas, nullary constructors, and the `assert`
  argument itself (its value is `False`).
- **Strict positions only.** Sub-expressions under a lazy position are not
  captured: the right operand of `&&` and `||`, the branches of `if` and
  `case`, and lambda bodies. Capturing them would change which expressions
  get evaluated.
- **Evaluation order is preserved.** Each captured sub-expression is bound
  once with `let`, in source order, and the original expression uses the
  binding. Nothing is evaluated twice.
- **`Show`.** A captured sub-expression whose type has no `Show` impl (a
  function, a type without `@derive(Show)`) prints `?` in its column. The
  `Show` evidence is resolved by the trait solver like any other constraint;
  the rewrite runs after solving with the sub-expression types known.
- **Cost.** A passing `assert` costs the `let` bindings only. The `show`
  calls sit in the failure branch. Test programs are not size sensitive.
- **Payload.** On failure the program emits `\0assert\0<assert-id>` even if
  there are no captured values, then traces one line per captured
  sub-expression with a `Show` impl, `\0assert\0<assert-id>\0<index>\0<shown>`,
  then errors. The compiler records, per test, the table
  `assert-id -> (source region, text, [(index, sub-expression region)])`.
  The runner joins the two. Sub-expressions without `Show` are in the table
  but never in the payload.

`assert e` outside a `tests` block (in a validator) is lowered by codegen as
`if e then () else fail` ([codegen.md](codegen.md)). The rewrite applies only
to test bodies.

## Generators

```elm
-- Prop.nash (stdlib)
type prng = Seeded bytes (list int) | Replayed (list int)
type alias generator 'a = prng -> option ('a, prng)

dependent : Prop.generator int
dependent state = do
    (bound, next) <- Prop.choice 10 state
    (value, final) <- Prop.choice bound next
    Some (value, final)

```

- `prng` is a **little** ADT: the runner builds a native constructor term and reads it
  back from the result. `Seeded seed choices` carries a 32-byte seed and the
  choices made so far, newest first. `Replayed choices` carries only the
  choices still to replay, next first. An empty list means replay is exhausted.
- `generator 'a` is a function alias. Each draw returns `(value, nextPrng)` in
  `Some`, or `None` for invalid replay. The value may itself contain functions.
  Call the function directly with a PRNG state. The alias has no trait instances
  or runtime constructor. `do` sequences the returned `option` values through
  `Monad option`; state threading remains explicit.
- Choices are integers in `0..18446744073709551615`, one per primitive draw, as in
  MiniThesis. Aiken uses bytes; Nash uses `int` so a primitive can draw a
  64-bit integer in one choice and shrink it with one binary search. Larger
  integers can be assembled from several choices. A primitive bound outside
  that range is a generator error; values are never truncated.

The single primitive:

```elm
-- The runner stores each primitive choice in a u64.
choice : Lift int 'n => 'n -> generator int
choice bound = choiceInt (lower bound)

choiceInt : int -> generator int
choiceInt bound prng =
    if bound < 0 || bound > 18446744073709551615 then
        (fail "Prop.choice bound must be in 0..18446744073709551615")
    else
        case prng of
            Seeded seed choices ->
                let
                    seed2 = Builtin.blake2b_256 seed
                    n = Builtin.byteStringToInteger True seed2 % (bound + 1)
                in
                Some (n, Seeded seed2 (Builtin.mkCons n choices))
            Replayed choices ->
                case choices of
                    [] -> None
                    n :: rest ->
                        if n < 0 || n > bound then None
                        else Some (n, Replayed rest)

```

A replayed sequence that runs out, or replays a value outside the requested
bounds, yields `None`. That is what makes choice-sequence shrinking sound: any
edit to the sequence either replays to a valid smaller input or is rejected
by the generator itself. The generation helpers (`int`, `listOf`, `oneOf`,
`bytes`) are built on `choice`, and generators must draw smaller values
from smaller choices for shrinking to produce smaller inputs.

Nullary generators like `int` are values, so `a via int` and
`xs via listOf int` read naturally.

## How the runner drives a property

For every `prop` the compiler emits one preparation program:

```nash
prepare : prng -> option (prng, unit -> unit, unit -> list string)
```

Codegen translates the source patterns, body, and selected Show calls into
ordinary callbacks. Nash `Test.both` composes generators in source order,
initializing each later generator only after the earlier draw succeeds.
Nash `Test.prepare` handles rejection and constructs the state/body/display tuple;
these operations are library code, not a second implementation in codegen.

It draws the `via` values once, then returns the next PRNG, a property-body
function, and a function that shows the values. Native tuples can hold these
functions, including their captured values. No generator rerun is needed to
recover state or display a counterexample.

The runner saves the state before calling the body with `()`. This order is
required by strict evaluation: a body failure must not discard the state.
Showing values is deferred until a counterexample is needed. Preparation and
body execution share one execution-budget limit; their consumed budgets are added.

Loop, seeded with `--seed`:

1. Evaluate `prepare prng`. An error or `None` is a generator failure.
2. Retain the returned state and functions. Call the body with `()`.
3. On success, continue from the returned PRNG with its choice history cleared.
4. On a counterexample, call the display function and shrink the recorded choices.
   `fail` and `fail once` retain their existing expected-outcome semantics.

### Shrinking

A port of Aiken's `Counterexample::simplify` (`test_framework.rs:848-1007`),
itself a port of MiniThesis, with `u64` choices instead of `u8`:

1. Delete chunks of 8, 4, 2, 1 choices from the end, with the extra step of
   decrementing the choice before a deleted chunk (list lengths).
2. Replace chunks of 8, 4, 2 with zeros.
3. Binary-search each choice down toward 0.
4. Sort chunks of 8, 4, 2 ascending.
5. Swap out-of-order neighbours at distance 2 and 1, and redistribute value
   between them with a binary search.
6. Repeat until a full pass makes no change.

A candidate sequence is evaluated with `Prng::from_choices(candidate)`:
preparation first (error or `None` is `Invalid`), then the retained body
(body error is `Keep`, completion is `Ignore`; swapped for `fail`). A candidate is accepted when it
is `Keep` and shorter, or equal length and lexicographically smaller
(`consider`, `test_framework.rs:807-826`). Results are memoised by the exact choice sequence. A custom generator can
inspect the remaining replay choices, so a successful
prefix does not prove that every extension has the same result. Exact caching
preserves those generators' semantics.

Shrinking reports `Simplifying counterexample from N choices` and
`Simplified counterexample in Tms after S steps` on stderr while it works.

### Determinism and replay

The seed is a `u32`. The initial PRNG is `Seeded (blake2b256 seed_be_bytes)
[]`, as in Aiken (`Prng::from_seed`, `test_framework.rs:676-692`). Given the
seed, `--max-success`, and the same compiled programs, a run is fully
deterministic, including the shrink. The summary prints the seed; pass it
back with `--seed` to replay. Tests run in parallel but each prop owns its
PRNG chain, so parallelism does not change results.

## Reporting

Per test, one line:

```
PASS lt is strict                    [mem:    1.2K, cpu:   345.1K]
PASS compare is antisymmetric        [after 100 tests]
FAIL division by zero fails          [after   1 test]
```

Budgets on unit tests show the consumed budget. Props show iterations; with
`within` they also show the maximum budget. Failures add, in this order:

1. `× counterexample` (or `★ counterexample` for `fail once`), one
   `pattern = shown` line per `via` binder, the pattern printed as written.
2. Power-assert output for the failing `assert`, when the failure came from
   one.
3. `× budget exceeded` with consumed vs limit, when `within` failed.
4. `· with coverage`: the label table, only for passing props with labels.
5. `· with traces`: remaining trace lines.

The label table shows each label with its share. `--coverage labels`
(default) divides by the total number of labels recorded; `--coverage tests`
divides by the number of iterations. Labels are sorted by count, descending.

### Example output

Run `cargo run -p nash-cli -- test examples/order --seed 1` for the executable
example. Its deliberately incorrect `invert` produces a shrunk counterexample
and exit status 1. The following layout illustrates the report; budgets depend
on the compiled program.

```
  Testing Order (src/Order.nash)

  PASS lt is strict                  [mem:    1.2K, cpu:   345.1K]
  FAIL compare is antisymmetric      [after 4 tests]
  × counterexample
  │ a = 1
  │ b = 0
  × assert (compare a b == invert (compare b a))
            │       │ │    │       │       │ │
            │       │ │    │       │       │ 1
            │       │ │    │       │       0
            │       │ │    │       LT
            │       │ │    LT
            │       │ 0
            │       1
            GT
    src/Order.nash:31:13
  PASS division by zero fails        [after 100 tests]
  · with coverage
  | neg  51.0%
  | pos  49.0%
  PASS sort stays within budget      [mem:    9.8K, cpu: 1,912.4K]

  Summary 3 passed, 1 failed, 0 skipped   seed 2894013177   0.81s
```

`Simplifying`/`Simplified` progress lines go to stderr as they happen and are
not part of this block.

### JSON

`nash test --json` writes one document to stdout:

```jsonc
{
  "seed": 2894013177,
  "maxSuccess": 100,
  "tests": [
    {
      "module": "Order",
      "name": "compare is antisymmetric",
      "kind": "prop",
      "status": "fail",
      "iterations": 4,
      "budget": { "cpu": 512000, "mem": 2400 },
      "counterexample": [ { "name": "a", "value": "1" }, { "name": "b", "value": "0" } ],
      "assert": { "file": "src/Order.nash", "line": 31, "column": 13,
                  "source": "compare a b == invert (compare b a)",
                  "values": [ { "row": 0, "column": 0, "value": "GT" }, ... ] },
      "labels": {},
      "traces": []
    }
  ]
}
```

Capture `row` is zero-based relative to the assertion's first source line.
`column` is a terminal display column, relative to the assertion start on its
first line and to the source line start on later lines. Terminal reports place
values below their own source line and indent multiline shown values.

## Parallel execution

Tests are independent. The runner evaluates them on a rayon pool
(`--jobs`). Each worker decodes the flat-encoded program into its own
`nash_plutus::arena::Arena`, so no arena crosses a thread. A prop's
iterations and its shrink run sequentially on one worker; parallelism is
across tests, as in Aiken (`aiken-project/src/lib.rs:1173-1176`).

## Runtime consequences

- `prng` uses native constructors with `bytes`, `int`, and `list int` fields.
  The runner uses `Prng::to_term` and `Prng::from_term`; no Data encoding is needed.
- `prepare` returns a `constr` term: `Some` is `constr 0 [constr 0 [prng, body, show]]`,
  `None` is `constr 1 []`. The stdlib declares `type option 'a = Some 'a |
  None` in that order; the runner depends on the tags.
- Test programs use the same unoptimized Core passes as validator builds.
  Compiler traces are enabled; user traces default to verbose unless the owning
  project explicitly configures a level or the CLI overrides it. Plan 08 remains
  deferred. Property result tuples/options use native constructors and cases,
  supported for V1, V2, and V3 at the protocol 11 baseline.

## Interactions

- **Checking.** `nash check` resolves and type-checks project test blocks without
  executing them. Check/test include local path test dependencies; production
  builds exclude them. Dependency packages contribute ordinary code, not their
  tests or test dependencies. Registry/git fetching is not implemented; use
  local path dependencies or workspace members.

- **Validators.** `nash build` strips the `tests` block before
  canonicalization ([validators.md](validators.md)).
- **Traits.** `Show` for power-assert and counterexamples; `Monad option` for
  optional `do` sequencing of direct draws; `@derive(Show)` from
  [macros.md](macros.md).
- **Representations.** `('a, prng)` is a tuple (`Term`) because `pair` requires
  `Storable` components, while `'a` may be `Term`; `list string` is a `Const` list of `Const` strings.
- **Codegen.** Each test program is a standalone UPLC program that inlines
  the module's dependency closure ([codegen.md](codegen.md)).
- **Diagnostics.** Test-shape errors (`via` on a `test`, `once` on a `test`,
  duplicate names) are parser/canonicalization errors rendered by `nash-report`.

## Open questions

- **Choices as `int`.** This diverges from Aiken's byte choices. It makes
  `choice bound` one draw instead of a byte loop and keeps the shrinker
  identical up to the element type. If interop with Aiken generators matters,
  a byte-based `choice8` can be added without changing the runner.
- **Benchmarks.** Aiken has `bench`; Nash v1 does not. `within` covers the
  regression case.
- **Precondition / discard.** MiniThesis `assume` is not supported (same as
  Aiken). A generator can return `None` to reject, which counts as invalid,
  not as a discarded iteration.

## Nested choice trace slice

`tests/fixtures/NestedTrace.nash` and `tests/nested_trace.rs` in `nash-driver`
exercise a proposed nested replay representation through the compiler and CEK.
The shipped `Prop` API and runner still use the flat choice sequence above.

The fixture defines a little `choiceTree` ADT with `Choice int` and
`Group (cons choiceTree)`. A term-level `cons` holds the little constructor
values; a builtin `list` cannot hold these values. `trace` is a reserved Nash
keyword, so the type is named `choiceTree`.

Generation, bounds checks, group entry/exit, and replay normalization are Nash
functions. Each replayed group supplies only its own children: missing choices
or a mismatched node reject generation with `None`; unused children are dropped
from the returned consumed trace. The parent resumes at the next sibling.
List iterations group their continuation choice together with the element draw.
The seed source reuses `Prop.choice`, discarding its flat history after each draw.

Rust compiles ordinary function roots, evaluates them, decodes the native
constructors, edits a group, reconstructs an argument, and invokes replay. The
snapshot covers seeded replay, whole-element deletion, dependent bounds,
missing choices, node-kind mismatches, and branch shortening that preserves a
sibling. No compiler intrinsic or special codegen rule is introduced.

This slice does not implement automatic reduction, generator trait composition,
or integration with the production property runner. The reduction baseline is
MacIver and Donaldson, *Test-Case Reduction via Test-Case Generation: Insights
from the Hypothesis Reducer* (ECOOP 2020), sections 2 and 3,
<https://doi.org/10.4230/LIPIcs.ECOOP.2020.13>. Its shortlex ordering applies to
consumed primitive choices; groups guide edits. Strict group-local replay is a
Nash adaptation, not a property established by the paper's flat replay model.
