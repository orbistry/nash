use std::path::Path;

use aiken_lang::format::{Formatter, MAX_COLUMNS};
use aiken_project::config::ProjectConfig;
use nash_frontend::{ModuleCatalog, ModuleName, ModuleRole, ProjectDiagnostic};
use url::Url;

use crate::discovery::{PackageSources, insert};

pub(crate) fn add(
    root: &Path,
    config: &ProjectConfig,
    sources: &PackageSources<'_>,
    catalog: &mut ModuleCatalog,
    warnings: &mut Vec<ProjectDiagnostic>,
) -> Result<(), ProjectDiagnostic> {
    if config.config.is_empty() {
        return Ok(());
    }
    let Some(values) = config.config.get(sources.environment) else {
        warnings.push(ProjectDiagnostic::new(
            "NAP4108",
            root.join("aiken.toml"),
            format!(
                "no configuration exists for environment {:?}; no config module was generated",
                sources.environment
            ),
        ));
        return Ok(());
    };
    // Use precisely SimpleExpr's inferred annotations: uniform arrays are lists,
    // heterogeneous arrays are tuples, and the empty array is List<Data>.
    let definitions: Vec<_> = values
        .iter()
        .map(|(name, value)| value.as_definition(name))
        .collect();
    let source = Formatter::new()
        .definitions(&definitions)
        .to_pretty_string(MAX_COLUMNS);
    let path = root.join("config.ak");
    let uri = Url::from_file_path(&path).map_err(|()| {
        ProjectDiagnostic::new(
            "NAP4104",
            &path,
            "generated config path cannot be represented as a file URI",
        )
    })?;
    insert(
        catalog,
        uri,
        sources.spec(
            root,
            ModuleName::new("config"),
            ModuleRole::Configuration,
            Some(source),
        ),
    )
}
