# Claude Code Guidelines for Nash Compiler

## Project Overview

Nash is a programming language compiler being ported from the Elm compiler. We are translating Haskell to Rust while adapting to Rust idioms where appropriate.

## Reference Material

The Elm compiler source is cloned locally at `elm/` (gitignored). Use this as the primary reference for:
- Parser structure: `elm/compiler/src/Parse/`
- Error types: `elm/compiler/src/Reporting/Error/Syntax.hs`
- AST definitions: `elm/compiler/src/AST/`

## Memory Management

We use **bumpalo** for arena allocation:
- One `Bump` arena per module
- All AST nodes are allocated in the arena
- Source text is loaded into the arena first, so `&'a str` slices point into arena memory
- Unified `'a` lifetime throughout - drop the arena, everything is freed
- bumpalo does NOT call `Drop` on items - this is fine since we only store `Copy` types or references

```rust
let bump = Bump::new();
let src: &str = bump.alloc_str(&file_contents);
let mut parser = Parser::new(&bump, src);
```

### AST Type Guidelines

**Inline small `Copy` types** - don't put them behind `&'a`:
- Small enums (e.g., `VarType`, `Associativity`) - just store the value
- Newtypes around primitives (e.g., `Precedence(u16)`) - just store the value
- `Region` uses native-size source coordinates (32 bytes on 64-bit hosts); keep it inline in AST nodes.

**Use `&'a T` for**:
- Large types
- Recursive types (must break infinite size)
- Slices (`&'a [T]`, `&'a str`)

**Use `Option<&'a T>` not `&'a Option<T>`**:
- Null pointer optimization makes `Option<&'a T>` the same size as `&'a T`
- `None` is free (null pointer, no arena allocation)
- Only allocates when `Some`

## Testing Strategy

We use **insta** for snapshot testing with extremely granular tests:
- Separate macros for success vs error cases
- `assert_X_snapshot!` - expects Ok, panics on Err, snapshots the parsed value
- `assert_X_error_snapshot!` - expects Err, panics on Ok, snapshots the error

Macros are defined in each module's test submodule for proper namespacing.

Snapshot expectations have one source of truth: the `.snap` file. Do not also
assert exact results, logs, diagnostic variants, counts, or text fragments
already present in that snapshot. Render actual compiler/evaluator output,
never a string selected from the expected outcome.

Keep parse/compile and success/error guards. Checks of independent properties
(type preservation, scope validity, pointer identity, roundtrips, or semantic
equivalence) are still required. When a fixture also has a snapshot, run these
property checks after it so output changes first produce an insta diff. Reuse
the fixture instead of compiling it again for a duplicate test.

Keep test helpers local to their test module and snapshots in its `snapshots/`
directory. Optimizer snapshots show both before and after Core or UPLC at the
pass's layer. All codegen snapshots that generate executable Core group unoptimized Core then
unoptimized UPLC, followed by optimized Core then optimized UPLC. The unoptimized
side uses original compiled or hand-built Core and ordinary lowering, with only required
recursion encoding: no optimization passes or builtin/constant sharing.
Use the shared production O1 pipeline in `crates/nash-codegen/src/optimizer.rs`,
rendered by `crates/nash-codegen/tests/support/optimizer.rs`, so each accepted
optimization updates the whole fixture corpus. Pass tests retain
isolated-pass evidence after that comparison. Diagnostic/metadata snapshots stay
focused; invalid Core records lowering errors, open Core is closed explicitly for
pipeline rendering, and deliberate divergence is rendered without evaluation.
Optimizer performance experiments and budget regression tests
belong in the separate `tools/optimizer-perf` workspace, outside normal test runs. Use
`cargo nextest run --workspace` for the repository test suite.

Internal dev-dependencies are path-only (`nash-report = { path = "../nash-report" }`, no
`version`). Cargo strips them at publish, so test-only edges never constrain the publish
order or require an unpublished crate on the registry.

Example snapshot output:
```
---
source: crates/nash-parse/src/lib.rs
description: "42"
---
42
```

Run tests with:
```bash
cargo insta test
cargo insta accept  # to accept new snapshots
cargo insta test --unreferenced delete  # delete stale snapshots
```

### Test String Formatting

The test macros use `indoc!` which strips leading indentation, so:
- **Simple one-liners** stay simple: `assert_expression_snapshot!("if x then y else z");`
- **Multiline tests** use the `assert_indented_*_snapshot!` macro variants,
  which lay the fragment out as it would appear indented inside a
  definition (a token at column 1 always starts a new top-level
  declaration, so bare multiline fragments are not valid input):
  ```rust
  assert_indented_expression_snapshot!(r#"
      if condition then
          trueBranch
      else
          falseBranch
  "#);
  ```
- No need to slam strings to the left - `indoc` handles it
- Only use multiline raw strings when testing actual multiline syntax

## Design Docs, Plans, Progress

- `docs/overview.md` — authoritative design decisions; `docs/<component>.md` — component specs
- `docs/syntax.md` — the grammar (EBNF)
- `plans/NN-<component>.md` — chunked implementation plans, in execution order
- `SPEC.md` — progress checklist only

Update the relevant doc when a decision changes; tick SPEC.md as plan chunks land.

## Development Workflow

1. Pick the next chunk in the current `plans/NN-*.md`
2. Write snapshot test(s) first
3. Implement
4. Run `cargo insta test`, review snapshots
5. Mark the chunk done in the plan; tick SPEC.md when the plan completes

## Validation

After making changes, run these checks before committing:

```bash
cargo fmt --all          # format code
cargo clippy --all-targets --all-features -- -D warnings  # lint (warnings = errors)
cargo test               # run tests
```

These same checks run in CI on every push/PR.

## Error Messages

We are porting Elm's full error hierarchy from `Reporting/Error/Syntax.hs`. Error types are nested (Module contains Decl, Decl contains Expr, etc.) to enable Elm-quality error messages.

## Versioning & Changesets

We use [**sampo**](https://github.com/bruits/sampo/blob/main/crates/sampo/README.md) for changelog and version management:
- Each crate has its own independent version (no shared `version.workspace = true`)
- When making notable changes, create a changeset with `sampo add`
- Sampo handles granular per-crate version bumps based on what actually changed
- CI runs the Sampo GitHub Action to automate release PRs and publishing

```bash
# single crate
sampo add -p nash-parse -m "Add support for let expressions"

# multiple crates in one changeset
sampo add -p nash-parse -p nash-region -m "Add position tracking to let expressions"
```

Since `sampo add` requires interactive bump level selection, Claude should write changeset files directly to `.sampo/changesets/` instead:

```markdown
---
cargo/nash-parse: minor
cargo/nash-region: patch
---

Add position tracking to let expressions.
```

Use a short descriptive filename like `.sampo/changesets/let-expr-positions.md`.

## Code Style

- Recursive descent parser (not parser combinators)
- Single `Parser<'a>` struct combining arena + state
- Incremental implementation - test each feature before moving on
- No premature abstraction
