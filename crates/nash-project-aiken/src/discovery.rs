use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use nash_frontend::{
    ModuleCatalog, ModuleKey, ModuleName, ModuleRole, PackageId, ProjectDiagnostic,
    ProjectMetadata, SourceOrigin, SourceSpec,
};
use url::Url;
use walkdir::WalkDir;

pub(crate) struct PackageSources<'a> {
    pub package: &'a PackageId,
    pub metadata: Arc<ProjectMetadata>,
    pub visible: &'a [PackageId],
    pub environment: &'a str,
    pub dependency: bool,
    pub resolution_root: &'a Path,
    pub additional_sources: &'a [PathBuf],
}

impl PackageSources<'_> {
    pub(crate) fn discover(
        &self,
        root: &Path,
        catalog: &mut ModuleCatalog,
        warnings: &mut Vec<ProjectDiagnostic>,
    ) -> Result<(), ProjectDiagnostic> {
        self.directory(&root.join("lib"), ModuleRole::Library, catalog, warnings)?;
        if !self.dependency {
            self.directory(
                &root.join("validators"),
                ModuleRole::Validator,
                catalog,
                warnings,
            )?;
            self.directory(
                &root.join("env"),
                ModuleRole::Environment,
                catalog,
                warnings,
            )?;
        }
        Ok(())
    }

    fn directory(
        &self,
        root: &Path,
        role: ModuleRole,
        catalog: &mut ModuleCatalog,
        warnings: &mut Vec<ProjectDiagnostic>,
    ) -> Result<(), ProjectDiagnostic> {
        let mut any_aiken = false;
        let mut has_default = false;
        let mut add = |path: &Path| -> Result<(), ProjectDiagnostic> {
            if path.extension().is_none_or(|extension| extension != "ak") {
                return Ok(());
            }
            any_aiken = true;
            let relative = path
                .strip_prefix(root)
                .map_err(|error| ProjectDiagnostic::new("NAP4104", path, error.to_string()))?;
            let Some(module) = module_name(relative) else {
                warnings.push(ProjectDiagnostic::new(
                    "NAP4105",
                    path,
                    "ignoring source with an invalid Aiken module filename",
                ));
                return Ok(());
            };
            has_default |= module.as_str() == "default";
            let uri = Url::from_file_path(path).map_err(|()| {
                ProjectDiagnostic::new(
                    "NAP4104",
                    path,
                    "source path cannot be represented as a file URI",
                )
            })?;
            if catalog.get(&uri).is_some_and(|existing| {
                existing.key.package == *self.package
                    && existing.key.module == module
                    && existing.resolution_root.as_deref() == Some(self.resolution_root)
            }) {
                return Ok(());
            }
            insert(catalog, uri, self.spec(root, module, role, None))
        };
        if root
            .try_exists()
            .map_err(|error| crate::manifest::io(root, error))?
        {
            for entry in WalkDir::new(root).follow_links(true).sort_by_file_name() {
                let entry = entry.map_err(|error| {
                    ProjectDiagnostic::new(
                        "NAP4104",
                        error.path().unwrap_or(root),
                        error.to_string(),
                    )
                })?;
                if entry.file_type().is_file() {
                    add(entry.path())?;
                }
            }
        }
        for path in self
            .additional_sources
            .iter()
            .filter(|path| path.starts_with(root))
        {
            add(path)?;
        }
        if role == ModuleRole::Environment && any_aiken && !has_default {
            return Err(ProjectDiagnostic::new(
                "NAP4106",
                root,
                "an environment directory containing Aiken sources requires env/default.ak",
            ));
        }
        Ok(())
    }

    pub(crate) fn spec(
        &self,
        root: &Path,
        module: ModuleName,
        role: ModuleRole,
        synthetic_source: Option<String>,
    ) -> SourceSpec {
        let origin = if synthetic_source.is_some() {
            SourceOrigin::Synthetic
        } else if self.dependency {
            SourceOrigin::Dependency
        } else {
            SourceOrigin::Root
        };
        SourceSpec {
            key: ModuleKey {
                package: self.package.clone(),
                module,
            },
            source_root: root.to_path_buf(),
            role: Some(role),
            frontend: Some("aiken".to_string()),
            origin,
            visible_packages: Some(self.visible.to_vec()),
            resolution_root: Some(self.resolution_root.to_path_buf()),
            import_aliases: BTreeMap::from([(
                ModuleName::new("env"),
                ModuleName::new(self.environment.replace('/', ".")),
            )]),
            synthetic_source,
            project: Some(self.metadata.clone()),
        }
    }
}

pub(crate) fn insert(
    catalog: &mut ModuleCatalog,
    uri: Url,
    spec: SourceSpec,
) -> Result<(), ProjectDiagnostic> {
    if let Some((first, _)) = catalog.iter().find(|(_, existing)| {
        existing.key == spec.key && existing.resolution_root == spec.resolution_root
    }) {
        return Err(ProjectDiagnostic::new(
            "NAP4107",
            uri.to_file_path().unwrap_or_default(),
            format!(
                "duplicate module {} in package {:?}; first source is {first}",
                spec.key.module.as_str(),
                spec.key.package.name
            ),
        ));
    }
    if catalog.contains_key(&uri) {
        return Err(ProjectDiagnostic::new(
            "NAP4107",
            uri.to_file_path().unwrap_or_default(),
            "source belongs to multiple packages",
        ));
    }
    catalog.insert(uri, spec);
    Ok(())
}

fn module_name(path: &Path) -> Option<ModuleName> {
    fn module(part: &str) -> bool {
        part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && part.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
    }
    let path = path.to_str()?.replace('\\', "/");
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop()?.strip_suffix(".ak")?;
    if !parts.iter().all(|part| module(part)) {
        return None;
    }
    let mut suffixes = file.split('.');
    if !module(suffixes.next()?)
        || !suffixes.all(|suffix| {
            suffix.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
        })
    {
        return None;
    }
    parts.push(file);
    Some(ModuleName::new(parts.join(".").replace('-', "_")))
}
