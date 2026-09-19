# nash-config

Configuration file parsing for Nash projects.

This crate handles parsing and validation of `nash.jsonc` configuration files with accurate error messages including line and column positions.

## JSON Schema

For IDE support and validation, use the [JSON Schema](https://github.com/nash-script/compiler/blob/main/crates/nash-config/nash.schema.json):

```jsonc
{
    "$schema": "https://raw.githubusercontent.com/nash-script/compiler/main/crates/nash-config/nash.schema.json",
    "type": "application",
    // ...
}
```

## Config Types

Nash supports three configuration types:

### Application

An application compiles to UPLC validators for the Cardano blockchain.

```jsonc
{
    "type": "application",
    "sourceDirectories": ["src"],
    "dependencies": {
        "example/math": "1.0.0 <= v < 2.0.0",
        "alice/json": { "workspace": true }
    },
    "testDependencies": {
        "nash/test": "1.0.0 <= v < 2.0.0"
    }
}
```

### Package

A publishable library that can be used as a dependency.

```jsonc
{
    "type": "package",
    "name": "alice/json-parser",
    "version": "1.0.0",
    "summary": "A JSON parser for Nash",
    "license": "MIT",
    "exposedModules": ["Json", "Json.Decode", "Json.Encode"],
    "dependencies": {
        "example/math": "1.0.0 <= v < 2.0.0"
    }
}
```

Exposed modules can also be categorized:

```jsonc
{
    "exposedModules": {
        "Decoding": ["Json.Decode", "Json.Decode.Pipeline"],
        "Encoding": ["Json.Encode"]
    }
}
```

### Workspace

A collection of related projects that share dependencies.

```jsonc
{
    "type": "workspace",
    "members": ["packages/*", "apps/my-app"],
    "dependencies": {
        "example/math": "1.0.0 <= v < 2.0.0",
        "alice/json": "2.0.0 <= v < 3.0.0"
    }
}
```

Members can inherit workspace dependencies:

```jsonc
// packages/my-lib/nash.jsonc
{
    "type": "package",
    "name": "bob/my-lib",
    "version": "1.0.0",
    "summary": "My library",
    "license": "MIT",
    "exposedModules": ["MyLib"],
    "dependencies": {
        "example/math": { "workspace": true }
    }
}
```

## Build Settings

Applications and packages accept these optional top-level fields:

| Field | Values | Default |
| --- | --- | --- |
| `plutusVersion` | `"v1"`, `"v2"`, `"v3"` | `"v3"` |
| `traceLevel` | `"silent"`, `"compact"`, `"verbose"` | `"silent"` |
| `compilerTraces` | `true`, `false` | `false` |

`Config::build()` returns these settings. Workspace configs return defaults;
set build fields on each member. `parse` and `parse_file` reject build fields
on workspace configs and reject `optimize` on every config type because the
optimizer is not implemented. Other unknown fields retain the parser's existing
ignore policy; the JSON Schema rejects unknown fields. Use `parse` or `parse_file`
for full configuration validation; direct Serde deserialization retains its
existing unknown-field policy.

## Dependency Types

Dependencies can be specified in several ways:

### Version Constraint

```jsonc
"example/math": "1.0.0 <= v < 2.0.0"
```

### Workspace Inheritance

Inherit the constraint from the workspace root:

```jsonc
"example/math": { "workspace": true }
```

### Path Dependency

Reference a local package:

```jsonc
"my-lib": { "path": "../packages/my-lib" }
```

### Git Dependency

Reference a git repository:

```jsonc
"alice/experimental": {
    "git": "https://github.com/alice/experimental",
    "branch": "main"
}
```

Git dependencies support `branch`, `tag`, or `rev` (commit hash).

## Package Names

Package names follow the `author/project` format:

- Both parts must start with a lowercase letter
- Can contain lowercase letters, digits, and hyphens
- Cannot end with a hyphen or have consecutive hyphens

Examples: `example/math`, `alice/json-parser`, `bob123/my-lib2`

## Usage

```rust
use nash_config::{parse_file, Config};

fn main() -> Result<(), nash_config::ConfigError> {
    let config = parse_file("nash.jsonc")?;

    match config {
        Config::Application(app) => {
            println!("Application with {} dependencies", app.dependencies.len());
        }
        Config::Package(pkg) => {
            println!("Package: {}", pkg.name);
        }
        Config::Workspace(ws) => {
            println!("Workspace with {} members", ws.members.len());
        }
    }

    Ok(())
}
```

## License

Apache-2.0
