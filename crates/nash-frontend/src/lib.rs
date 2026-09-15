//! Parser-neutral source boundary. Adapters own syntax; Nash owns semantics.

mod entries;
mod project;
pub use entries::{SourceBoundaryBinding, SourceEntryPoint, SourceHandler};
pub use project::{
    LoadedPackage, LoadedProject, ModuleCatalog, ModuleKey, PackageId, PackageSourceId,
    ProjectDiagnostic, ProjectFormat, ProjectLoadRequest, ProjectMetadata, ProjectMode,
    RepositoryMetadata, ResolvedDependency, SourceOrigin, SourceSpec,
};

use bumpalo::Bump;
use nash_region::Region;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleName(String);

impl ModuleName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleRole {
    Library,
    Validator,
    Environment,
    Configuration,
}

#[derive(Clone, Copy)]
pub struct SourceInput<'source, 'context> {
    pub source: &'source str,
    pub uri: &'context Url,
    pub expected_module: &'context ModuleName,
    /// None lets syntax determine the role; Some enforces project metadata.
    pub role: Option<ModuleRole>,
    pub origin: SourceOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Label {
    pub region: Region,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct FrontendDiagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub title: String,
    pub message: String,
    pub region: Option<Region>,
    pub primary_label: Option<String>,
    pub labels: Vec<Label>,
    pub context: Option<Region>,
    pub help: Vec<String>,
    pub suggestions: Vec<String>,
}

impl FrontendDiagnostic {
    pub fn error(
        code: &'static str,
        title: impl Into<String>,
        message: impl Into<String>,
        region: Option<Region>,
    ) -> Self {
        Self {
            code,
            severity: Severity::Error,
            title: title.into(),
            message: message.into(),
            region,
            primary_label: region.map(|_| String::new()),
            labels: Vec::new(),
            context: region,
            help: Vec::new(),
            suggestions: Vec::new(),
        }
    }
}

/// Failure is nonempty by construction; parser recovery may add diagnostics.
#[derive(Clone, Debug)]
pub struct FrontendFailure {
    pub first: Box<FrontendDiagnostic>,
    pub rest: Vec<FrontendDiagnostic>,
}

impl From<FrontendDiagnostic> for FrontendFailure {
    fn from(first: FrontendDiagnostic) -> Self {
        Self {
            first: Box::new(first),
            rest: Vec::new(),
        }
    }
}

impl FrontendFailure {
    pub fn diagnostics(&self) -> impl Iterator<Item = &FrontendDiagnostic> {
        std::iter::once(self.first.as_ref()).chain(&self.rest)
    }
}

#[derive(Clone, Debug)]
pub struct ModuleDependency {
    pub module: ModuleName,
    pub region: Region,
}

#[derive(Debug)]
pub struct InspectOutput {
    pub dependencies: Vec<ModuleDependency>,
    pub diagnostics: Vec<FrontendDiagnostic>,
}

pub struct ParseOutput<'arena> {
    pub module: &'arena nash_source::Module<'arena>,
    pub diagnostics: Vec<FrontendDiagnostic>,
    pub entry_points: &'arena [SourceEntryPoint<'arena>],
    /// Require public value and constructor signatures to expose no private nominal type.
    pub reject_private_types_in_exports: bool,
}

pub struct FrontendDescriptor {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
}

pub trait Frontend: Send + Sync {
    fn descriptor(&self) -> &'static FrontendDescriptor;
    fn inspect(&self, input: SourceInput<'_, '_>) -> Result<InspectOutput, FrontendFailure>;
    fn parse<'arena>(
        &self,
        arena: &'arena Bump,
        input: SourceInput<'arena, '_>,
    ) -> Result<ParseOutput<'arena>, FrontendFailure>;
}

/// Registration is composed by the driver; this crate never imports adapters.
pub struct FrontendRegistry {
    frontends: &'static [&'static dyn Frontend],
}

impl FrontendRegistry {
    pub const fn new(frontends: &'static [&'static dyn Frontend]) -> Self {
        Self { frontends }
    }

    pub fn select(
        &self,
        uri: &Url,
        id: Option<&str>,
    ) -> Result<&'static dyn Frontend, FrontendFailure> {
        let extension = std::path::Path::new(uri.path())
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("");
        self.frontends
            .iter()
            .copied()
            .find(|frontend| {
                let descriptor = frontend.descriptor();
                id.map_or_else(
                    || descriptor.extensions.contains(&extension),
                    |id| descriptor.id == id,
                )
            })
            .ok_or_else(|| {
                FrontendDiagnostic::error(
                    "NAF1001",
                    "UNKNOWN FRONTEND",
                    format!("No frontend registered for {}.", id.unwrap_or(extension)),
                    None,
                )
                .into()
            })
    }

    pub fn extensions(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.frontends
            .iter()
            .flat_map(|frontend| frontend.descriptor().extensions.iter().copied())
    }
}
