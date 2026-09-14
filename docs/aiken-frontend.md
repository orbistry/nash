# Architecture decision record

## Decision

Add a frontend-neutral boundary at `nash_source::Module` and an owned inspection
boundary for dependency discovery.

```text
                             dependency graph
source + project metadata -> Frontend::inspect -> ModuleDependency[] -> ModuleCatalog
              │
              └-----------> Frontend::parse -------------------------------┐
                                                                           ▼
                                                              nash_source::Module
                                                                           │
                                                canonicalize -> solve -> nitpick
                                                                           │
                                                                      interface
```

## Why `nash_source::Module`

This boundary is early enough for Nash to own name resolution, kinds, runtime
representations, traits, type inference, exhaustiveness, and code generation. It is
late enough to hide parser-specific trees from the driver and the compiler middle
end.

Do not lower Aiken directly to Nash's canonical AST. That would duplicate name
resolution and representation checks. Do not lower Aiken directly to Core. That
would duplicate most of the Nash semantic frontend.

## Why inspection is separate

The current driver creates the graph before it runs the compiler pipeline. It also
parses source once for imports and again for compilation. A separate owned inspection
result makes this existing split frontend-neutral.

`Frontend::inspect` returns only canonical module dependencies and diagnostics. It
must not return arena references. `Frontend::parse` returns the full arena-backed
source AST.

The Aiken adapter can inspect imports without lowering every expression. If
inspection fails, the driver keeps that module with no discovered dependencies and
retains the diagnostics. Independent modules can still enter the graph.

## Crate responsibilities

### `nash-frontend`

- Defines `Frontend`.
- Defines `SourceInput`, `InspectOutput`, and `ParseOutput`.
- Defines canonical `ModuleName` and `ModuleDependency`.
- Defines owned frontend diagnostics.
- Selects adapters by explicit ID or file extension.
- Has no parser-specific behavior.

### `nash-frontend-nash`

- Wraps `nash_parse::Parser`.
- Validates the required module header, path-derived module name, and module role.
- Copies imports into owned dependency records during inspection.
- Converts parser failures into frontend diagnostics.

### `nash-frontend-aiken`

- Calls the official Aiken parser.
- Converts Aiken byte spans into Nash one-based byte-column regions.
- Applies a named compatibility profile.
- Normalizes Aiken module paths.
- Lowers the untyped Aiken AST into `nash_source::Module`.
- Rejects unsupported encoding decorators before partial lowering.
- Synthesizes validator dispatch only after that behavior has conformance tests.

### `nash-driver`

- Reads files and project configuration.
- Derives `ModuleName`, `ModuleRole`, package identity, and frontend choice.
- Uses `Frontend::inspect` when it builds the dependency graph.
- Resolves canonical imports through `ModuleCatalog`.
- Uses `Frontend::parse` inside the existing build-wide arena.
- Converts frontend diagnostics to `nash-report` values.

### `nash-report`

- Remains unchanged.
- Receives owned `Report` values from the driver adapter.

## Module identity and resolution

A frontend normalizes syntax. It does not resolve packages or files.

```text
Aiken `aiken/list` -> canonical `aiken.list`
Nash  `Aiken.List` -> canonical spelling selected by Nash project rules
canonical name     -> ModuleCatalog -> source URI
```

The source specification also retains Nash's existing optional package owner so the
driver can pass it to canonicalization. The production catalog key should include
package identity when two packages can provide the same module name. That concern
stays in the driver or project layer, not in either parser adapter.

## Lifetime model

Dependency inspection returns owned values and uses no persistent arena.

Compilation uses Nash's current build-wide arena:

```text
build arena
├── source text copied for parsers that borrow it
├── nash_source::Module produced by the selected frontend
├── canonical nodes
├── solved annotations and evidence
└── interfaces
```

This matches the current requirement that canonical node addresses remain stable for
solved evidence. It also avoids a self-referential object that owns a `Bump` and
references data in that `Bump`.

## Compatibility profile

A profile keeps compatibility policy out of lowering code. It controls:

1. Whether a source feature can lower.
2. How imported modules map into Nash module space.
3. How built-in type names map.
4. How built-in value names map.

The first profile is `NashV1Profile`. It maps Aiken primitive representations to the
correct Nash constant types. It rejects custom constructor tags, list data encodings,
tests, benchmarks, environment modules, configuration modules, and multiple validator
declarations.

A later profile can map Aiken standard-library modules to Nash compatibility modules
without changing the frontend trait.

## Diagnostics

Frontend diagnostics are owned and module-local. The driver retains the URI, module
name, path, and source text, then wraps diagnostics in `nash_report::ModuleReports`.

Each diagnostic has:

- A stable code.
- Error or warning severity.
- A title and message.
- An optional primary region.
- Secondary labels.
- Help messages.

Examples:

```text
NAF1001  no frontend registered for .ak
NAF2001  Aiken syntax error
NAF2101  integer does not fit the temporary i128 source AST
NAF2201  custom @tag encoding is not supported
NAF2301  Aiken validator lowering is not implemented
NNF2001  Nash syntax error
```

## Rejected designs

### Full Aiken source tree in Nash

Rejected because it couples Nash to Aiken's complete compiler, build graph, release
process, and toolchain.

### Independent Aiken parser first

Rejected because grammar drift and error-recovery differences would become Nash's
maintenance burden before the lowering semantics are proven.

### Aiken AST in the driver

Rejected because it would leak parser-specific types into project resolution,
diagnostics, and compilation.

### Direct canonical AST or Core lowering

Rejected because it would bypass or duplicate Nash semantic stages.

### Extension-based import resolution

Rejected because one graph can contain `.nash` and `.ak` sources, and package
resolution must not depend on a source file suffix.

## First-version non-goals

- Exact Aiken package compatibility.
- Exact blueprint generation.
- Custom Aiken data encodings.
- Aiken tests and benchmarks.
- Aiken type inference.
- Calling the Aiken CLI during normal compilation.
- Keeping Aiken AST nodes after the adapter returns.

# Interface guide

## `Frontend`

```rust
pub trait Frontend: Send + Sync {
    fn descriptor(&self) -> &'static FrontendDescriptor;

    fn inspect(
        &self,
        input: SourceInput<'_, '_>,
    ) -> Result<InspectOutput, FrontendFailure>;

    fn parse<'arena>(
        &self,
        arena: &'arena Bump,
        input: SourceInput<'arena, '_>,
    ) -> Result<ParseOutput<'arena>, FrontendFailure>;
}
```

### Invariants

- `inspect` returns owned data.
- `parse` returns only `nash_source` nodes allocated in `arena`.
- A parser-specific AST never crosses the trait boundary.
- A frontend does not read files, resolve packages, or choose module identity.
- A failure contains at least one diagnostic.
- A successful parse can still return warnings.

## `SourceInput`

```rust
pub struct SourceInput<'source, 'context> {
    pub source: &'source str,
    pub uri: &'context Url,
    pub expected_module: &'context ModuleName,
    pub role: ModuleRole,
}
```

The project layer owns module identity. Aiken source does not declare a module header,
so the adapter assigns `expected_module` to the temporary Aiken AST before lowering.
Native Nash source validates its required module header against this value. The frontend reports a missing header before canonicalization.

## `InspectOutput`

```rust
pub struct InspectOutput {
    pub dependencies: Vec<ModuleDependency>,
    pub diagnostics: Vec<FrontendDiagnostic>,
}

pub struct ModuleDependency {
    pub module: ModuleName,
    pub region: Region,
}
```

Dependencies are canonical and owned. Their source regions let the driver report an
unknown or ambiguous import at the correct location.

## `ParseOutput`

```rust
pub struct ParseOutput<'arena> {
    pub module: &'arena nash_source::Module<'arena>,
    pub diagnostics: Vec<FrontendDiagnostic>,
}
```

The existing compiler pipeline should consume `module` without knowing which parser
created it.

## `ModuleName`

`ModuleName` uses `.` as its internal separator. Adapters normalize source spelling:

```text
aiken/list -> aiken.list
Aiken.List  -> Aiken.List
```

The driver resolves that identity through `ModuleCatalog`. It must not append a fixed
file extension. `SourceSpec` retains the existing optional package owner for the
canonicalization context; that package value does not cross the frontend trait.

## `CompatibilityProfile`

```rust
pub trait CompatibilityProfile: Send + Sync + 'static {
    fn feature(&self, feature: AikenFeature) -> FeatureDecision;
    fn map_module_name(&self, module: &str) -> ModuleMapping;
    fn map_type_name(&self, module: Option<&str>, name: &str) -> NameMapping;
    fn map_value_name(&self, module: Option<&str>, name: &str) -> NameMapping;
}
```

This interface separates syntax conversion from compatibility policy. For example,
`NashV1Profile` maps Aiken `Int` to Nash `int` and can later map `aiken.list` to a Nash
compatibility module.

## Composition functions

These functions contain orchestration only:

- `inspect_with_registry`: select adapter, then inspect.
- `parse_with_registry`: select adapter, then parse.
- `inspect_native_module`: parse native Nash, then copy dependencies.
- `parse_native_module`: run parser, validate role, validate module name.
- `inspect_source`: parse Aiken, then inspect imports.
- `normalize_module_path`: normalize Aiken segments, then apply the profile mapping.
- `parse_and_lower`: parse Aiken, then lower.
- `Lowerer::lower`: run preflight, then compose all declaration lowerers.
- `inspect_sources`: apply `inspect_loaded_source` to all loaded files and retain per-module failures.
- `frontend_module_reports`: map diagnostics, then wrap and sort reports.

Keeping these functions free of leaf logic makes control flow easy to test.

## Unimplemented leaf functions

Every `todo!` has a purpose comment and one example. The main stubs are:

```text
lower_use
lower_annotation / lower_type
lower_pattern
lower_integer
lower_expr
lower_function / lower_constant
lower_data_type / lower_type_alias
lower_validator
lower_exports / lower_docs
build_graph_from_inspection
compile_parsed_module
```

Implement leaf lowerers before changing orchestration. This keeps each pull request
small and lets fixture tests target one semantic mapping at a time.

## Dependency constraints

`nash-frontend` depends only on stable Nash surface types and small utility crates.
It must not depend on:

```text
nash-parse
aiken-lang
nash-can
nash-solve
nash-report
nash-driver
```

`nash-frontend-aiken` is the only crate allowed to depend on `aiken-lang`. This rule
makes a later switch to `aiken-syntax` local to one manifest and one adapter.
