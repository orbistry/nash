use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

use aiken_project::{
    config::{Dependency, Platform},
    deps::manifest::{Manifest, Package},
    package_name::PackageName,
    paths,
};
use nash_frontend::{PackageId, PackageSourceId, ProjectDiagnostic};
use serde::{Deserialize, Serialize};

use crate::manifest::{self, io as io_error, read_toml};

/// Nash-owned package access boundary; implementations must not compile packages.
pub trait PackageAccess {
    fn resolve(
        &self,
        request: PackageRequest<'_>,
    ) -> Result<Vec<PackageLocation>, ProjectDiagnostic>;
}

pub struct PackageRequest<'a> {
    pub root: &'a Path,
    pub package: &'a PackageId,
    pub requirements: &'a [PackageId],
    pub warnings: &'a mut Vec<ProjectDiagnostic>,
}

#[derive(Clone, Debug)]
pub struct PackageLocation {
    pub package: PackageId,
    pub root: PathBuf,
}

/// Pinned lock/cache semantics, with downloads only for packages absent from the cache.
pub struct AikenPackageAccess;

impl PackageAccess for AikenPackageAccess {
    fn resolve(
        &self,
        request: PackageRequest<'_>,
    ) -> Result<Vec<PackageLocation>, ProjectDiagnostic> {
        resolve(request, None, true)
    }
}

/// The same lock and materialization path, with network access disabled.
/// `cache` is the directory containing Aiken package zipballs, not its parent.
pub struct PreparedPackages {
    pub cache: PathBuf,
}

impl PackageAccess for PreparedPackages {
    fn resolve(
        &self,
        request: PackageRequest<'_>,
    ) -> Result<Vec<PackageLocation>, ProjectDiagnostic> {
        resolve(request, Some(&self.cache), false)
    }
}

#[derive(Default, Deserialize, Serialize)]
struct LocalPackages {
    packages: Vec<Dependency>,
}

fn resolve(
    request: PackageRequest<'_>,
    cache: Option<&Path>,
    network: bool,
) -> Result<Vec<PackageLocation>, ProjectDiagnostic> {
    let lock_path = request.root.join(paths::manifest());
    let requirements = request
        .requirements
        .iter()
        .map(|id| dependency(id, &lock_path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut manifest = load_manifest(&lock_path, &requirements)?;
    if manifest.packages.is_empty() {
        save_manifest(&lock_path, &manifest)?;
        return Ok(Vec::new());
    }
    let build = request.root.join(paths::build());
    fs::create_dir_all(&build).map_err(|e| io_error(&build, e))?;
    let build_lock_path = build.join("aiken-compile.lock");
    let mut build_lock =
        fslock::LockFile::open(&build_lock_path).map_err(|e| io_error(&build_lock_path, e))?;
    build_lock
        .lock_with_pid()
        .map_err(|e| io_error(&build_lock_path, e))?;
    manifest = load_manifest(&lock_path, &requirements)?;
    let local_path = request.root.join(paths::packages_toml());
    let local: LocalPackages = if local_path
        .try_exists()
        .map_err(|e| io_error(&local_path, e))?
    {
        read_toml(&local_path)?
    } else {
        LocalPackages::default()
    };
    let mut locations = Vec::new();
    for index in 0..manifest.packages.len() {
        let package = manifest.packages[index].clone();
        if request.package.name.as_deref() == Some(package.name.to_string().as_str()) {
            return Err(ProjectDiagnostic::new(
                "NAP4110",
                &lock_path,
                "a package cannot depend on itself",
            ));
        }
        let destination = request.root.join(paths::build_deps_package(&package.name));
        let tracked = local
            .packages
            .iter()
            .find(|entry| entry.name == package.name);
        let ready = tracked.is_some_and(|entry| {
            entry.version == package.version
                && entry.source == package.source
                && paths::is_git_sha_or_tag(&entry.version)
        }) && destination.is_dir();
        if !ready {
            if destination
                .try_exists()
                .map_err(|e| io_error(&destination, e))?
                && tracked.is_none()
            {
                return Err(ProjectDiagnostic::new(
                    "NAP4111",
                    &destination,
                    "package directory is not recorded in build/packages/packages.toml; refusing to overwrite untracked sources",
                ));
            }
            let default_cache;
            let cache = match cache {
                Some(cache) => cache,
                None => {
                    default_cache = dirs::cache_dir()
                        .ok_or_else(|| {
                            ProjectDiagnostic::new(
                                "NAP4111",
                                &destination,
                                "cannot determine the Aiken package cache directory",
                            )
                        })?
                        .join("aiken")
                        .join("packages");
                    &default_cache
                }
            };
            let archive =
                cached_package(&package, &mut manifest, cache, network, request.warnings)?;
            install(&archive, &destination)?;
        }
        if !destination.join("aiken.toml").is_file() {
            return Err(ProjectDiagnostic::new(
                "NAP4111",
                &destination,
                format!("resolved package {} has no aiken.toml", package.name),
            ));
        }
        locations.push(PackageLocation {
            package: PackageId {
                name: Some(package.name.to_string()),
                version: package.version,
                source: manifest::source(package.source),
            },
            root: fs::canonicalize(&destination).map_err(|e| io_error(&destination, e))?,
        });
    }
    save_manifest(&lock_path, &manifest)?;
    let local = LocalPackages {
        packages: manifest
            .packages
            .iter()
            .map(|package| Dependency {
                name: package.name.clone(),
                version: package.version.clone(),
                source: package.source,
            })
            .collect(),
    };
    // Old package directories are deliberately retained: they may contain user edits.
    save_toml(&local_path, &local, "")?;
    Ok(locations)
}

fn load_manifest(path: &Path, requirements: &[Dependency]) -> Result<Manifest, ProjectDiagnostic> {
    let mut manifest = if path.try_exists().map_err(|e| io_error(path, e))? {
        read_toml::<Manifest>(path)?
    } else {
        fresh_manifest(requirements)
    };
    if manifest.requirements != requirements {
        manifest = fresh_manifest(requirements);
    }
    validate_lock(&manifest, requirements, path)?;
    Ok(manifest)
}

fn dependency(id: &PackageId, path: &Path) -> Result<Dependency, ProjectDiagnostic> {
    let name = id.name.as_deref().ok_or_else(|| {
        ProjectDiagnostic::new("NAP4110", path, "Aiken dependencies require a package name")
    })?;
    manifest::validate_name(name, path)?;
    let name: PackageName = name
        .parse()
        .map_err(|error| ProjectDiagnostic::new("NAP4110", path, format!("{error}")))?;
    let source = match id.source {
        PackageSourceId::Github => Platform::Github,
        PackageSourceId::Gitlab => Platform::Gitlab,
        PackageSourceId::Bitbucket => Platform::Bitbucket,
        _ => {
            return Err(ProjectDiagnostic::new(
                "NAP4110",
                path,
                "Aiken lock dependencies require a hosted package source",
            ));
        }
    };
    Ok(Dependency {
        name,
        version: id.version.clone(),
        source,
    })
}

fn fresh_manifest(requirements: &[Dependency]) -> Manifest {
    Manifest {
        requirements: requirements.to_vec(),
        packages: requirements
            .iter()
            .map(|dependency| Package {
                name: dependency.name.clone(),
                version: dependency.version.clone(),
                source: dependency.source,
                requirements: Vec::new(),
            })
            .collect(),
        etags: BTreeMap::new(),
    }
}

fn validate_lock(
    manifest: &Manifest,
    requirements: &[Dependency],
    path: &Path,
) -> Result<(), ProjectDiagnostic> {
    let mut names = BTreeMap::new();
    let mut destinations = BTreeSet::new();
    for package in &manifest.packages {
        manifest::validate_name(&package.name.to_string(), path)?;
        if names.insert(package.name.to_string(), package).is_some() {
            return Err(ProjectDiagnostic::new(
                "NAP4110",
                path,
                format!(
                    "duplicate or conflicting locked versions of {}",
                    package.name
                ),
            ));
        }
        if !destinations.insert(paths::build_deps_package(&package.name)) {
            return Err(ProjectDiagnostic::new(
                "NAP4110",
                path,
                "two locked package names map to the same build directory",
            ));
        }
    }
    for dependency in requirements {
        if !names.contains_key(&dependency.name.to_string()) {
            return Err(ProjectDiagnostic::new(
                "NAP4110",
                path,
                format!("lock does not contain required package {}", dependency.name),
            ));
        }
    }
    Ok(())
}

fn cached_package(
    package: &Package,
    manifest: &mut Manifest,
    cache: &Path,
    network: bool,
    warnings: &mut Vec<ProjectDiagnostic>,
) -> Result<PathBuf, ProjectDiagnostic> {
    let prefix = format!(
        "{}-{}-{}",
        package.name.owner,
        package.name.repo,
        package.version.replace('/', "_")
    );
    let mut client = None;
    let key = if paths::is_git_sha_or_tag(&package.version) {
        prefix
    } else if let Some(etag) = manifest.lookup_etag(package) {
        format!("{prefix}@{etag}")
    } else if network {
        let client = client.get_or_insert_with(reqwest::blocking::Client::new);
        match fetch_etag(client, package) {
            Ok(etag) => {
                manifest.insert_etag(package, etag.clone());
                format!("{prefix}@{etag}")
            }
            Err(error) => {
                let archive = newest_cached(cache, &prefix).map_err(|fallback| {
                    ProjectDiagnostic::new(
                        "NAP4111",
                        cache,
                        format!("{error}; cache fallback failed: {}", fallback.message),
                    )
                })?;
                warnings.push(ProjectDiagnostic::new(
                    "NAP4112",
                    &archive,
                    format!(
                        "cannot refresh {} at {}: {error}; using the most recent cached archive",
                        package.name, package.version
                    ),
                ));
                return Ok(archive);
            }
        }
    } else {
        return newest_cached(cache, &prefix);
    };
    if Path::new(&key).components().count() != 1 || key.contains('\\') {
        return Err(ProjectDiagnostic::new(
            "NAP4110",
            cache,
            "invalid package cache key",
        ));
    }
    let archive = cache.join(format!("{key}.zip"));
    if archive.is_file() {
        return Ok(archive);
    }
    if !network {
        return Err(ProjectDiagnostic::new(
            "NAP4111",
            &archive,
            format!(
                "package {} at {} is absent from the prepared cache (network disabled)",
                package.name, package.version
            ),
        ));
    }
    let client = client.get_or_insert_with(reqwest::blocking::Client::new);
    fs::create_dir_all(cache).map_err(|e| io_error(cache, e))?;
    let mut response = client
        .get(package_url(package))
        .header("User-Agent", "aiken-lang")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| {
            ProjectDiagnostic::new(
                "NAP4111",
                &archive,
                format!(
                    "cannot download {} at {}: {error}",
                    package.name, package.version
                ),
            )
        })?;
    let mut temporary = tempfile::NamedTempFile::new_in(cache).map_err(|e| io_error(cache, e))?;
    io::copy(&mut response, &mut temporary).map_err(|e| io_error(&archive, e))?;
    temporary
        .persist(&archive)
        .map_err(|e| io_error(&archive, e.error))?;
    Ok(archive)
}

fn package_url(package: &Package) -> String {
    format!(
        "https://api.github.com/repos/{}/{}/zipball/{}",
        package.name.owner, package.name.repo, package.version
    )
}

fn fetch_etag(client: &reqwest::blocking::Client, package: &Package) -> Result<String, String> {
    let response = client
        .head(package_url(package))
        .header("User-Agent", "aiken-lang")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    response
        .headers()
        .get("etag")
        .ok_or_else(|| format!("no ETag returned for {}", package.name))?
        .to_str()
        .map(|etag| etag.replace('"', ""))
        .map_err(|e| e.to_string())
}

fn newest_cached(cache: &Path, prefix: &str) -> Result<PathBuf, ProjectDiagnostic> {
    let mut newest = None;
    for entry in fs::read_dir(cache).map_err(|e| io_error(cache, e))? {
        let entry = entry.map_err(|e| io_error(cache, e))?;
        let filename = entry.file_name();
        let Some(filename) = filename.to_str() else {
            continue;
        };
        if !filename.starts_with(prefix)
            || !filename.ends_with(".zip")
            || !entry
                .file_type()
                .map_err(|e| io_error(&entry.path(), e))?
                .is_file()
        {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|e| io_error(&entry.path(), e))?;
        if newest.as_ref().is_none_or(|(date, _)| modified > *date) {
            newest = Some((modified, entry.path()));
        }
    }
    newest.map(|(_, path)| path).ok_or_else(|| {
        ProjectDiagnostic::new(
            "NAP4111",
            cache,
            format!("no cached archive matches {prefix}"),
        )
    })
}

fn install(archive: &Path, destination: &Path) -> Result<(), ProjectDiagnostic> {
    let parent = destination.parent().ok_or_else(|| {
        ProjectDiagnostic::new("NAP4111", destination, "package directory has no parent")
    })?;
    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    let staging = tempfile::Builder::new()
        .prefix(".nash-unpack-")
        .tempdir_in(parent)
        .map_err(|e| io_error(parent, e))?;
    let file = fs::File::open(archive).map_err(|e| io_error(archive, e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| ProjectDiagnostic::new("NAP4111", archive, e.to_string()))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| ProjectDiagnostic::new("NAP4111", archive, e.to_string()))?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            ProjectDiagnostic::new("NAP4111", archive, "archive contains an unsafe path")
        })?;
        let relative: PathBuf = enclosed.components().skip(1).collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let path = staging.path().join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|e| io_error(&path, e))?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
            }
            let mut output = fs::File::create(&path).map_err(|e| io_error(&path, e))?;
            io::copy(&mut entry, &mut output).map_err(|e| io_error(&path, e))?;
        }
    }
    if !staging.path().join("aiken.toml").is_file() {
        return Err(ProjectDiagnostic::new(
            "NAP4111",
            archive,
            "package archive contains no aiken.toml after removing its archive root",
        ));
    }
    let preserved = if destination
        .try_exists()
        .map_err(|e| io_error(destination, e))?
    {
        let backup = tempfile::Builder::new()
            .prefix(".nash-preserved-")
            .tempdir_in(parent)
            .map_err(|e| io_error(parent, e))?
            .keep();
        let old = backup.join("package");
        fs::rename(destination, &old).map_err(|e| io_error(destination, e))?;
        Some(old)
    } else {
        None
    };
    if let Err(error) = fs::rename(staging.path(), destination) {
        if let Some(old) = &preserved {
            fs::rename(old, destination).map_err(|restore| ProjectDiagnostic::new("NAP4111", destination, format!("install failed: {error}; restoring old sources from {} also failed: {restore}", old.display())))?;
        }
        return Err(io_error(destination, error));
    }
    Ok(())
}

fn save_manifest(path: &Path, manifest: &Manifest) -> Result<(), ProjectDiagnostic> {
    save_toml(
        path,
        manifest,
        "# This file was generated by Aiken\n# You typically do not need to edit this file\n\n",
    )
}

fn save_toml(path: &Path, value: &impl Serialize, prefix: &str) -> Result<(), ProjectDiagnostic> {
    let source = toml::to_string(value)
        .map_err(|e| ProjectDiagnostic::new("NAP4110", path, e.to_string()))?;
    let source = format!("{prefix}{source}");
    if fs::read_to_string(path).is_ok_and(|existing| existing == source) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| ProjectDiagnostic::new("NAP4110", path, "metadata path has no parent"))?;
    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| io_error(parent, e))?;
    io::Write::write_all(&mut temporary, source.as_bytes()).map_err(|e| io_error(path, e))?;
    temporary
        .persist(path)
        .map_err(|e| io_error(path, e.error))?;
    Ok(())
}
