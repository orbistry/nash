//! Aiken's project and package syntax stops here; the returned catalog is Nash-owned.

mod config_module;
mod discovery;
mod manifest;
mod packages;

pub use packages::{
    AikenPackageAccess, PackageAccess, PackageLocation, PackageRequest, PreparedPackages,
};

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use nash_frontend::{
    LoadedPackage, LoadedProject, ModuleCatalog, PackageId, PackageSourceId, ProjectDiagnostic,
    ProjectLoadRequest,
};
use url::Url;

use discovery::PackageSources;

/// Load the closest owning Aiken manifest (or a directly selected aiken.toml).
/// Call from a blocking context: manifest, package cache and network IO are synchronous.
pub fn load(request: ProjectLoadRequest<'_>) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    load_with_sources(request, &[])
}

/// Include unsaved file URIs in normal source discovery; source text stays in the driver.
pub fn load_with_sources(
    request: ProjectLoadRequest<'_>,
    additional_sources: &[Url],
) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    load_project(request, &AikenPackageAccess, additional_sources)
}

/// Load with a Nash-owned package provider, including fully offline prepared caches.
pub fn load_with_package_access(
    request: ProjectLoadRequest<'_>,
    access: &dyn PackageAccess,
) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    load_project(request, access, &[])
}

fn load_project(
    request: ProjectLoadRequest<'_>,
    access: &dyn PackageAccess,
    additional_sources: &[Url],
) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    let root = manifest::root(request.location).map_err(|error| vec![error])?;
    let members = manifest::members(&root).map_err(|error| vec![error])?;
    let additional_sources = additional_sources
        .iter()
        .map(|uri| {
            uri.to_file_path().map_err(|()| {
                ProjectDiagnostic::new(
                    "NAP4104",
                    &root,
                    format!("additional source is not a file URI: {uri}"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| vec![error])?;
    let mut project = LoadedProject {
        root,
        packages: Vec::new(),
        catalog: ModuleCatalog::new(),
        warnings: Vec::new(),
    };
    let mut errors = Vec::new();
    let mut names = BTreeSet::new();
    for member in members {
        match load_member(
            &member,
            request.environment.unwrap_or("default"),
            access,
            &additional_sources,
        ) {
            Ok(loaded) => {
                if let Some(root_package) = loaded.packages.first()
                    && !names.insert(root_package.id.name.clone())
                {
                    errors.push(ProjectDiagnostic::new(
                        "NAP4102",
                        &member,
                        "workspace members have duplicate package names",
                    ));
                    continue;
                }
                project.packages.extend(loaded.packages);
                project.warnings.extend(loaded.warnings);
                for (uri, spec) in loaded.catalog {
                    if let Err(error) = discovery::insert(&mut project.catalog, uri, spec) {
                        errors.push(error);
                    }
                }
            }
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(project)
    } else {
        Err(errors)
    }
}

fn load_member(
    root: &Path,
    environment: &str,
    access: &dyn PackageAccess,
    additional_sources: &[PathBuf],
) -> Result<LoadedProject, ProjectDiagnostic> {
    let config = manifest::config(root)?;
    let metadata = manifest::metadata(&config);
    let id = PackageId {
        name: metadata.name.clone(),
        version: metadata.version.clone(),
        source: PackageSourceId::Local(root.to_path_buf()),
    };
    let requirements = manifest::requirements(&config);
    let mut warnings = Vec::new();
    let dependencies = access.resolve(PackageRequest {
        root,
        package: &id,
        requirements: &requirements,
        warnings: &mut warnings,
    })?;
    let visible: Vec<_> = std::iter::once(id.clone())
        .chain(
            dependencies
                .iter()
                .map(|dependency| dependency.package.clone()),
        )
        .collect();
    let direct = dependencies
        .iter()
        .filter(|dependency| {
            requirements
                .iter()
                .any(|requested| requested.name == dependency.package.name)
        })
        .map(|dependency| dependency.package.clone())
        .collect();
    let mut project = LoadedProject {
        root: root.to_path_buf(),
        packages: vec![LoadedPackage {
            id: id.clone(),
            root: root.to_path_buf(),
            metadata: metadata.clone(),
            dependencies: direct,
        }],
        catalog: ModuleCatalog::new(),
        warnings,
    };
    let current = aiken_project::config::compiler_version(false);
    if metadata.compiler != current {
        project.warnings.push(ProjectDiagnostic::new(
            "NAP4109",
            root.join("aiken.toml"),
            format!(
                "project requests compiler {}; the Aiken syntax adapter is pinned to {current}",
                metadata.compiler
            ),
        ));
    }
    let sources = PackageSources {
        package: &id,
        metadata: Arc::new(metadata),
        visible: &visible,
        environment,
        dependency: false,
        resolution_root: root,
        additional_sources,
    };
    sources.discover(root, &mut project.catalog, &mut project.warnings)?;
    config_module::add(
        root,
        &config,
        &sources,
        &mut project.catalog,
        &mut project.warnings,
    )?;
    for dependency in dependencies {
        let config = manifest::config(&dependency.root)?;
        let metadata = manifest::metadata(&config);
        if metadata.name != dependency.package.name {
            return Err(ProjectDiagnostic::new(
                "NAP4110",
                dependency.root.join("aiken.toml"),
                format!(
                    "package manifest name {:?} does not match locked name {:?}",
                    metadata.name, dependency.package.name
                ),
            ));
        }
        let sources = PackageSources {
            package: &dependency.package,
            metadata: Arc::new(metadata.clone()),
            visible: &visible,
            environment,
            dependency: true,
            resolution_root: root,
            additional_sources,
        };
        sources.discover(
            &dependency.root,
            &mut project.catalog,
            &mut project.warnings,
        )?;
        let declared = manifest::requirements(&config);
        let dependency_ids = visible
            .iter()
            .filter(|id| {
                **id != dependency.package
                    && declared
                        .iter()
                        .any(|requested| requested.name == id.name && requested.source == id.source)
            })
            .cloned()
            .collect();
        project.packages.push(LoadedPackage {
            id: dependency.package,
            root: dependency.root,
            metadata,
            dependencies: dependency_ids,
        });
    }
    Ok(project)
}
