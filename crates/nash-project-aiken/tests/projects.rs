use std::{collections::BTreeMap, fs, io::Write, path::Path};

use aiken_lang::ast::{Annotation, ModuleKind, UntypedDefinition};
use nash_frontend::{
    LoadedProject, ModuleName, ModuleRole, ProjectDiagnostic, ProjectLoadRequest, ProjectMode,
    SourceOrigin,
};
use nash_project_aiken::{PreparedPackages, load, load_with_package_access, load_with_sources};

fn put(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn manifest(name: &str) -> String {
    format!("name = {name:?}\nversion = \"0.0.0\"\ncompiler = \"v1.1.23\"\nplutus = \"v3\"\n")
}

fn request<'a>(location: &'a Path, environment: Option<&'a str>) -> ProjectLoadRequest<'a> {
    ProjectLoadRequest {
        location,
        environment,
        mode: ProjectMode::Check,
    }
}

fn offline(
    root: &Path,
    environment: Option<&str>,
) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    load_with_package_access(
        request(root, environment),
        &PreparedPackages {
            cache: root.join("cache"),
        },
    )
}

fn root_project(root: &Path) {
    put(root, "aiken.toml", &manifest("test/root"));
    put(
        root,
        "lib/main.ak",
        "use env\npub fn selected() { env.value }\n",
    );
}

#[test]
fn environments_select_aliases_without_discarding_other_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    root_project(root);
    put(root, "env/default.ak", "pub const value = 1\n");
    put(root, "env/preview.ak", "pub const value = 2\n");
    let default = offline(root, None).unwrap();
    let named = offline(root, Some("preview")).unwrap();
    let main = |project: &LoadedProject| {
        project
            .catalog
            .values()
            .find(|source| source.key.module.as_str() == "main")
            .unwrap()
            .import_aliases[&ModuleName::new("env")]
            .clone()
    };
    assert_eq!(main(&default), ModuleName::new("default"));
    assert_eq!(main(&named), ModuleName::new("preview"));
    for project in [&default, &named] {
        let environments: Vec<_> = project
            .catalog
            .values()
            .filter(|source| source.role == Some(ModuleRole::Environment))
            .map(|source| source.key.module.as_str())
            .collect();
        assert_eq!(environments, ["default", "preview"]);
    }
    // Selecting an unknown environment is only an error when a source imports it.
    let missing = offline(root, Some("missing")).unwrap();
    assert_eq!(main(&missing), ModuleName::new("missing"));
}

#[test]
fn default_is_required_only_after_an_aiken_file_is_discovered() {
    let temp = tempfile::tempdir().unwrap();
    root_project(temp.path());
    fs::create_dir(temp.path().join("env")).unwrap();
    assert!(
        offline(temp.path(), None)
            .unwrap()
            .catalog
            .values()
            .all(|source| source.role != Some(ModuleRole::Environment))
    );
    put(temp.path(), "env/NotValid.ak", "pub const ignored = True\n");
    let errors = offline(temp.path(), None).unwrap_err();
    assert!(errors.iter().any(|error| error.code == "NAP4106"));
    put(temp.path(), "env/default.ak", "pub const value = 1\n");
    let loaded = offline(temp.path(), None).unwrap();
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == "NAP4105")
    );
}

fn annotation(annotation: &Annotation) -> String {
    match annotation {
        Annotation::Constructor {
            name, arguments, ..
        } if arguments.is_empty() => name.clone(),
        Annotation::Constructor {
            name, arguments, ..
        } => format!(
            "{name}<{}>",
            arguments
                .iter()
                .map(self::annotation)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Annotation::Tuple { elems, .. } => format!(
            "({})",
            elems
                .iter()
                .map(self::annotation)
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => panic!("unexpected config type {other:?}"),
    }
}

#[test]
fn generated_config_preserves_exact_simple_expression_types() {
    let temp = tempfile::tempdir().unwrap();
    let source = format!(
        "{}\n[config.default]\nflag = true\nnegative = -42\nutf8 = \"λ\"\nhex = {{ bytes = \"00ff\", encoding = \"base16\" }}\nlist = [1, 2]\ntuple = [1, false, \"x\"]\nempty = []\nnested = [[1, true], [2, false]]\n",
        manifest("test/root")
    );
    put(temp.path(), "aiken.toml", &source);
    let loaded = offline(temp.path(), None).unwrap();
    let config = loaded
        .catalog
        .values()
        .find(|source| source.key.module.as_str() == "config")
        .unwrap();
    assert_eq!(config.origin, SourceOrigin::Synthetic);
    let (parsed, _) = aiken_lang::parser::module(
        config.synthetic_source.as_ref().unwrap(),
        ModuleKind::Config,
    )
    .unwrap();
    let types: BTreeMap<_, _> = parsed
        .definitions
        .iter()
        .map(|definition| match definition {
            UntypedDefinition::ModuleConstant(constant) => (
                constant.name.as_str(),
                annotation(constant.annotation.as_ref().unwrap()),
            ),
            _ => panic!("config should export constants"),
        })
        .collect();
    assert_eq!(
        types,
        BTreeMap::from([
            ("empty", "List<Data>".into()),
            ("flag", "Bool".into()),
            ("hex", "ByteArray".into()),
            ("list", "List<Int>".into()),
            ("negative", "Int".into()),
            ("nested", "List<(Int,Bool)>".into()),
            ("tuple", "(Int,Bool,ByteArray)".into()),
            ("utf8", "ByteArray".into()),
        ])
    );
    let missing = offline(temp.path(), Some("preview")).unwrap();
    assert!(
        missing
            .catalog
            .values()
            .all(|source| source.key.module.as_str() != "config")
    );
    assert!(
        missing
            .warnings
            .iter()
            .any(|warning| warning.code == "NAP4108")
    );
}

fn dependency(name: &str) -> String {
    format!("name = {name:?}\nversion = \"v1.0.0\"\nsource = \"github\"\n")
}

fn locked(root: &Path, direct: &[&str], packages: &[&str]) {
    let mut project = manifest("test/root");
    for name in direct {
        project.push_str(&format!("\n[[dependencies]]\n{}", dependency(name)));
    }
    put(root, "aiken.toml", &project);
    let mut lock = String::new();
    for name in direct {
        lock.push_str(&format!("[[requirements]]\n{}\n", dependency(name)));
    }
    if direct.is_empty() {
        lock.push_str("requirements = []\n");
    }
    for name in packages {
        lock.push_str(&format!(
            "[[packages]]\n{}requirements = []\n\n",
            dependency(name)
        ));
    }
    if packages.is_empty() {
        lock.push_str("packages = []\n");
    }
    put(root, "aiken.lock", &lock);
}

fn prepared(root: &Path, names: &[&str]) {
    let mut local = String::new();
    for name in names {
        local.push_str(&format!("[[packages]]\n{}\n", dependency(name)));
        let directory = format!("build/packages/{}", name.replace('/', "-"));
        put(root, &format!("{directory}/aiken.toml"), &manifest(name));
    }
    put(root, "build/packages/packages.toml", &local);
}

#[test]
fn direct_and_transitive_locked_sources_remain_visible_without_recursive_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    locked(root, &["test/alpha"], &["test/alpha", "test/beta"]);
    prepared(root, &["test/alpha", "test/beta"]);
    put(
        root,
        "lib/main.ak",
        "use alpha\npub fn result() { alpha.answer() }\n",
    );
    put(
        root,
        "build/packages/test-alpha/lib/alpha.ak",
        "use beta\npub fn answer() { beta.value }\n",
    );
    put(
        root,
        "build/packages/test-beta/lib/beta.ak",
        "pub const value = 42\n",
    );
    put(
        root,
        "build/packages/test-alpha/validators/ignored.ak",
        "not parsed as a dependency source",
    );
    put(
        root,
        "build/packages/test-alpha/env/default.ak",
        "not parsed as a dependency source",
    );
    let loaded = load(request(root, None)).unwrap();
    let package_names: Vec<_> = loaded
        .packages
        .iter()
        .map(|package| package.id.name.as_deref().unwrap())
        .collect();
    assert_eq!(package_names, ["test/root", "test/alpha", "test/beta"]);
    let alpha = loaded
        .catalog
        .values()
        .find(|source| source.key.module.as_str() == "alpha")
        .unwrap();
    assert_eq!(alpha.origin, SourceOrigin::Dependency);
    assert!(
        alpha
            .visible_packages
            .as_ref()
            .unwrap()
            .iter()
            .any(|id| id.name.as_deref() == Some("test/beta"))
    );
    assert!(
        loaded
            .catalog
            .values()
            .all(|source| !matches!(source.key.module.as_str(), "ignored" | "default"))
    );
    let persisted: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("aiken.lock")).unwrap()).unwrap();
    assert_eq!(persisted["packages"].as_array().unwrap().len(), 2);
}

#[test]
fn prepared_zip_cache_materializes_packages_without_network_or_deleting_unrelated_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    locked(root, &["test/alpha"], &["test/alpha"]);
    put(root, "build/packages/unrelated/lib/keep.ak", "user source");
    fs::create_dir_all(root.join("cache")).unwrap();
    let file = fs::File::create(root.join("cache/test-alpha-v1.0.0.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive
        .start_file("github-root/aiken.toml", options)
        .unwrap();
    archive
        .write_all(manifest("test/alpha").as_bytes())
        .unwrap();
    archive
        .start_file("github-root/lib/alpha.ak", options)
        .unwrap();
    archive.write_all(b"pub const answer = 42\n").unwrap();
    archive.finish().unwrap();
    let loaded = offline(root, None).unwrap();
    assert!(
        loaded
            .catalog
            .values()
            .any(|source| source.key.module.as_str() == "alpha")
    );
    assert_eq!(
        fs::read_to_string(root.join("build/packages/unrelated/lib/keep.ak")).unwrap(),
        "user source"
    );
    // On the next load the build/packages record is authoritative, even without a zipball.
    fs::remove_file(root.join("cache/test-alpha-v1.0.0.zip")).unwrap();
    let next = offline(root, None).unwrap();
    assert_eq!(
        next.catalog.keys().collect::<Vec<_>>(),
        loaded.catalog.keys().collect::<Vec<_>>()
    );
}

#[test]
fn missing_packages_and_same_package_duplicates_are_located_errors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    locked(root, &["test/alpha"], &["test/alpha"]);
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4111");
    assert!(error.path.ends_with("cache/test-alpha-v1.0.0.zip"));
    prepared(root, &["test/alpha"]);
    put(root, "lib/shared.ak", "pub const answer = 1\n");
    put(
        root,
        "build/packages/test-alpha/lib/shared.ak",
        "pub const answer = 2\n",
    );
    let loaded = offline(root, None).unwrap();
    assert_eq!(
        loaded
            .catalog
            .values()
            .filter(|source| source.key.module.as_str() == "shared")
            .count(),
        2
    );
    // Cross-package ambiguity belongs to the import diagnostic in the driver.
    put(
        root,
        "validators/shared.ak",
        "validator shared { else(_) { True } }\n",
    );
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4107");
    assert!(error.path.ends_with("shared.ak"));
}

#[test]
fn workspaces_check_members_in_separate_visibility_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(root, "aiken.toml", "members = [\"members/*\"]\n");
    for member in ["alpha", "beta"] {
        let member_root = root.join("members").join(member);
        put(
            &member_root,
            "aiken.toml",
            &manifest(&format!("test/{member}")),
        );
        put(&member_root, "lib/shared.ak", "pub const answer = 1\n");
    }
    let loaded = offline(root, None).unwrap();
    for source in loaded.catalog.values() {
        assert_eq!(
            source.visible_packages.as_ref().unwrap(),
            std::slice::from_ref(&source.key.package)
        );
        assert_eq!(
            source.resolution_root.as_ref().unwrap(),
            &source.source_root.parent().unwrap().to_path_buf()
        );
    }
    put(
        root,
        "aiken.toml",
        "members = [\"members/alpha\", \"members/alpha\"]\n",
    );
    assert_eq!(offline(root, None).unwrap_err()[0].code, "NAP4102");
}

#[test]
fn explicit_workspace_dependencies_use_locked_materialized_packages() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(
        root,
        "aiken.toml",
        "members = [\"consumer\", \"library\"]\n",
    );
    let consumer = root.join("consumer");
    locked(&consumer, &["test/library"], &["test/library"]);
    prepared(&consumer, &["test/library"]);
    put(
        &consumer,
        "lib/main.ak",
        "use library\npub fn result() { library.answer }\n",
    );
    put(
        &consumer,
        "build/packages/test-library/lib/library.ak",
        "pub const answer = 42\n",
    );
    put(root, "library/aiken.toml", &manifest("test/library"));
    put(root, "library/lib/library.ak", "pub const answer = 42\n");
    let loaded = offline(root, None).unwrap();
    let main = loaded
        .catalog
        .values()
        .find(|source| source.key.module.as_str() == "main")
        .unwrap();
    let libraries: Vec<_> = loaded
        .catalog
        .values()
        .filter(|source| source.key.module.as_str() == "library")
        .collect();
    assert_eq!(libraries.len(), 2);
    assert_eq!(
        libraries
            .iter()
            .filter(|library| library.resolution_root == main.resolution_root
                && main
                    .visible_packages
                    .as_ref()
                    .unwrap()
                    .contains(&library.key.package))
            .count(),
        1
    );
}

#[test]
fn metadata_validation_and_manifest_selection_preserve_pinned_rules() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = "name = \"test/root\"\nversion = \"custom-tag\"\ncompiler = \"v1.1.21\"\nplutus = \"v3\"\nlicense = \"Apache-2.0\"\ndescription = \"metadata\"\n[repository]\nuser = \"test\"\nproject = \"root\"\nplatform = \"github\"\n";
    put(root, "aiken.toml", source);
    let loaded = offline(root, None).unwrap();
    assert_eq!(loaded.packages[0].metadata.version, "custom-tag");
    assert_eq!(
        loaded.packages[0].metadata.license.as_deref(),
        Some("Apache-2.0")
    );
    assert_eq!(
        loaded.packages[0]
            .metadata
            .repository
            .as_ref()
            .unwrap()
            .project,
        "root"
    );
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == "NAP4109")
    );
    put(root, "nash.jsonc", "{}");
    assert_eq!(offline(root, None).unwrap_err()[0].code, "NAP4101");
    assert_eq!(
        load(request(&root.join("aiken.toml"), None))
            .unwrap()
            .packages[0]
            .id
            .name
            .as_deref(),
        Some("test/root")
    );
    put(root, "aiken.toml", &source.replace("\"v3\"", "\"v2\""));
    let errors = load(request(&root.join("aiken.toml"), None)).unwrap_err();
    assert_eq!(errors[0].code, "NAP4103");
    assert!(errors[0].region.is_some());
}

#[test]
fn resolved_lock_versions_are_authoritative_and_conflicts_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    locked(root, &["test/alpha"], &["test/alpha"]);
    prepared(root, &["test/alpha"]);
    let resolved = dependency("test/alpha").replace("v1.0.0", "v1.1.0");
    let lock = format!(
        "[[requirements]]\n{}\n[[packages]]\n{resolved}requirements = []\n",
        dependency("test/alpha")
    );
    put(root, "aiken.lock", &lock);
    put(
        root,
        "build/packages/packages.toml",
        &format!("[[packages]]\n{resolved}"),
    );
    let loaded = offline(root, None).unwrap();
    assert_eq!(loaded.packages[0].dependencies[0].version, "v1.1.0");
    assert_eq!(loaded.packages[1].id.version, "v1.1.0");
    put(
        root,
        "aiken.lock",
        &format!(
            "{lock}\n[[packages]]\n{}requirements = []\n",
            dependency("test/alpha")
        ),
    );
    assert_eq!(offline(root, None).unwrap_err()[0].code, "NAP4110");
}

#[test]
fn the_same_locked_package_keeps_independent_workspace_resolution_contexts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(root, "aiken.toml", "members = [\"one\", \"two\"]\n");
    for member in ["one", "two"] {
        let member_root = root.join(member);
        locked(&member_root, &["test/alpha"], &["test/alpha"]);
        let source = fs::read_to_string(member_root.join("aiken.toml"))
            .unwrap()
            .replace("test/root", &format!("test/{member}"));
        put(&member_root, "aiken.toml", &source);
        prepared(&member_root, &["test/alpha"]);
        put(
            &member_root,
            "build/packages/test-alpha/lib/alpha.ak",
            "use env\npub fn answer() { env.value }\n",
        );
        put(&member_root, "env/default.ak", "pub const value = 42\n");
    }
    let loaded = offline(root, None).unwrap();
    let alpha: Vec<_> = loaded
        .catalog
        .values()
        .filter(|source| source.key.module.as_str() == "alpha")
        .collect();
    assert_eq!(alpha.len(), 2);
    assert_eq!(alpha[0].key, alpha[1].key);
    assert_ne!(alpha[0].resolution_root, alpha[1].resolution_root);
    assert_ne!(alpha[0].visible_packages, alpha[1].visible_packages);
}

#[test]
fn unsaved_sources_use_normal_discovery_even_when_source_directories_do_not_exist() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    put(&root, "aiken.toml", &manifest("test/root"));
    let paths = [
        "lib/new-module.ak",
        "validators/spending.ak",
        "env/default.ak",
        "env/preview.ak",
    ];
    let uris: Vec<_> = paths
        .iter()
        .map(|path| url::Url::from_file_path(root.join(path)).unwrap())
        .collect();
    let loaded = load_with_sources(request(&root, Some("preview")), &uris).unwrap();
    let roles: BTreeMap<_, _> = loaded
        .catalog
        .values()
        .map(|source| (source.key.module.as_str(), source.role.unwrap()))
        .collect();
    assert_eq!(
        roles,
        BTreeMap::from([
            ("default", ModuleRole::Environment),
            ("new_module", ModuleRole::Library),
            ("preview", ModuleRole::Environment),
            ("spending", ModuleRole::Validator),
        ])
    );
    assert!(
        loaded
            .catalog
            .values()
            .all(|source| source.import_aliases[&ModuleName::new("env")]
                == ModuleName::new("preview"))
    );
    assert!(paths.iter().all(|path| !root.join(path).exists()));
    let missing_default = load_with_sources(
        request(&root, None),
        &uris[..2]
            .iter()
            .chain(&uris[3..])
            .cloned()
            .collect::<Vec<_>>(),
    )
    .unwrap_err();
    assert_eq!(missing_default[0].code, "NAP4106");
    put(&root, "env/default.ak", "pub const value = 42\n");
    let deduplicated = load_with_sources(request(&root, None), &uris).unwrap();
    assert_eq!(
        deduplicated.catalog.keys().collect::<Vec<_>>(),
        loaded.catalog.keys().collect::<Vec<_>>()
    );
}

#[test]
fn workspace_member_errors_identify_missing_conflicting_and_nested_packages() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    put(root, "aiken.toml", "members = [\"missing/*\"]\n");
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4102");
    assert!(error.path.ends_with("missing/*"));

    put(root, "aiken.toml", "members = [\"alpha\", \"beta\"]\n");
    for member in ["alpha", "beta"] {
        put(
            &root.join(member),
            "aiken.toml",
            &manifest("test/duplicate"),
        );
    }
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4102");
    assert!(error.path.ends_with("beta"));

    put(
        root,
        "aiken.toml",
        "members = [\"alpha\", \"alpha/nested\"]\n",
    );
    put(root, "alpha/nested/aiken.toml", &manifest("test/nested"));
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4102");
    assert!(error.path.ends_with("alpha/nested"));

    put(root, "aiken.toml", "members = [\"beta\"]\n");
    put(root, "beta/aiken.toml", "members = []\n");
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4102");
    assert!(error.path.ends_with("beta"));
}

#[test]
fn changed_requirements_refresh_the_lock_but_malformed_locks_are_not_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    locked(root, &["test/alpha"], &["test/alpha"]);
    prepared(root, &["test/beta"]);
    put(
        root,
        "build/packages/test-beta/lib/beta.ak",
        "pub const value = 42\n",
    );
    put(
        root,
        "aiken.toml",
        &format!(
            "{}\n[[dependencies]]\n{}",
            manifest("test/root"),
            dependency("test/beta")
        ),
    );
    let loaded = offline(root, None).unwrap();
    assert!(
        loaded
            .packages
            .iter()
            .all(|package| package.id.name.as_deref() != Some("test/alpha"))
    );
    assert!(
        loaded
            .catalog
            .values()
            .any(|source| source.key.module.as_str() == "beta")
    );
    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("aiken.lock")).unwrap()).unwrap();
    assert_eq!(lock["requirements"][0]["name"].as_str(), Some("test/beta"));
    assert_eq!(lock["packages"][0]["name"].as_str(), Some("test/beta"));

    put(root, "aiken.lock", "requirements = [\n");
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4103");
    assert!(error.path.ends_with("aiken.lock"));
    assert!(error.region.is_some());
    assert_eq!(
        fs::read_to_string(root.join("aiken.lock")).unwrap(),
        "requirements = [\n"
    );
}

#[test]
fn untracked_dependency_sources_are_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    locked(root, &["test/alpha"], &["test/alpha"]);
    put(
        root,
        "build/packages/test-alpha/lib/alpha.ak",
        "pub const work_in_progress = 42\n",
    );
    let error = offline(root, None).unwrap_err().remove(0);
    assert_eq!(error.code, "NAP4111");
    assert!(error.path.ends_with("build/packages/test-alpha"));
    assert_eq!(
        fs::read_to_string(root.join("build/packages/test-alpha/lib/alpha.ak")).unwrap(),
        "pub const work_in_progress = 42\n"
    );
}
