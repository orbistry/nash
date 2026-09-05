# Nash Compiler — Status and Roadmap

Nash compiles an Elm/Haskell-style functional language to Untyped Plutus
Core. Design decisions live in [`docs/overview.md`](docs/overview.md);
component specs in [`docs/`](docs/); chunked implementation plans in
[`plans/`](plans/). This file tracks progress only.

## Pipeline

```
parse -> canonicalize -> kinds -> constrain/solve (+ traits) -> nitpick
      -> macro expansion (loop) -> Core IR -> optimize -> UPLC
```

Modules do not compile individually. Only `validator module`s and tests
produce UPLC programs; all dependencies inline into each program.

## Crates

| Crate | Purpose | Status |
|---|---|---|
| `nash-region` | spans | done |
| `nash-source` | surface AST | extend ([plans/01](plans/01-syntax.md)) |
| `nash-parse` | parser + Elm error hierarchy | extend ([plans/01](plans/01-syntax.md)) |
| `nash-ast` | canonical AST | extend |
| `nash-can` | canonicalization, interfaces | extend |
| `nash-constrain` | constraint generation, kinds | extend |
| `nash-solve` | solver, traits, defaulting | extend |
| `nash-nitpick` | exhaustiveness | new ([plans/05](plans/05-nitpick.md)) |
| `nash-report` | diagnostics (Elm prose -> miette) | new ([plans/06](plans/06-diagnostics.md)) |
| `nash-ir` | Core IR + passes | new ([plans/07](plans/07-codegen.md), [08](plans/08-optimizer.md)) |
| `nash-codegen` | Can -> Core -> UPLC | new ([plans/07](plans/07-codegen.md)) |
| `nash-test` | test runner, fuzzing, shrinking | new ([plans/10](plans/10-testing.md)) |
| `nash-macro` | macro expansion, comptime | new ([plans/11](plans/11-macros-comptime.md)) |
| `nash-fmt` / `nash-docs` | formatter, docs | new ([plans/13](plans/13-fmt-docs.md)) |
| `nash-plutus` | UPLC terms, flat, CEK, cost models | done |
| `nash-config` | `nash.jsonc` | done, extend |
| `nash-driver` | build graph, caching | done, extend |
| `nash-cli` | `nash` binary | `check`, `lsp`; add `build test fmt docs` |
| `nash-language-server` | LSP | stub |
| `core/` | `nash/core` stdlib package (Nash source) | new ([plans/12](plans/12-stdlib.md)) |

## Progress

Done:

- [x] Parser (Elm `Parse/*` port, full syntax error hierarchy)
- [x] Canonicalization (Elm `Canonicalize/*` port, SCC, interfaces)
- [x] Type inference (Elm `Type/*` port: constraints, rank-based solver, records, aliases)
- [x] Project config, driver, dependency-ordered builds, interface cache
- [x] `nash check`
- [x] UPLC runtime (`nash-plutus`): conformance suite passes

Planned, in execution order (each links to its plan):

- [x] 01 Syntax: `'a` type vars, little/Big names, `trait`/`impl`, `=>` contexts, kind annotations, `validator module`, `tests` block, `do`, attributes, `name!()`, `comptime`, `assert`/`fail`/`todo`/`trace`; drop `Float`/`Char`/record extension types — [plans/01-syntax.md](plans/01-syntax.md)
- [x] 02 Kinds: Big / Const / Term, kind inference, kind variables — [plans/02-kinds.md](plans/02-kinds.md)
- [ ] 03 Traits: qualified types, resolution, superclasses, defaults, multi-param, orphan rules, literal traits + defaulting, evidence — [plans/03-traits.md](plans/03-traits.md)
- [ ] 04 Representation: remove row polymorphism and Elm supertypes, builtin type inventory, record encoding — [plans/04-representation.md](plans/04-representation.md)
- [ ] 05 Exhaustiveness (`Nitpick/PatternMatches` port) — [plans/05-nitpick.md](plans/05-nitpick.md)
- [ ] 06 Diagnostics (`nash-report`, Elm `Reporting/*` port onto miette) — [plans/06-diagnostics.md](plans/06-diagnostics.md)
- [ ] 07 Codegen: Core IR, monomorphization, decision trees, recursion, Data casts, UPLC lowering — [plans/07-codegen.md](plans/07-codegen.md)
- [ ] 08 Optimizer: inlining, builtin force caching, DCE, case-of-known-ctor/constant folding — [plans/08-optimizer.md](plans/08-optimizer.md)
- [ ] 09 Validators + `nash build` — [plans/09-validators-build.md](plans/09-validators-build.md)
- [ ] 10 Testing: `tests` block, props, fuzzers, shrinking, power-assert, `nash test` — [plans/10-testing.md](plans/10-testing.md)
- [ ] 11 Macros + comptime — [plans/11-macros-comptime.md](plans/11-macros-comptime.md)
- [ ] 12 Stdlib `nash/core` — [plans/12-stdlib.md](plans/12-stdlib.md)
- [ ] 13 `nash fmt`, `nash docs` — [plans/13-fmt-docs.md](plans/13-fmt-docs.md)

Later: LSP features, web playground, package registry (pubgrub), TypeScript codegen.

## Elm file mappings

| Elm (`elm/compiler/src/`) | Nash |
|---|---|
| `Parse/*` | `crates/nash-parse/src/*` |
| `Reporting/Error/Syntax.hs` | `crates/nash-parse/src/error.rs` |
| `AST/Source.hs` | `crates/nash-source/src/lib.rs` |
| `AST/Canonical.hs` | `crates/nash-ast/src/lib.rs` |
| `Canonicalize/*` | `crates/nash-can/src/*` |
| `Type/Type.hs`, `Type/Constrain/*` | `crates/nash-constrain/src/*` |
| `Type/{Solve,Unify,Occurs}.hs` | `crates/nash-solve/src/*` |
| `Nitpick/PatternMatches.hs` | `crates/nash-nitpick` (planned) |
| `Reporting/{Doc,Report,Render,Suggest}.hs`, `Reporting/Error/*` | `crates/nash-report` (planned) |
| `builder/src/Elm/Outline.hs` | `crates/nash-config` |
| `builder/src/Build.hs` | `crates/nash-driver` |

## Validation

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo insta test
```
