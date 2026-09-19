# CLI

The `nash` binary is `crates/nash-cli`. Every command loads the project from
`PATH` (default `.`) by walking up to the nearest `nash.jsonc`, exactly like
`Project::load` in `crates/nash-driver/src/project.rs` does today.

## Commands

| Command | Status | Purpose |
|---|---|---|
| `nash check [PATH]` | exists | Parse, canonicalize and type check every module. Includes test blocks but does not execute them. No codegen. |
| `nash build [PATH]` | exists | Exclude test blocks, check the frontend, then compile every validator module for its configured target. |
| `nash test [PATH]` | exists | Check, compile and run project `test` and `prop` declarations. |
| `nash fmt [PATH...]` | planned | Format files in place, or `--check` to report unformatted files. |
| `nash docs [PATH]` | planned | Generate HTML documentation for exposed modules into `docs/`. |
| `nash lsp` | exists | Language server over stdio. |
| `nash init NAME` | planned | Create a project skeleton: `nash.jsonc`, `src/`, one validator module with a `tests` block. |

Aliases: `nash c` for `check`, `nash b` for `build`; `nash t` for `test`.

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
| `--plutus-version v1\|v2\|v3` | config `plutusVersion`, else `v3` | Ledger language target at the protocol 11 baseline. |
| `--trace-level silent\|compact\|verbose` | config `traceLevel`, else `silent` | User `trace` compilation mode. |
| `--compiler-traces[=true\|false]` | config `compilerTraces`, else false | Independently control compiler traces; the bare flag enables them. |
| `--out DIR` | `build` | Output directory. |

CLI options override the owning project's configuration. Builds are unoptimized;
`--optimize` and the `optimize` config field are rejected while Plan 08 is deferred.
See [target compatibility](validators.md#target-compatibility).

`nash test`:

| Flag | Default | Effect |
|---|---|---|
| `--seed N` | random `u32` | Seed for property tests. Printed in the summary so a run can be replayed. |
| `--max-success N` | `100` | Iterations per property. |
| `--match PATTERN` | all | Run only tests whose `Module.Name` or name contains `PATTERN`. Repeatable. `--match "Vesting.{claim}"` selects a test by name inside a module. |
| `--exact` | off | `--match` compares whole strings. |
| `--trace-level` | config `traceLevel`, else `verbose` | As for `build`, but tests default to `verbose`. |
| `--jobs N` | number of cores | Positive worker count; fixed seeds give the same ordered results across worker counts. |
| `--plutus-version v1\|v2\|v3` | member config, else `v3` | Target validation and bundled execution cost model. |
| `--json` | off | Write one structured result document to stdout. |
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

`build` uses human diagnostics and shows warnings. Additional report controls
for `build` are not implemented. `test --json` emits structured test outcomes.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. For `test`: all tests passed. For `fmt --check`: nothing to change. |
| `1` | The project has errors: parse, canonicalization, type, kind, or codegen errors; or a test failed; or `fmt --check` found a file to change. |
| `2` | The command could not run: no `nash.jsonc`, invalid config, unknown flag, missing output directory permissions, import cycle. |

Warnings never change the exit code.

## Output layout

```
<project>/
  nash.jsonc
  src/**/*.nash
  build/                      nash build
    Module.Name.uplc
    Module.Name.flat
    Module.Name.cbor
    .nash-artifacts            generated-file ownership
  docs/                       nash docs
  .nash/                      caches (interfaces, downloaded compilers)
```

Outputs are written only after every module and validator compiles successfully.
The `.nash-artifacts` manifest records generated filenames. A successful build
removes stale owned outputs, including when there are no validators. Other
files are preserved; a new output colliding with an unowned file is rejected.
Pre-manifest outputs are not claimed automatically: remove or relocate them
explicitly, or choose a fresh `--out` directory. Artifact and manifest symlinks
are rejected before writing.

`.flat` is raw bytes. `.cbor` is hex text representing a single CBOR byte
string containing those Flat bytes. The CLI prints a script hash for each
validator. Compilation failure leaves the previous artifacts and manifest intact.

Human-readable output goes to stderr. Machine-readable output (`--json`) goes
to stdout. Diagnostics use the miette fancy renderer with Elm's prose (see
[diagnostics.md](diagnostics.md)).

### `nash build` output

After the existing frontend diagnostic summary, each validator prints its module
name, Flat size, and script hash. The final line reports the validator count and
output directory.

### `nash test` transcript

See [testing.md](testing.md#example-output).

## `nash.jsonc` additions

Three optional build settings are accepted on `application` and `package` configs:

```jsonc
{
    "type": "application",
    "sourceDirectories": ["src"],
    "plutusVersion": "v3",       // "v1" | "v2" | "v3"; default "v3"
    "traceLevel": "compact",     // "silent" | "compact" | "verbose"; default "silent"
    "compilerTraces": false,     // boolean; default false
    "dependencies": { }
}
```

| Field | Used by | Meaning |
|---|---|---|
| `plutusVersion` | `build` | Ledger language target and permitted generated features at the supported protocol baseline. |
| `traceLevel` | `build` | Default for `--trace-level`. |
| `compilerTraces` | `build` | Default for `--compiler-traces`; independent of user traces. |

Workspace configs reject these fields; each member sets its own. CLI overrides
apply to every member for that invocation. A
dependency's values are ignored: the building project's settings apply to the
whole script.

Rust side (`crates/nash-config/src/config.rs`): `PlutusVersion` and
`TraceLevel` enums and `Build` settings with validated JSONC defaults. Details in [plans/09](../plans/09-validators-build.md).

## Open questions

- **`nash init`** needs a template stdlib import list and a default
  `compiler` pin. Both depend on the first published `nash/core` version.
- **`--json` shape** for `nash test` is specified in testing.md; compile
  errors from `check`, `build` and `test` use nash-report's Elm-shaped JSON
  (`--report=json`, [diagnostics.md](diagnostics.md)). Whether a stable
  schema is promised before 1.0 is undecided.
- **Watch mode** (`--watch`) is not planned for v1; the LSP covers the
  interactive case.
