//! Run documentation extraction through the same solved frontend as builds.
use crate::{DocsWarning, ModuleDocs, extract, primitives};
use nash_config::Config;
use nash_driver::{
    Database, DriverError, FileSystemSource, Project, build_graph_production, build_with,
    bundled_base,
};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::Arc,
};
use tokio::sync::Mutex;

pub struct Documentation {
    pub compiler: nash_driver::BuildResult,
    pub modules: Vec<ModuleDocs>,
    pub warnings: Vec<DocsWarning>,
}

pub enum Input<'a> {
    Base,
    Project(&'a Path),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Driver(#[from] DriverError),
    #[error(
        "module {0} appears in multiple packages; generate documentation for each package separately"
    )]
    DuplicateModule(String),
}

pub async fn build(input: Input<'_>) -> Result<Documentation, Error> {
    let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
    let base = matches!(input, Input::Base);
    let mut packages = BTreeMap::new();
    let (origins, roots) = match input {
        Input::Base => {
            let modules = bundled_base::modules();
            let roots = modules.keys().cloned().collect::<HashSet<_>>();
            (modules, roots)
        }
        Input::Project(path) => {
            let project = Project::load(path).await?;
            for member in &project.members {
                if let Config::Package(package) = &member.config {
                    packages.insert(
                        package.name.to_string(),
                        package
                            .exposed_modules
                            .flatten()
                            .into_iter()
                            .map(str::to_owned)
                            .collect::<HashSet<_>>(),
                    );
                }
            }
            let database = db.lock().await;
            let origins = project.discover_modules_production(&database).await?;
            let roots = project
                .discover_own_modules(&database)
                .await?
                .into_keys()
                .collect::<HashSet<_>>();
            (origins, roots)
        }
    };
    let graph =
        build_graph_production(db.clone(), &origins.keys().cloned().collect::<Vec<_>>()).await?;
    let selected_roots = roots.clone();
    let (compiler, extracted) = build_with(db, &graph, &origins, move |solved| {
        solved
            .modules
            .iter()
            .filter(|m| {
                selected_roots.contains(&m.uri)
                    && (base
                        || m.module.name.package.is_none_or(|p| {
                            packages
                                .get(&format!("{}/{}", p.author, p.project))
                                .is_some_and(|exports| exports.contains(m.module.name.name))
                        }))
            })
            .map(|module| {
                let parsed = nash_parse::Parser::new(solved.store, module.source)
                    .module()
                    .expect("solved source parses");
                let interface =
                    nash_can::from_module(solved.store, module.module, &module.annotations);
                extract(&parsed, &interface)
            })
            .collect::<Vec<_>>()
    })
    .await;
    // The driver backend keeps one solved module per name. Detect collisions
    // from its URI-keyed interfaces before an overwritten module can disappear.
    for (uri, interface) in &compiler.interfaces {
        if roots.contains(uri)
            && compiler.interfaces.iter().any(|(other_uri, other)| {
                uri != other_uri && interface.module_name == other.module_name
            })
        {
            return Err(Error::DuplicateModule(interface.module_name.clone()));
        }
    }
    let mut modules = Vec::new();
    let mut warnings = Vec::new();
    for extraction in extracted.unwrap_or_default() {
        modules.push(extraction.module);
        warnings.extend(extraction.warnings);
    }
    if base && compiler.is_success() {
        modules.extend(primitives());
    }
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Documentation {
        compiler,
        modules,
        warnings,
    })
}
