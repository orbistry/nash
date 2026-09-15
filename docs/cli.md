# CLI

The `nash` binary is `crates/nash-cli`. `check` and `build` load `PATH`
(default `.`) through the nearest owning `nash.jsonc` or `aiken.toml`.
Pass a manifest path explicitly to select a format when both files exist in one
directory; an unqualified directory selection reports that ambiguity.

Native `.nash` and Aiken `.ak` sources share import inspection and the Nash
compiler pipeline. The [Aiken source/project frontend](aiken-frontend.md) targets
exact Aiken 1.1.23 and Plutus V3, including locked packages, environments,
configuration, workspaces and multiple named validators. Tests and benchmarks
are checked, not executed or emitted as production entry points.

## Commands

| Command | Status | Purpose |
|---|---|---|
| `nash check [PATH]` | exists | Parse, canonicalize and type check every module, including `tests` blocks. No codegen. |
| `nash build [PATH]` | exists | Check all sources, then emit UPLC, Flat and CBOR for each native validator module and each named Aiken validator. |
| `nash test [PATH]` | planned (plans/10) | `check`, then compile and run every `test` and `prop`. |
| `nash fmt [PATH...]` | planned | Format files in place, or `--check` to report unformatted files. |
| `nash docs [PATH]` | planned | Generate HTML documentation for exposed modules into `docs/`. |
| `nash lsp` | exists | Language server over stdio. |
| `nash init NAME` | planned | Create a project skeleton: `nash.jsonc`, `src/`, one validator module with a `tests` block. |

Aliases: `nash c` for `check`, `nash b` for `build`; `nash t` is planned with `test`.

Version proxying stays as it is: `nash` reads the `compiler` field of
`nash.jsonc` and re-executes the matching downloaded compiler
(`crates/nash-cli/src/proxy.rs`).

## Flags

Global flags, accepted before the subcommand:

| Flag | Effect |
|---|---|
| `--color auto\|always\|never` | ANSI colors on stderr. Default `auto`. |
| `--json` (planned) | Machine-readable output on stdout, human output suppressed. Diagnostics are emitted as miette JSON. |
| `-q`, `--quiet` (planned) | Only errors and the final summary. |

`nash build`:

| Flag | Default | Effect |
|---|---|---|
| `--trace-level silent\|compact\|verbose` | `silent` | User `trace` compilation mode. |
| `--compiler-traces` | off | Keep compiler-generated traces. |
| `--out DIR` | `build` | Output directory. |
| `--env NAME` | `default` for Aiken | Select the environment module and configuration section. |

The current build targets Plutus V3 without optimization. Config defaults,
`--optimize`, and other target versions remain Plans 08/09 work.

`nash test` (planned):

| Flag | Default | Effect |
|---|---|---|
| `--seed N` | random `u32` | Seed for property tests. Printed in the summary so a run can be replayed. |
| `--max-success N` | `100` | Iterations per property. |
| `--match PATTERN` | all | Run only tests whose `Module.Name` or name contains `PATTERN`. Repeatable. `--match "Vesting.{claim}"` selects a test by name inside a module. |
| `--exact` | off | `--match` compares whole strings. |
| `--trace-level` | config `traceLevel`, else `verbose` | As for `build`, but tests default to `verbose`. |
| `--jobs N` | number of cores | Worker threads. |
| `--coverage labels\|tests` | `labels` | Denominator of the label table: total labels, or total iterations. |

`nash fmt`:

| Flag | Effect |
|---|---|
| `--check` | Do not write. Exit `1` if any file would change. |
| `--stdin` | Format stdin to stdout. |

`nash check`:

| Flag | Effect |
|---|---|
| `--no-warnings` | Suppress warnings; errors only. |
| `--report human\|json` | `json` prints nash-report's Elm-shaped JSON document (`{"type":"compile-errors",...}`) instead of the terminal rendering. Default `human`. |
| `--env NAME` | Select the Aiken environment module and configuration section; defaults to `default`. |

Plans 09/10 add these two flags to `build` and `test`. The initial build
command uses human diagnostics and shows warnings.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. For `test`: all tests passed. For `fmt --check`: nothing to change. |
| `1` | The project has errors: parse, canonicalization, type, kind, or codegen errors; or a test failed; or `fmt --check` found a file to change. |
| `2` | The command could not run, such as an unknown flag. Project-loading and compiler errors are reported through the command's diagnostic path. |

Warnings never change the exit code.

## Output layout

```
<project>/
  nash.jsonc
  src/**/*.nash
  src/**/*.ak
  build/                      nash build
    Module.Name.uplc
    Module.Name.flat
    Module.Name.cbor
  docs/                       nash docs
  .nash/                      caches (interfaces, downloaded compilers)
```

Native output names remain `Module.Name`. Aiken output names include package,
version, module and validator; delimiter bytes inside components are
percent-encoded. Each name has `.uplc`, `.flat` and `.cbor` files.
`.cbor` is one CBOR byte string containing the `.flat` bytes.

Outputs are written only after successful checking and code generation.
Stale-output removal remains Plan 09 work; obsolete artifacts must currently
be removed explicitly. For Aiken projects, `build/packages/` also contains
materialized dependencies and their tracking file. Do not delete edited package
sources inadvertently.

Human-readable output goes to stderr. Machine-readable output (`--json`) goes
to stdout. Diagnostics use the miette fancy renderer with Elm's prose (see
[diagnostics.md](diagnostics.md)).

### First Aiken manual checks

```sh
cargo run -p nash-cli -- check crates/nash-driver/tests/fixtures/aiken/full-language
cargo run -p nash-cli -- check crates/nash-driver/tests/fixtures/aiken/env-config-project --env preview
cargo run -p nash-cli -- build crates/nash-driver/tests/fixtures/aiken/multi-validator-project --out build/manual
cargo test -p nash-driver --test aiken_projects
```

The first three projects need no downloads. The integration test prepares the
committed stdlib and direct/transitive package archives offline and checks them
through normal project loading. A direct CLI check of `stdlib-project` or
`dependency-project` needs those packages in the normal cache/build directory.
The multi-validator build emits two artifact sets and does not run its test or
benchmark. Blueprint serialization and parameter application are not provided.

### Planned `nash build` transcript

```
   Compiling 14 modules
    Building Vesting (build/Vesting.cbor, 2.1 KB)
             hash 3a9f…c41e
    Building Vesting.Mint (build/Vesting.Mint.cbor, 1.4 KB)
             hash 88b0…12ff
    Finished 2 validators in 0.42s
```

### `nash test` transcript

See [testing.md](testing.md#example-output).

## `nash.jsonc` additions

Plan 09 adds three optional fields on `application` and `package` configs.
The initial build command does not read them:

```jsonc
{
    "type": "application",
    "sourceDirectories": ["src"],
    "plutusVersion": "v3",       // "v1" | "v2" | "v3"; default "v3"
    "traceLevel": "compact",     // "silent" | "compact" | "verbose"; default "silent"
    "optimize": 2,               // 0 | 1 | 2; default 2
    "dependencies": { }
}
```

| Field | Used by | Meaning |
|---|---|---|
| `plutusVersion` | `build`, `test` | Builtin set available to codegen; cost model used by `nash test`. |
| `traceLevel` | `build`, `test` | Default for `--trace-level`. `nash test` overrides the default to `verbose` when the field is absent. |
| `optimize` | `build`, `test` | Default for `--optimize`. Tests always run the same passes as the build so budgets in `within` reflect what ships. |

Workspace configs do not carry these fields; each member sets its own. A
dependency's values are ignored: the building project's settings apply to the
whole script.

Rust side (`crates/nash-config/src/config.rs`): `PlutusVersion` and
`TraceLevel` enums, `optimize: u8` validated to `0..=2` at parse time, all
with serde defaults. Details in [plans/09](../plans/09-validators-build.md).

## Open questions

- **`nash init`** needs a template stdlib import list and a default
  `compiler` pin. Both depend on the first published `nash/core` version.
- **`--json` shape** for `nash test` is specified in testing.md; compile
  errors from `check`, `build` and `test` use nash-report's Elm-shaped JSON
  (`--report=json`, [diagnostics.md](diagnostics.md)). Whether a stable
  schema is promised before 1.0 is undecided.
- **Watch mode** (`--watch`) is not planned for v1; the LSP covers the
  interactive case.
