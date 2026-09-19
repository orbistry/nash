# Validators

A validator module is the unit of on-chain code. `nash build` compiles each
validator module in the project into one UPLC script.

## Purpose

Nash has no `validator` declaration inside a module and no blueprint. The
*module* is the validator: its header says so, its `main` value is the entry
point, and the whole dependency closure of `main` is inlined into one script.

## Surface syntax

```elm
validator module Vesting exposing (main)

import Cardano.Tx exposing (Tx, Output)

type Datum = Datum { owner : Bytes, deadline : Int }

main : Datum -> Redeemer -> Data -> unit
main datum redeemer ctx =
    case redeemer of
        Claim -> assert (lower datum.deadline < currentSlot ctx)
        Cancel -> assert (signedBy ctx datum.owner)
```

Grammar (extends `module_header` in [syntax.md](syntax.md)):

```ebnf
module_header  ::= [ "validator" ] "module" module_name "exposing" exposing
```

The `validator` keyword is only legal in the header. Everything else in the
module is ordinary Nash: types, traits, impls, values, a `tests` block.

## Detection

A module is a validator when its header starts with `validator`. Nothing else
marks it: not a file location, not a config entry, not the presence of `main`.
The parser records the kind on the source module and canonicalization carries
it into the canonical module (see [plans/09](../plans/09-validators-build.md)).

`nash build` collects every module whose kind is `Validator`, in every source
directory of every workspace member, and compiles each one. A package can
contain validator modules; they are built when the package is built directly,
not when it is used as a dependency.

## `main`

A validator module must define a top-level value named `main`. Canonicalization
reports an error when it is missing:

```
-- NO MAIN ---------------------------------------------------- src/Vesting.nash

This is a validator module, so I require that it has a `main` value. That way I
have something to compile into a script!

1| validator module Vesting exposing (main)
   ^^^^^^^^^^^^^^^^^^^^^^^^
Try adding a `main` value to this module? Or if you just want to verify that
this module compiles, drop the `validator` keyword from the header.

Note: Adding a `main` value can be as brief as:

    main : Data -> unit
    main ctx = ()
```

`main` must also be exposed. A validator module whose `exposing` list omits
`main` gets a second error, `MAIN NOT EXPOSED`, pointing at the exposing list
with the hint "add `main` to the exposing list". `exposing (..)` satisfies
it.

### Signature

The signature of `main` is not constrained by the compiler. Every parameter
becomes an outer lambda of the script, in order, and the body becomes the
script body:

```elm
main : Datum -> Redeemer -> Data -> unit
```

compiles to

```
(program 1.1.0 (lam datum (lam redeemer (lam ctx <body>))))
```

The ledger applies the script to `Data` arguments. An off-chain tool applies
any *parameters* first, then the ledger applies the script-context argument(s)
the ledger version defines. Which arguments the ledger passes is a convention
of the target chain, not of Nash:

- Cardano Plutus V3 passes exactly one argument, the script context. A
  parameter-free V3 validator is `main : ScriptContext -> unit` (or
  `Data -> unit` with manual decoding). Extra leading parameters are applied
  off-chain.
- Layer-2 systems and other ledgers are free to define their own arity and
  argument types. Nash compiles whatever `main` is.

One constraint follows from "arguments are applied from outside": every
parameter type of `main` must have representation `Big` or `Const`. A `Big` parameter
receives a `Data` constant, which is what a ledger passes. A `Const`
parameter receives any other UPLC constant (`int`, `bytes`, `list Int`, ...),
which only an off-chain tool or a test can apply; it exists for
parameterized scripts. No conversion is inserted at the boundary: a `Big`
value *is* its `Data`, a `Const` value is the constant itself. A Cardano
ledger only applies `Data`, so a `Const` parameter must be filled off-chain;
`nash build` does not warn about one in v1, because the compiler does not
know the target. A parameter
with representation `Term` (a function, a little ADT such as `option`, a tuple, a little
record) is an error on the parameter's type: "nothing outside the script can
supply this" (see [kinds.md](kinds.md), [codegen.md](codegen.md)). The
return type is free and is ignored.

### Success and failure

A script succeeds when evaluation completes without an error. The return
value is ignored. `fail`, `assert`, a failed pattern match, a builtin error or
budget exhaustion all fail the script. Cardano developers return `unit` by
convention; nothing checks it.

There is no boolean protocol: `main` returning `False` is a *successful*
script. Use `assert` to fail.

## Build outputs

`nash build` writes, for every validator module `A.B.C`, into `build/`:

| File | Content |
|---|---|
| `build/A.B.C.uplc` | UPLC text syntax, `(program 1.1.0 ...)`, with named binders |
| `build/A.B.C.flat` | The flat-encoded program, raw bytes |
| `build/A.B.C.cbor` | Hex text of the CBOR byte string wrapping the flat bytes |

The `.cbor` file is the single-wrapped form (`CBOR(bytes(flat))`). Transaction
builders that need the double-wrapped `PlutusV3Script` envelope wrap it once
more. `nash build` also prints the script hash (blake2b-224 of the language tag
byte followed by the single-wrapped bytes) for each validator.

## Target compatibility

Each validator uses its owning application's or package's `plutusVersion`
(default `"v3"`), overridden by `--plutus-version`. Workspace members retain
their own settings; dependency settings do not change the caller's script.

Compatibility targets **protocol version 11**. Ledger language versions and
UPLC versions are distinct. V1, V2, and V3 all emit UPLC 1.1.0 and permit the
builtins supported by this compiler at this protocol baseline; the selected
ledger language still determines the script hash tag and evaluator semantics.

The entire generated program is validated, including nested terms and empty
typed containers. Native `constr`/`case` terms are available for all three ledger
languages. Protocol 11 also permits `case` on boolean, integer, list, pair,
and unit constants. `Data` is not directly supported by native `case`: Nash
uses `chooseData` directly with delayed branches, forcing only the selected one.
BLS runtime constants cannot be serialized as script literals; BLS builtins can
construct values at runtime.

The built-in evaluator uses bundled cost models. These are not live protocol
parameters queried from mainnet, so reported budgets do not establish current
mainnet execution costs.

Generated files are tracked in `.nash-artifacts` in the output directory.
Successful builds remove only stale tracked files, including when no validators
remain. Unowned files and previous outputs on compilation failure are preserved;
unowned output collisions and artifact/manifest symlinks are errors. Old outputs
without a manifest must be relocated or removed explicitly before reuse.

## `nash build` flags

```
nash build [PATH] [--plutus-version v1|v2|v3] [--trace-level LEVEL]
           [--compiler-traces[=true|false]] [--out DIR]
```

| Flag | Values | Default | Effect |
|---|---|---|---|
| `--trace-level` | `silent`, `compact`, `verbose` | `traceLevel` in `nash.jsonc`, else `silent` | How user `trace` calls compile. `silent` removes them, `compact` keeps a short code per site, `verbose` keeps the full message. |
| `--compiler-traces` | optional boolean | `compilerTraces` in config, else false | Keep compiler traces independently of user traces. The bare flag enables them; `--compiler-traces=false` disables them. |
| `--plutus-version` | `v1`, `v2`, `v3` | `plutusVersion` in config, else `v3` | Select the target compatibility rules. |
| `--out` | path | `build` | Output directory. |

Flags override config values for one run. Builds remain unoptimized; optimizer
settings are rejected until Plan 08. See [cli.md](cli.md).

## Tests inside validator modules

A validator module may end with a `tests` block. `nash build` strips it
before canonicalization: the block's own imports are not resolved, test-only
dependencies are not needed, and nothing from the block reaches the script.
`nash check` retains and currently diagnoses unsupported test blocks; Plan 10
will add test checking and execution. See [testing.md](testing.md).

Stripping happens on the source AST in the driver, before name resolution and type checking. Syntactically valid test-only names
and imports therefore cost nothing under `nash build`. Syntax errors still fail
parsing; this does not implement a test runner.

## Interactions

- **Representations.** Parameter types of `main` must satisfy `Storable`
  (`Big` or `Const`). The host can supply only UPLC constants at this boundary.
- **Codegen.** `main` is the root of dead-code elimination. Only values in the
  transitive closure of `main` are lowered to Core and monomorphized; traits
  and impls that are never reached are not emitted.
- **Traces.** The trace level and the compiler-trace switch are codegen
  options, applied per build. A validator built with `verbose` is a different
  script (and hash) from the same module built with `silent`.
- **Comptime and macros.** They run during compilation of the module as usual;
  a validator module gets no special treatment.

## Non-goals (v1)

- **No blueprint.** Nash does not write a CIP-57 `plutus.json`. Type
  information for off-chain code is a later feature that generates TypeScript
  from the solved types of `main`.
- **No parameter application.** `nash build --apply` is not part of v1. The
  off-chain tool applies parameters as `Data` to the `.cbor` script.
- **No multi-validator modules.** One module, one `main`, one script. Related
  validators (spend + mint of the same contract) are separate modules sharing
  library modules.

## Open questions

- **Overview example arity.** The example in overview.md has
  `main : Datum -> Redeemer -> Data -> unit`. Under Plutus V3 the ledger passes
  one argument, so `datum` and `redeemer` there are off-chain parameters, not
  ledger arguments. The docs above keep the compiler indifferent; the stdlib
  `Cardano` module should ship a V3 convention with a single `ScriptContext`
  argument and helpers that pull the datum and redeemer out of it.
- **`--apply`.** A `nash build --apply Module.Name arg.json ...` command that
  applies `Data` parameters and writes a new `.cbor` is cheap to add once the
  `PlutusData` JSON codec exists. Deferred until the TypeScript codegen
  decides how off-chain code represents parameters.
