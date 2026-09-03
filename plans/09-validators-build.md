# Plan 09 — Validator modules and `nash build`

## Goal

`validator module Foo exposing (main)` is recognised end to end: the parser
records the kind, canonicalization requires an exposed `main`, the kind
checker rejects `Term` parameters, the driver strips `tests` blocks and compiles every
validator module through `nash-codegen`, and `nash build` writes
`build/<Module.Name>.{uplc,flat,cbor}`.

Spec: [docs/validators.md](../docs/validators.md), [docs/cli.md](../docs/cli.md).

## Prerequisites

- plans/01 (syntax): the parser accepts the `validator` keyword and the
  `tests` block and stores them on `nash_source::Module`. This plan consumes:

  ```rust
  // nash-source, added by plans/01
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum ModuleKind {
      Normal,
      /// Region of the `validator` keyword, for diagnostics.
      Validator(Region),
  }

  pub struct Module<'a> {
      pub kind: ModuleKind,                  // new
      pub name: Option<&'a Located<&'a str>>,
      pub exports: &'a Located<Exposing<'a>>,
      pub docs: &'a Docs<'a>,
      pub imports: &'a [&'a Import<'a>],
      pub values: &'a [&'a Located<Value<'a>>],
      pub unions: &'a [&'a Located<Union<'a>>],
      pub aliases: &'a [&'a Located<Alias<'a>>],
      pub binops: &'a [&'a Located<Infix<'a>>],
      pub tests: Option<&'a Tests<'a>>,       // new, see plans/10
  }
  ```

- plans/02 (kinds) and plans/03 Chunk 9: `nash_ast::{BaseKind, Kind,
  KindScheme, KindSet}`, and `nash_ast::Annotation.kinds` (one `KindScheme`
  per free variable, name-sorted like `free_vars`). This plan consumes:

  ```rust
  // nash-ast, from plans/02 and plans/03 Chunk 9
  pub enum BaseKind { Big, Const, Term }
  pub enum Kind<'a> { Base(BaseKind), Var(u16), Arrow(&'a Kind<'a>, &'a Kind<'a>) }
  pub struct Annotation<'a> {
      pub free_vars: FreeVars<'a>,
      pub context: &'a [Pred<'a>],
      pub kinds: &'a [KindScheme<'a>],
      pub typ: &'a Located<Type<'a>>,
  }
  ```

  A named type's base kind follows from the casing of its head
  (docs/kinds.md); `->`, tuples and little records are `Term`; a type
  variable's kind bound comes from `Annotation.kinds`.

- plans/07 (codegen): `nash-codegen` compiles a solved module, with its
  dependency closure, into a UPLC program rooted at `main`. This plan
  consumes plans/07 Chunk 11 as written there:

  ```rust
  // nash-codegen, from plans/07 Chunk 11
  pub struct Compiled<'a> {
      pub program: &'a Program<'a, DeBruijn>,
      pub named: &'a Term<'a, Name<'a>>,       // for the `.uplc` text
  }

  /// Codegen context over every solved module of the build (ADTs, impls,
  /// `SolvedTypes`, options). Constructed by the driver, plans/07 Chunk 9.
  pub struct Build<'a> { /* ... */ }

  pub fn validator<'a>(arena: &'a Arena, build: &Build<'a>, module: &'a nash_ast::Module<'a>) -> Result<Compiled<'a>, Error>;
  ```

  and the final solver entry point (plans/03 Chunk 5 "Solver API"):

  ```rust
  pub fn run<'a>(
      bump: &'a Bump,
      uf: &mut UnionFind<'a>,
      constraint: &Constraint<'a>,
      tables: &'a nash_can::Tables<'a>,
      fields: &'a nash_can::FieldTable<'a>,
      mode: nash_can::Mode,
  ) -> Result<(Annotations<'a>, SolvedTypes<'a>), Vec<Error<'a>>>
  ```
 `Build<'a>` takes the trace level, the
  compiler-trace switch and the optimization level; plans/07 names the
  field type `nash_ir::optimize::Options` or similar; this plan passes the
  three values from `nash_config::Build` (Chunk 4).

  The UPLC text printer is plans/07 Chunk 2 (`nash_plutus::pretty::program`).

- plans/06 (diagnostics): `nash-report` renders `nash_can::Error` variants.
  This plan adds two variants and gives the prose.

## Crates touched

`nash-source` (consume), `nash-ast`, `nash-can`, `nash-constrain`,
`nash-config`, `nash-plutus`, `nash-driver`, `nash-cli`.

## Elm / Aiken references

- `elm/builder/src/Reporting/Exit.hs` `MakeNoMain` (lines 1671-1693): prose
  for the missing-`main` error.
- `elm/compiler/src/Canonicalize/Module.hs` `canonicalize`: where the check
  sits in the phase order.
- `aiken/crates/aiken-project/src/lib.rs` `build` / `blueprint` (validator
  collection, output writing) and `crates/aiken-project/src/config.rs`
  (`plutus` version field).
- `aiken/crates/uplc/src/ast.rs` `Program::to_hex`, `to_cbor`,
  `Display for Term` (the UPLC text printer).

---

## Chunk 1 — `ModuleKind` on the canonical module

**Files**

- `crates/nash-ast/src/lib.rs`
- `crates/nash-can/src/module.rs`
- `crates/nash-can/src/interface.rs` (no change to `Interface`; kinds are not
  part of the interface)

**Change**

`nash_ast::Module` (`crates/nash-ast/src/lib.rs:33`) gets a `kind` field.
`nash_can::canonicalize` (`crates/nash-can/src/module.rs:48`) copies it from
the source module.

**Code**

```rust
// crates/nash-ast/src/lib.rs
pub use nash_source::ModuleKind;

#[derive(Debug)]
pub struct Module<'a> {
    pub kind: ModuleKind,
    pub name: ModuleName<'a>,
    pub exports: Exports<'a>,
    pub docs: &'a Docs<'a>,
    pub decls: &'a Decls<'a>,
    pub unions: &'a [&'a Located<Union<'a>>],
    pub aliases: &'a [&'a Located<Alias<'a>>],
    pub binops: &'a [&'a Located<Binop<'a>>],
}

impl Module<'_> {
    /// Whether this module compiles to a script.
    pub fn is_validator(&self) -> bool {
        matches!(self.kind, ModuleKind::Validator(_))
    }
}
```

```rust
// crates/nash-can/src/module.rs, in `canonicalize`
let can_module = CanModule {
    kind: module.kind,
    name: env.home,
    exports,
    docs: module.docs,
    decls,
    unions,
    aliases,
    binops,
};
```

`nash-ast` already depends on `nash-source`? It does not: check
`crates/nash-ast/Cargo.toml`. If adding the dependency is unwelcome, define
`ModuleKind` in `nash-region` next to `Region` and re-export from both.

**Elm reference**: none; Elm has no module kinds.

**Tests**

- `crates/nash-can/src/module.rs` tests: `assert_module_snapshot!` on
  `"validator module Foo exposing (main)\n\nmain ctx = ()"` shows
  `kind: Validator(...)`.

**Done when** the workspace compiles and existing snapshots are unchanged
except for the new `kind` field.

---

## Chunk 2 — `main` presence check

**Files**

- `crates/nash-can/src/error.rs`
- `crates/nash-can/src/module.rs`

**Change**

After `environment::local::add_vars` (`crates/nash-can/src/module.rs:66`)
the set of top-level value names is known. A validator module without `main`
is an error. The check runs before `canonicalize_decls` so the error is
reported alone, not buried under unrelated errors. A second check, after
`canonicalize_exports`, requires `main` to be exposed (syntax.md: "`nash
build` requires it to expose `main`").

**Code**

```rust
// crates/nash-can/src/error.rs
pub enum Error<'a> {
    // ...
    /// A `validator module` without a top-level `main`.
    ValidatorMissingMain {
        /// Region of the `validator` keyword.
        region: Region,
        module: &'a str,
    },
    /// A `validator module` whose exposing list omits `main`.
    ValidatorMainNotExposed {
        /// Region of the exposing list.
        region: Region,
        module: &'a str,
    },
}
```

```rust
// crates/nash-can/src/module.rs
fn check_validator_main<'a>(module: &SourceModule<'a>, home: ModuleName<'a>) -> Result<(), Error<'a>> {
    let ModuleKind::Validator(region) = module.kind else {
        return Ok(());
    };
    if module.values.iter().any(|v| v.value.name.value == "main") {
        return Ok(());
    }
    Err(Error::ValidatorMissingMain {
        region,
        module: home.name,
    })
}

fn check_validator_main_exposed<'a>(module: &SourceModule<'a>, exports: &Exports<'a>, home: ModuleName<'a>) -> Result<(), Error<'a>> {
    if !matches!(module.kind, ModuleKind::Validator(_)) {
        return Ok(());
    }
    let exposed = match exports {
        Exports::Everything(_) => true,
        Exports::Explicit(items) => items.iter().any(|e| matches!(e.value, Export::Value("main"))),
    };
    if exposed {
        Ok(())
    } else {
        Err(Error::ValidatorMainNotExposed { region: module.exports.region, module: home.name })
    }
}

// in `canonicalize`, after `add_vars` and after `canonicalize_exports`:
environment::local::add_vars(&mut env, module.values)?;
check_validator_main(module, env.home).map_err(|e| vec![e])?;
// ...
let exports = canonicalize_exports(bump, module)?;
check_validator_main_exposed(module, &exports, env.home).map_err(|e| vec![e])?;
```

Prose for `nash-report` (plans/06), ported from `MakeNoMain`:

```
title:   NO MAIN
message: This is a validator module, so I require that it has a `main` value.
         That way I have something to compile into a script!
region:  the `validator` keyword
hint:    Try adding a `main` value to this module? Or if you just want to
         verify that this module compiles, drop the `validator` keyword from
         the header.
note:    Adding a `main` value can be as brief as:

             main : Data -> unit
             main ctx = ()
```

**Elm reference**: `Reporting/Exit.hs` `MakeNoMain` for prose;
`Canonicalize/Module.hs` `canonicalize` for the phase.

Prose for `ValidatorMainNotExposed`: title `MAIN NOT EXPOSED`, "This is a
validator module, but its exposing list does not include `main`, so I
cannot tell whether `main` is meant to be the script.", hint "Add `main`
to the exposing list."

**Tests**

- `assert_module_error_snapshot!("validator module Foo exposing (main)\n\nx = 1")`
  → `ValidatorMissingMain`.
- `assert_module_error_snapshot!("validator module Foo exposing (x)\n\nx = 1\n\nmain c = ()")`
  → `ValidatorMainNotExposed`.
- `assert_module_snapshot!("validator module Foo exposing (main)\n\nmain ctx = ()")`
  → Ok.
- `assert_module_snapshot!("module Foo exposing (..)\n\nx = 1")` → Ok, no
  `main` needed for normal modules.

**Done when** the three snapshots are accepted.

---

## Chunk 3 — `Big` or `Const` parameters of `main`

**Files**

- `crates/nash-constrain/src/error.rs`
- `crates/nash-constrain/src/module.rs`

**Change**

After solving, the annotation of `main` is known
(`Annotations<'a>`, `crates/nash-can/src/interface.rs:71`). Walk its
`Type::Lambda` spine; every `from` type must have kind `Big` or `Const`
(overview.md, codegen.md "Validators"). `Term` parameters are the error:
nothing outside the script can supply a function, a little ADT or a tuple.
This is a post-solve check because `main` may be unannotated.

**Code**

```rust
// crates/nash-constrain/src/error.rs
pub enum Error<'a> {
    // ...
    /// A parameter of a validator's `main` whose kind is `Term`.
    MainParameterIsTerm {
        region: Region,
        index: usize,
        typ: &'a Located<nash_ast::Type<'a>>,
    },
}
```

```rust
// crates/nash-constrain/src/module.rs
/// Every parameter of a validator's `main` must be `Big` or `Const`: only constants can be applied from outside.
pub fn check_main_parameters<'a>(
    module: &nash_ast::Module<'a>,
    annotations: &Annotations<'a>,
) -> Result<(), Error<'a>> {
    if !module.is_validator() {
        return Ok(());
    }
    let main = annotations["main"];
    let mut typ = main.typ;
    let mut index = 0;
    while let nash_ast::Type::Lambda { from, to } = &typ.value {
        if param_is_term(main, from) {
            return Err(Error::MainParameterIsTerm { region: from.region, index, typ: from });
        }
        typ = to;
        index += 1;
    }
    Ok(())
}

/// `true` when the parameter type is definitely `Term`: an arrow, a tuple, a
/// little record, a lowercase-headed ADT, or a variable whose kind bound
/// (`Annotation.kinds`, plans/03 Chunk 9) excludes `Big` and `Const`.
fn param_is_term<'a>(main: &Annotation<'a>, typ: &Located<nash_ast::Type<'a>>) -> bool {
    use nash_ast::{BaseKind, Kind, KindSet, Type};
    match &typ.value {
        Type::Lambda { .. } | Type::Tuple { .. } | Type::Record { .. } | Type::Unit => true,
        Type::Named { reference, .. } => nash_ast::base_kind_of_head(reference.name) == BaseKind::Term,
        Type::Var(name) => {
            let i = main.free_vars.index_of(name).expect("annotation var is free");
            let scheme = &main.kinds[i];
            matches!(scheme.kind, Kind::Base(BaseKind::Term))
                || scheme.bounds.first().is_some_and(|b| *b == KindSet::TERM)
        }
        Type::Alias { real, .. } => param_is_term(main, real),
    }
}
```

Prose:

```
title:   BAD MAIN PARAMETER
message: The 2nd parameter of `main` has type `option int`, which is a Term
         type. Nothing outside the script can supply this: a script is only
         ever applied to constants, so every parameter of `main` must be a
         Big type (`Data`) or a Const type (`int`, `bytes`, ...).
hint:    Take `Data` (or a Big type like `Int` or a `type` with Big fields)
         and convert it inside `main`.
```

**Elm reference**: `Type/Constrain/Module.hs` `constrainEffects` /
`checkMain` is the closest analogue (Elm validates `main`'s type against
`Program`).

**Tests** (`crates/nash-constrain` snapshot tests):

- `validator module V exposing (main)\n\nmain : Int -> unit\nmain _ = ()` → Ok.
- `validator module V exposing (main)\n\nmain : int -> unit\nmain _ = ()` → Ok
  (`Const` parameter, applied by a tool or a test).
- `validator module V exposing (main)\n\nmain : option int -> unit\nmain _ = ()` →
  `MainParameterIsTerm { index: 0 }`.
- unannotated `main x = ()` where `x`'s type is a variable → Ok (a type
  variable's kind is unconstrained; codegen instantiates it at `Data`).

**Done when** the checks pass and `nash-driver` calls
`check_main_parameters` after `nash_solve::run` in `compile_module`.

---

## Chunk 4 — Config fields

**Files**

- `crates/nash-config/src/config.rs`
- `crates/nash-config/src/parse.rs`
- `crates/nash-config/src/error.rs`
- `crates/nash-config/src/lib.rs`

**Change**

Add `PlutusVersion`, `TraceLevel` and an `optimize` level to `Application`
(`config.rs:26`) and `Package` (`config.rs:53`), with a shared `Build`
struct so the driver reads one thing.

**Code**

```rust
// crates/nash-config/src/config.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlutusVersion {
    V1,
    V2,
    #[default]
    V3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TraceLevel {
    #[default]
    Silent,
    Compact,
    Verbose,
}

/// Codegen settings shared by `nash build` and `nash test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    #[serde(default)]
    pub plutus_version: PlutusVersion,
    #[serde(default)]
    pub trace_level: TraceLevel,
    /// `0..=2`, validated by the parser.
    #[serde(default = "default_optimize")]
    pub optimize: u8,
}

fn default_optimize() -> u8 {
    2
}

impl Default for Build {
    fn default() -> Self {
        Build { plutus_version: PlutusVersion::V3, trace_level: TraceLevel::Silent, optimize: 2 }
    }
}

pub struct Application {
    // existing fields...
    #[serde(flatten)]
    pub build: Build,
}

pub struct Package {
    // existing fields...
    #[serde(flatten)]
    pub build: Build,
}

impl Config {
    /// Build settings; a workspace has none and uses the defaults.
    pub fn build(&self) -> Build {
        match self {
            Config::Application(app) => app.build,
            Config::Package(pkg) => pkg.build,
            Config::Workspace(_) => Build::default(),
        }
    }
}
```

```rust
// crates/nash-config/src/parse.rs
fn parse_build(contents: &str, path: &Path, obj: &Object) -> Result<Build, ConfigError> {
    let plutus_version = match parse_optional_string(contents, path, obj, "plutusVersion")? {
        None => PlutusVersion::V3,
        Some(s) => match s.as_str() {
            "v1" => PlutusVersion::V1,
            "v2" => PlutusVersion::V2,
            "v3" => PlutusVersion::V3,
            other => {
                return Err(ConfigError::invalid_enum(
                    path,
                    "plutusVersion",
                    other,
                    &["v1", "v2", "v3"],
                    property_position(contents, obj, "plutusVersion"),
                ));
            }
        },
    };
    let trace_level = match parse_optional_string(contents, path, obj, "traceLevel")? {
        None => TraceLevel::Silent,
        Some(s) => match s.as_str() {
            "silent" => TraceLevel::Silent,
            "compact" => TraceLevel::Compact,
            "verbose" => TraceLevel::Verbose,
            other => {
                return Err(ConfigError::invalid_enum(
                    path,
                    "traceLevel",
                    other,
                    &["silent", "compact", "verbose"],
                    property_position(contents, obj, "traceLevel"),
                ));
            }
        },
    };
    let optimize = match find_property(obj, "optimize") {
        None => 2,
        Some(prop) => {
            let n = prop.value.as_number_lit().ok_or_else(|| {
                ConfigError::expected_number(path, position_of(contents, prop.value.range()))
            })?;
            match n.value.as_ref() {
                "0" => 0,
                "1" => 1,
                "2" => 2,
                other => {
                    return Err(ConfigError::invalid_enum(
                        path,
                        "optimize",
                        other,
                        &["0", "1", "2"],
                        position_of(contents, prop.value.range()),
                    ));
                }
            }
        }
    };
    Ok(Build { plutus_version, trace_level, optimize })
}
```

`parse_application` and `parse_package` call `parse_build` and store the
result. `ConfigError::invalid_enum` and `expected_number` are new
constructors in `error.rs` in the style of `expected_string`.

**Tests** (`crates/nash-config/src/parse.rs` tests):

- `{"type":"application"}` → `build == Build::default()`.
- `{"type":"application","plutusVersion":"v2","traceLevel":"verbose","optimize":1}`.
- `{"type":"application","optimize":3}` → `invalid_enum`.
- `{"type":"package", ... ,"traceLevel":"loud"}` → `invalid_enum`.

**Done when** `cargo test -p nash-config` passes and serde round-trips
(`serde_json` in dev-deps) agree with the AST parser.

---

## Chunk 5 — Script envelopes in `nash-plutus`

**Files**

- `crates/nash-plutus/src/script.rs` (new)
- `crates/nash-plutus/src/lib.rs`

**Change**

The UPLC text printer is plans/07 Chunk 2 (`nash_plutus::pretty::program`,
round-tripping through `syn::parse_program`); this chunk adds only the
CBOR wrapping and the script hash.

**Code**

```rust
// crates/nash-plutus/src/script.rs
use cryptoxide::{blake2b::Blake2b, digest::Digest};

/// `CBOR(bytes(flat))`, the single-wrapped script.
pub fn cbor_wrap(flat: &[u8]) -> Vec<u8> {
    minicbor::to_vec(minicbor::bytes::ByteSlice::from(flat)).expect("byte string encoding cannot fail")
}

/// Blake2b-224 of the language tag byte followed by the single-wrapped script.
pub fn script_hash(plutus_version_tag: u8, single_wrapped: &[u8]) -> [u8; 28] {
    let mut hasher = Blake2b::new(28);
    hasher.input(&[plutus_version_tag]);
    hasher.input(single_wrapped);
    let mut out = [0u8; 28];
    hasher.result(&mut out);
    out
}
```

The tag is `1` for V1, `2` for V2, `3` for V3.

**Aiken reference**: `uplc/src/ast.rs` `Program::to_cbor`,
`aiken-project/src/blueprint/validator.rs` `Validator::hash`.

**Tests**

- `cbor_wrap(&[0x01, 0x02])` → `[0x42, 0x01, 0x02]`.
- `script_hash` against one known Cardano script hash (take any published
  always-succeeds V3 script and its hash).

**Done when** both tests pass.

---

## Chunk 6 — Driver: compile modes, keep solved modules, build validators

**Files**

- `crates/nash-driver/src/compile.rs`
- `crates/nash-driver/src/build.rs` (new)
- `crates/nash-driver/src/error.rs`
- `crates/nash-driver/src/lib.rs`
- `crates/nash-driver/Cargo.toml` (add `nash-codegen`, `nash-plutus`, `hex`)

**Change**

Today `compile_module` (`compile.rs:155`) parses, canonicalizes and solves
in a per-module arena, deep-copies the interface into `store`, and drops
the module. Codegen needs every solved module alive. Introduce a
`CompileMode`; in `Build` and `Test` modes the module is allocated directly
in `store` and retained as a `Solved` record. `Check` keeps today's
behaviour. `Build` mode also strips `tests` from the source module.

**Code**

```rust
// crates/nash-driver/src/compile.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompileMode {
    /// Type check everything, including `tests` blocks. Modules are dropped.
    Check,
    /// Strip `tests` blocks, retain solved modules for codegen.
    Build,
    /// Keep `tests` blocks, retain solved modules for codegen.
    Test,
}

/// A solved module retained for codegen; lives in the build-wide arena.
pub struct SolvedModule<'s> {
    pub uri: Url,
    pub module: &'s nash_ast::Module<'s>,
    pub annotations: &'s nash_can::Annotations<'s>,
    pub types: &'s nash_solve::SolvedTypes<'s>,   // plans/07 Chunk 3
    pub source: &'s str,
}

pub struct BuildResult {
    pub modules: HashMap<Url, ModuleResult>,
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub warnings: Vec<String>,
}

pub async fn build(db: Arc<Mutex<Database>>, graph: &DepGraph) -> BuildResult {
    build_with(db, graph, CompileMode::Check, |_| ()).await
}

/// Compile in `mode`; `finish` sees the solved modules before the arena is dropped.
pub async fn build_with<R: Send + 'static>(
    db: Arc<Mutex<Database>>,
    graph: &DepGraph,
    mode: CompileMode,
    finish: impl FnOnce(&Solved<'_>) -> R + Send + 'static,
) -> (BuildResult, R) {
    let modules: Vec<&Url> = graph.levels().into_iter().flatten().collect();
    let sources = fetch_sources(&db, &modules).await;
    tokio::task::spawn_blocking(move || {
        let store = Bump::new();
        let (result, solved) = build_sync(&store, sources, mode);
        let r = finish(&Solved { store: &store, modules: solved });
        (result, r)
    })
    .await
    .expect("compile task panicked")
}

/// All solved modules of one build, in dependency order.
pub struct Solved<'s> {
    pub store: &'s Bump,
    pub modules: Vec<SolvedModule<'s>>,
}
```

`build_sync` becomes:

```rust
fn build_sync<'s>(
    store: &'s Bump,
    sources: Vec<(Url, Result<String, String>)>,
    mode: CompileMode,
) -> (BuildResult, Vec<SolvedModule<'s>>) {
    let mut interfaces: BTreeMap<&str, Interface<'_>> = BTreeMap::new();
    let mut results = HashMap::new();
    let mut all_warnings = Vec::new();
    let mut solved = Vec::new();

    for (uri, source) in &sources {
        let (output, module) = compile_module(uri, source, store, &interfaces, mode);
        if let Some(module) = module {
            interfaces.insert(module.module.name.name, nash_can::from_module(store, module.module, module.annotations));
            if mode != CompileMode::Check {
                solved.push(module);
            }
        }
        all_warnings.extend(output.warnings);
        results.insert(output.uri, output.result);
    }
    // totals as today
    (BuildResult { .. }, solved)
}
```

and `compile_module` allocates in `store` for `Build`/`Test` (the
per-module `Bump` stays for `Check`):

```rust
fn compile_module<'s>(
    uri: &Url,
    source: &Result<String, String>,
    store: &'s Bump,
    interfaces: &BTreeMap<&'s str, Interface<'s>>,
    mode: CompileMode,
) -> (CompileOutput, Option<SolvedModule<'s>>) {
    // ... as today, but `bump` is `store` when mode != Check ...
    let mut module = match parser.module() { .. };
    if mode == CompileMode::Build {
        module.tests = None;
    }
    // canonicalize, constrain, solve as today
    // Final signature: plans/03 Chunk 5 "Solver API".
    let (annotations, types) = nash_solve::run(
        store, &mut uf, &constraint, &can_result.tables, &can_result.fields, nash_can::Mode::Strict,
    )?;
    nash_constrain::check_main_parameters(&can_result.module, &annotations)
        .map_err(|e| ..)?;
    let solved = SolvedModule {
        uri: uri.clone(),
        module: store.alloc(can_result.module),
        annotations: store.alloc(annotations),
        types: store.alloc(types),
        source: src,
    };
    (CompileOutput { .. }, Some(solved))
}
```

`Check` mode keeps a per-module arena and only the deep-copied interface, as
now. The interface no longer needs `deep_copy_interface` in `Build`/`Test`
mode because the module itself is in `store`; `from_module(store, ..)`
allocates there directly.

Validator compilation:

```rust
// crates/nash-driver/src/build.rs
use nash_config::Build as BuildConfig;
use nash_plutus::{arena::Arena, flat, script};

pub struct ValidatorOutput {
    pub module: String,
    pub uplc: String,
    pub flat: Vec<u8>,
    pub cbor: Vec<u8>,
    pub hash: [u8; 28],
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum BuildError {
    #[error("codegen failed for {module}: {source}")]
    Codegen { module: String, #[source] source: nash_codegen::Error },
    #[error("flat encoding failed for {module}: {source}")]
    Flat { module: String, #[source] source: flat::FlatEncodeError },
}

/// Compile every validator module in `solved`.
pub fn build_validators(
    solved: &Solved<'_>,
    config: &BuildConfig,
    compiler_traces: bool,
) -> Result<Vec<ValidatorOutput>, BuildError> {
    let arena = Arena::new();
    // plans/07 Chunk 9/11: one codegen context over every solved module.
    let build = nash_codegen::Build::new(
        &arena,
        solved.modules.iter().map(|m| (m.module, m.annotations, m.types)),
        nash_codegen::Options {
            plutus_version: config.plutus_version,
            trace_level: config.trace_level,
            compiler_traces,
            optimize: config.optimize,
        },
    );

    let mut outputs = Vec::new();
    for m in solved.modules.iter().filter(|m| m.module.is_validator()) {
        let name = m.module.name.name.to_string();
        let compiled = nash_codegen::validator(&arena, &build, m.module)
            .map_err(|source| BuildError::Codegen { module: name.clone(), source })?;
        let flat_bytes = flat::encode(compiled.program).map_err(|source| BuildError::Flat { module: name.clone(), source })?;
        let cbor = script::cbor_wrap(&flat_bytes);
        let hash = script::script_hash(plutus_tag(config.plutus_version), &cbor);
        let named = Program::new(&arena, Version::plutus_v3(&arena), compiled.named);
        outputs.push(ValidatorOutput { module: name, uplc: nash_plutus::pretty::program(named), flat: flat_bytes, cbor, hash });
    }
    Ok(outputs)
}

fn plutus_tag(v: nash_config::PlutusVersion) -> u8 {
    match v { nash_config::PlutusVersion::V1 => 1, nash_config::PlutusVersion::V2 => 2, nash_config::PlutusVersion::V3 => 3 }
}

/// Write the three files for each validator into `out_dir`, removing stale outputs.
pub async fn write_outputs(db: &Database, out_dir: &Url, outputs: &[ValidatorOutput]) -> Result<(), DriverError> {
    for out in outputs {
        let base = out_dir.join(&out.module).expect("module name is a valid path segment");
        db.write(&with_ext(&base, "uplc"), &out.uplc).await?;
        db.write_bytes(&with_ext(&base, "flat"), &out.flat).await?;
        db.write(&with_ext(&base, "cbor"), &hex::encode(&out.cbor)).await?;
    }
    Ok(())
}
```

`FileSource` (`crates/nash-driver/src/source.rs:22`) gains
`write_bytes(&self, uri, &[u8])`; `write` stays for text. Stale-output
removal globs `out_dir/*.{uplc,flat,cbor}` and deletes files whose stem is
not in `outputs` (add `remove` to `FileSource`).

Codegen errors carry a `Region` and module; convert them to `ModuleResult::Failed`
per module so `nash build` reports them with the same shape as type errors.

plans/07 Chunk 11 sketches calling `nash_codegen::validator` from inside
`compile_module` and storing flat bytes on `ModuleResult::Success`. This
plan owns the driver: codegen runs after *all* modules are solved (the
validator inlines its dependency closure, so the `Build` context needs every
module), through `build_with`'s `finish` closure. plans/07's
`validator(...)` signature is unchanged; only the call site moves.

**Elm reference**: `builder/src/Build.hs` `fromPaths` → `Generate.prod` for
the "solve all, then generate from roots" shape.

**Tests**

- `compile.rs`: `test_build_mode_strips_tests` — a module whose `tests`
  block imports a module that does not exist compiles in `Build` mode and
  fails in `Check` mode.
- `compile.rs`: `test_build_mode_retains_modules` — `build_with(.., Build,
  |s| s.modules.len())` returns 2 for a two-module project.

**Done when** `nash-driver` tests pass and `cargo clippy` is clean.

---

## Chunk 7 — `nash build` command

**Files**

- `crates/nash-cli/src/cmd/build.rs` (new)
- `crates/nash-cli/src/cmd/mod.rs`
- `crates/nash-cli/Cargo.toml` (add `hex`)

**Change**

Mirror `cmd/check.rs` (`crates/nash-cli/src/cmd/check.rs:16-83`): load the
project, discover modules, build the graph, compile in `Build` mode, then
compile validators and write outputs.

**Code**

```rust
// crates/nash-cli/src/cmd/build.rs
use std::path::PathBuf;
use std::sync::Arc;

use miette::{IntoDiagnostic, Result};
use nash_config::{PlutusVersion, TraceLevel};
use nash_driver::{
    Database, FileSystemSource, Project, build_graph,
    build::{build_validators, write_outputs},
    compile::{CompileMode, build_with},
    source::path_to_uri,
};
use tokio::sync::Mutex;

#[derive(clap::Args)]
pub struct Args {
    /// Path to the project (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// How user `trace` calls compile: silent, compact, verbose
    #[arg(long, value_enum)]
    pub trace_level: Option<TraceLevelArg>,

    /// Keep compiler-generated traces (pattern match failures, `todo`)
    #[arg(long)]
    pub compiler_traces: bool,

    /// Core optimization level, 0..=2
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=2))]
    pub optimize: Option<u8>,

    /// Output directory, relative to the project root
    #[arg(long, default_value = "build")]
    pub out: PathBuf,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum TraceLevelArg { Silent, Compact, Verbose }

impl From<TraceLevelArg> for TraceLevel {
    fn from(a: TraceLevelArg) -> Self {
        match a { TraceLevelArg::Silent => TraceLevel::Silent, TraceLevelArg::Compact => TraceLevel::Compact, TraceLevelArg::Verbose => TraceLevel::Verbose }
    }
}

impl Args {
    pub async fn exec(self) -> Result<()> {
        let project = Project::load(&self.path).await.into_diagnostic()?;
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));

        let modules = project.discover_modules(&*db.lock().await).await.into_diagnostic()?;
        if modules.is_empty() {
            eprintln!("No Nash source files found.");
            return Ok(());
        }
        let graph = build_graph(db.clone(), &modules).await.into_diagnostic()?;
        eprintln!("   Compiling {} modules", graph.order.len());

        let mut config = project.config.build();
        if let Some(level) = self.trace_level { config.trace_level = level.into(); }
        if let Some(level) = self.optimize { config.optimize = level; }
        let compiler_traces = self.compiler_traces;

        let (result, validators) = build_with(db.clone(), &graph, CompileMode::Build, move |solved| {
            build_validators(solved, &config, compiler_traces)
        })
        .await;

        for warning in &result.warnings {
            eprintln!("Warning: {warning}");
        }
        if !result.is_success() {
            report_failures(&result);
            std::process::exit(1);
        }
        let validators = validators.into_diagnostic()?;

        let out_dir = path_to_uri(&project.root.join(&self.out)).into_diagnostic()?;
        write_outputs(&*db.lock().await, &out_dir, &validators).await.into_diagnostic()?;

        for v in &validators {
            eprintln!("    Building {} ({}/{}.cbor, {} bytes)", v.module, self.out.display(), v.module, v.cbor.len());
            eprintln!("             hash {}", hex::encode(v.hash));
        }
        eprintln!("    Finished {} validators", validators.len());
        Ok(())
    }
}

fn report_failures(result: &nash_driver::BuildResult) {
    eprintln!("Compilation failed: {} succeeded, {} failed", result.success, result.failed);
    for (uri, module_result) in &result.modules {
        if let nash_driver::ModuleResult::Failed { message } = module_result {
            eprintln!("\nError in {}:\n  {}", uri.path(), message);
        }
    }
}
```

```rust
// crates/nash-cli/src/cmd/mod.rs
pub mod build;

pub enum Cmd {
    #[clap(visible_alias = "c")]
    Check(check::Args),
    /// Compile validator modules to UPLC
    #[clap(visible_alias = "b")]
    Build(build::Args),
    Lsp(lsp::Args),
}
// exec: Cmd::Build(args) => args.exec().await,
```

Once plans/06 lands, `report_failures` is replaced by miette rendering of the
collected diagnostics; the exit code stays `1`. Config/IO errors surface as
miette errors and exit `2` via `main`.

**Tests**: none beyond `clap`'s `debug_assert` on `Cli` (add
`Cli::command().debug_assert()` to the existing cli tests if absent).

**Done when** `nash build` on `examples/vesting` (add a minimal example
project under `examples/`) writes three files and prints a hash.

---

## Chunk 8 — Driver integration tests with snapshots

**Files**

- `crates/nash-driver/tests/build.rs` (new)
- `crates/nash-driver/tests/snapshots/*` (insta)

**Change**

End-to-end tests with `InMemorySource` (`crates/nash-driver/src/source.rs:127`):
a validator plus a helper module, compiled in `Build` mode, with the UPLC
text snapshotted. Snapshot the text, not the flat bytes, so diffs are
readable.

**Code**

```rust
// crates/nash-driver/tests/build.rs
use std::sync::Arc;
use nash_driver::{Database, InMemorySource, build_graph, build::build_validators, compile::{CompileMode, build_with}};
use tokio::sync::Mutex;
use url::Url;

fn url(p: &str) -> Url { Url::parse(&format!("file:///proj/src/{p}")).unwrap() }

async fn build_uplc(files: &[(&str, &str)]) -> Vec<(String, String)> {
    let mem = InMemorySource::new();
    for (name, src) in files {
        mem.insert(url(name), src.to_string());
    }
    let db = Arc::new(Mutex::new(Database::new(mem)));
    let modules: Vec<Url> = files.iter().map(|(n, _)| url(n)).collect();
    let graph = build_graph(db.clone(), &modules).await.unwrap();
    let config = nash_config::Build::default();
    let (result, outputs) = build_with(db, &graph, CompileMode::Build, move |solved| {
        build_validators(solved, &config, false)
    })
    .await;
    assert!(result.is_success(), "{:?}", result.modules);
    outputs.unwrap().into_iter().map(|o| (o.module, o.uplc)).collect()
}

#[tokio::test]
async fn validator_with_helper_module() {
    let outputs = build_uplc(&[
        ("Helper.nash", indoc::indoc! {r#"
            module Helper exposing (isPositive)

            isPositive : int -> bool
            isPositive n = n > 0
        "#}),
        ("Main.nash", indoc::indoc! {r#"
            validator module Main exposing (main)

            import Helper exposing (isPositive)

            main : Int -> unit
            main n = assert (isPositive (lower n))

            tests
                test "positive" = do
                    assert (isPositive 1)
        "#}),
    ])
    .await;
    assert_eq!(outputs.len(), 1);
    insta::assert_snapshot!(outputs[0].1);
}

#[tokio::test]
async fn non_validator_modules_produce_no_output() {
    let outputs = build_uplc(&[("Lib.nash", "module Lib exposing (x)\n\nx = 1\n")]).await;
    assert!(outputs.is_empty());
}

#[tokio::test]
async fn missing_main_fails_build() {
    // Same harness, but assert `result.failed == 1` and the message contains "NO MAIN".
}

#[tokio::test]
async fn uplc_text_round_trips() {
    // parse each snapshot with nash_plutus::syn::parse_program and compare with flat::encode of the original.
}
```

**Done when** snapshots are accepted and `cargo insta test -p nash-driver`
is green. The snapshot for `validator_with_helper_module` is the first
committed UPLC output of the compiler; keep it small.

---

## Ordering and compile state

| Chunk | Compiles alone | Depends on |
|---|---|---|
| 1 | yes | plans/01 `ModuleKind` |
| 2 | yes | 1 |
| 3 | yes | 1, plans/02 |
| 4 | yes | — |
| 5 | yes | — |
| 6 | yes | 1–5, plans/07 Chunks 2, 3, 9, 11 |
| 7 | yes | 6 |
| 8 | yes | 7 |

Chunks 4 and 5 can land before 1.

## Open questions

- **Where `ModuleKind` lives.** `nash-ast` does not depend on `nash-source`
  today; putting the enum in `nash-region` avoids a new edge.
- **`Build::new` signature.** plans/07 Chunk 9 defines the codegen context;
  the constructor call in Chunk 6 is this plan's guess at its inputs (every
  solved module plus options).
- **Per-target rules.** `check_main_parameters` is target-independent in
  v1: no warning for a `Const` parameter on a Cardano validator, no switch
  for an L2 that passes non-`Data` arguments.
- **Stale output removal** needs `FileSource::remove`; the LSP overlay
  source ignores it.
