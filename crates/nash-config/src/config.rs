//! Project configuration types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::PackageName;

/// The default config file name.
pub const CONFIG_FILE_NAME: &str = "nash.jsonc";

/// A Nash project configuration, parsed from `nash.jsonc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Config {
    Application(Application),
    Package(Package),
    Workspace(Workspace),
}

/// Plutus ledger language version used for validator compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlutusVersion {
    V1,
    V2,
    #[default]
    V3,
}

/// Amount of user trace information included in compiled validators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TraceLevel {
    #[default]
    Silent,
    Compact,
    Verbose,
}

/// Project build settings, stored at the top level of `nash.jsonc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(from = "BuildFields", into = "BuildFields")]
pub struct Build {
    pub plutus_version: PlutusVersion,
    pub trace_level: TraceLevel,
    /// Distinguishes an explicit silent setting from the command-specific default.
    pub trace_level_explicit: bool,
    pub compiler_traces: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildFields {
    #[serde(default)]
    plutus_version: PlutusVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace_level: Option<TraceLevel>,
    #[serde(default)]
    compiler_traces: bool,
}

impl From<BuildFields> for Build {
    fn from(fields: BuildFields) -> Self {
        Self {
            plutus_version: fields.plutus_version,
            trace_level: fields.trace_level.unwrap_or_default(),
            trace_level_explicit: fields.trace_level.is_some(),
            compiler_traces: fields.compiler_traces,
        }
    }
}

impl From<Build> for BuildFields {
    fn from(build: Build) -> Self {
        Self {
            plutus_version: build.plutus_version,
            trace_level: (build.trace_level_explicit || build.trace_level != TraceLevel::Silent)
                .then_some(build.trace_level),
            compiler_traces: build.compiler_traces,
        }
    }
}

impl Build {
    pub fn for_tests(mut self) -> Self {
        if !self.trace_level_explicit {
            self.trace_level = TraceLevel::Verbose;
        }
        self.compiler_traces = true;
        self
    }
}

/// An application project configuration.
///
/// Applications are executables that compile to UPLC validators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    /// Validator build settings.
    #[serde(flatten)]
    pub build: Build,

    /// Required compiler version (semver).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<String>,

    /// Source directories. Defaults to `["src"]` if not specified.
    #[serde(default = "default_source_dirs")]
    pub source_directories: Vec<String>,

    /// Direct dependencies with version constraints or source references.
    #[serde(default)]
    pub dependencies: BTreeMap<PackageName, Dependency>,

    /// Test dependencies with version constraints or source references.
    #[serde(default)]
    pub test_dependencies: BTreeMap<PackageName, Dependency>,
}

fn default_source_dirs() -> Vec<String> {
    vec!["src".to_string()]
}

/// A package (library) configuration.
///
/// Packages can be published and used as dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    /// Package build settings.
    #[serde(flatten)]
    pub build: Build,

    /// Required compiler version (semver).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<String>,

    /// Package name in `author/project` format.
    pub name: PackageName,

    /// Package version (semver).
    pub version: String,

    /// Short description (should be under 80 characters).
    pub summary: String,

    /// SPDX license identifier.
    pub license: String,

    /// Modules exposed by this package.
    pub exposed_modules: ExposedModules,

    /// Dependencies with version constraints or source references.
    #[serde(default)]
    pub dependencies: BTreeMap<PackageName, Dependency>,

    /// Test dependencies with version constraints or source references.
    #[serde(default)]
    pub test_dependencies: BTreeMap<PackageName, Dependency>,
}

/// Exposed modules can be a flat list or categorized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExposedModules {
    /// A flat list of module names.
    List(Vec<String>),
    /// Modules organized by category.
    Categorized(BTreeMap<String, Vec<String>>),
}

impl ExposedModules {
    /// Get all exposed module names as a flat list.
    pub fn flatten(&self) -> Vec<&str> {
        match self {
            ExposedModules::List(modules) => modules.iter().map(|s| s.as_str()).collect(),
            ExposedModules::Categorized(categories) => {
                categories.values().flatten().map(|s| s.as_str()).collect()
            }
        }
    }
}

/// Workspace configuration.
///
/// A workspace is a collection of related projects that share dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Required compiler version (semver).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<String>,

    /// Glob patterns for workspace members.
    pub members: Vec<String>,

    /// Dependencies available for members to inherit via `{ "workspace": true }`.
    #[serde(default)]
    pub dependencies: BTreeMap<PackageName, Dependency>,
}

impl Config {
    /// Returns application or package build settings, or defaults for workspaces.
    pub fn build(&self) -> Build {
        match self {
            Config::Application(app) => app.build,
            Config::Package(pkg) => pkg.build,
            Config::Workspace(_) => Build::default(),
        }
    }

    /// Returns the `compiler` version requirement, if specified.
    pub fn compiler(&self) -> Option<&str> {
        match self {
            Config::Application(app) => app.compiler.as_deref(),
            Config::Package(pkg) => pkg.compiler.as_deref(),
            Config::Workspace(ws) => ws.compiler.as_deref(),
        }
    }
}

// ============================================================================
// Dependencies
// ============================================================================

/// A dependency specification.
///
/// Can be either a version constraint string or a structured source reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// Version constraint string: "1.0.0 <= v < 2.0.0"
    Constraint(String),
    /// Structured dependency source (workspace, path, git)
    Source(DependencySource),
}

/// Structured dependency source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySource {
    /// Inherit from workspace root: `{ "workspace": true }`
    Workspace(WorkspaceDep),
    /// Path to local package: `{ "path": "../my-lib" }`
    Path(PathDep),
    /// Git repository: `{ "git": "https://...", "branch": "main" }`
    Git(GitDep),
}

/// Workspace dependency marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDep {
    /// Must be `true` to inherit from workspace.
    pub workspace: bool,
}

/// Path-based dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathDep {
    /// Relative path to the package.
    pub path: String,
}

/// Git-based dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDep {
    /// Git repository URL.
    pub git: String,
    /// Branch to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Tag to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Specific revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
}

impl Dependency {
    /// Returns `true` if this is a workspace dependency.
    pub fn is_workspace(&self) -> bool {
        matches!(self, Dependency::Source(DependencySource::Workspace(_)))
    }

    /// Returns `true` if this is a path dependency.
    pub fn is_path(&self) -> bool {
        matches!(self, Dependency::Source(DependencySource::Path(_)))
    }

    /// Returns `true` if this is a git dependency.
    pub fn is_git(&self) -> bool {
        matches!(self, Dependency::Source(DependencySource::Git(_)))
    }

    /// Returns the version constraint if this is a constraint dependency.
    pub fn as_constraint(&self) -> Option<&str> {
        match self {
            Dependency::Constraint(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test_defaults {
    use super::*;

    #[test]
    fn tests_distinguish_absent_trace_setting_from_explicit_silent() {
        for source in [
            r#"{"type":"application"}"#,
            r#"{"type":"application","traceLevel":"silent"}"#,
        ] {
            let config = crate::parse(source, "nash.jsonc").unwrap();
            let explicit = source.contains("traceLevel");
            let build = config.build();
            assert_eq!(build.trace_level, TraceLevel::Silent);
            assert_eq!(
                build.for_tests().trace_level,
                if explicit {
                    TraceLevel::Silent
                } else {
                    TraceLevel::Verbose
                }
            );
            assert!(build.for_tests().compiler_traces);
            let encoded = serde_json::to_string(&config).unwrap();
            assert_eq!(crate::parse(&encoded, "nash.jsonc").unwrap(), config);
            assert_eq!(serde_json::from_str::<Config>(&encoded).unwrap(), config);
        }
    }
}
