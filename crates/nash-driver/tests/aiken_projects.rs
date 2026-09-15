use nash_codegen::build::TraceConfig;
use nash_driver::build::{ValidatorOutput, build_validators, write_outputs};
use nash_driver::{
    Database, FileSystemSource, Project, ProjectMode, build, build_graph, build_with,
};
use nash_plutus::{arena::Arena, data::PlutusData, syn, term::Term};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aiken")
}

fn copy_source(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if matches!(name.to_str(), Some("build" | "artifacts" | "plutus.json")) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().unwrap().is_dir() {
            copy_source(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn prepare(name: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    copy_source(&fixtures().join(name), directory.path());
    nash_project_aiken::load_with_package_access(
        nash_frontend::ProjectLoadRequest {
            location: directory.path(),
            environment: None,
            mode: ProjectMode::Check,
        },
        &nash_project_aiken::PreparedPackages {
            cache: fixtures().join("packages"),
        },
    )
    .unwrap();
    directory
}

async fn outputs(path: &Path, environment: Option<&str>) -> Vec<ValidatorOutput> {
    let project = Project::load_with_options(path, environment, ProjectMode::Build)
        .await
        .unwrap();
    let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
    let catalog = project.discover_modules(&*db.lock().await).await.unwrap();
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let (report, artifacts) = build_with(db, &graph, &catalog, |solved| {
        build_validators(
            solved,
            TraceConfig {
                compiler: false,
                ..TraceConfig::default()
            },
        )
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    artifacts.unwrap().unwrap()
}

fn succeeds<'a>(
    arena: &'a Arena,
    output: &ValidatorOutput,
    arguments: &[&'a PlutusData<'a>],
) -> bool {
    let mut program = syn::parse_program(arena, &output.uplc)
        .into_result()
        .unwrap();
    for argument in arguments {
        program = program.apply(arena, Term::data(arena, argument));
    }
    let evaluation = program.eval(arena);
    if let Ok(term) = evaluation.term {
        assert_eq!(term, Term::unit(arena));
        true
    } else {
        false
    }
}

#[tokio::test]
async fn official_standard_library_checks_unchanged_through_locked_local_packages() {
    let directory = prepare("stdlib-project");
    let project = Project::load(directory.path().join("aiken.toml"))
        .await
        .unwrap();
    let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
    let catalog = project.discover_modules(&*db.lock().await).await.unwrap();
    assert!(
        catalog
            .values()
            .any(
                |source| source.key.package.name.as_deref() == Some("aiken-lang/stdlib")
                    && source.key.package.version == "v3.1.0"
            )
    );
    let graph = build_graph(db.clone(), &catalog).await.unwrap();
    let report = build(db, &graph, &catalog).await;
    assert!(
        report.is_success(),
        "{:#?}",
        report
            .ordered_reports()
            .iter()
            .flat_map(|module| module
                .reports
                .iter()
                .filter(|diagnostic| diagnostic.severity == nash_report::Severity::Error)
                .map(|diagnostic| (
                    &module.name,
                    diagnostic.code,
                    diagnostic.region,
                    &diagnostic.before
                )))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn locked_transitive_layout_is_preserved_and_executed() {
    let directory = prepare("dependency-project");
    let artifacts = outputs(directory.path(), None).await;
    let [output] = artifacts.as_slice() else {
        panic!("expected one boxed validator")
    };
    let metadata = output.metadata.as_ref().unwrap();
    let (identity, layout) = metadata
        .layouts
        .iter()
        .find(|(identity, _)| identity.name == "Box")
        .unwrap();
    assert_eq!(
        identity.module.package.name.as_deref(),
        Some("sample/transitive")
    );
    assert_eq!(identity.module.package.version, "v1.0.0");
    assert_eq!(layout.constructors[0].tag, 7);
    let arena = Arena::new();
    let boxed = |number| {
        PlutusData::constr(
            &arena,
            7,
            arena.alloc_slice_copy(&[PlutusData::integer_from(&arena, number)]),
        )
    };
    let context = |redeemer| {
        PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::integer_from(&arena, 0),
                redeemer,
                PlutusData::constr(
                    &arena,
                    0,
                    arena.alloc_slice_copy(&[PlutusData::byte_string(&arena, b"policy")]),
                ),
            ]),
        )
    };
    assert!(succeeds(&arena, output, &[context(boxed(7))]));
    assert!(!succeeds(&arena, output, &[context(boxed(8))]));
    assert!(!succeeds(
        &arena,
        output,
        &[context(PlutusData::integer_from(&arena, 7))]
    ));
}

#[tokio::test]
async fn selected_environment_and_config_execute_together() {
    let directory = prepare("env-config-project");
    for environment in [None, Some("preview")] {
        let artifacts = outputs(directory.path(), environment).await;
        let [output] = artifacts.as_slice() else {
            panic!("expected selected-environment entry")
        };
        let arena = Arena::new();
        assert!(succeeds(
            &arena,
            output,
            &[PlutusData::integer_from(&arena, 0)]
        ));
    }
}

#[tokio::test]
async fn named_entries_keep_metadata_and_exclude_checked_tools() {
    let directory = prepare("multi-validator-project");
    let artifacts = outputs(directory.path(), None).await;
    let names = artifacts
        .iter()
        .map(|output| output.metadata.as_ref().unwrap().id.name.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(names, BTreeSet::from(["alpha", "beta"]));
    let alpha = artifacts
        .iter()
        .find(|output| output.metadata.as_ref().unwrap().id.name == "alpha")
        .unwrap();
    let beta = artifacts
        .iter()
        .find(|output| output.metadata.as_ref().unwrap().id.name == "beta")
        .unwrap();
    assert_ne!(alpha.output_name, beta.output_name);
    let metadata = alpha.metadata.as_ref().unwrap();
    assert_eq!(metadata.parameters[0].name, "minimum");
    assert_eq!(metadata.parameters[0].label, "threshold");
    assert_eq!(metadata.parameters[0].position, 0);
    assert_eq!(
        metadata.docs.as_deref(),
        Some(" Parameterized minting entry.")
    );
    assert_eq!(
        alpha.project.as_ref().unwrap().license.as_deref(),
        Some("Apache-2.0")
    );
    let arena = Arena::new();
    let context = |redeemer| {
        PlutusData::constr(
            &arena,
            0,
            arena.alloc_slice_copy(&[
                PlutusData::integer_from(&arena, 0),
                PlutusData::integer_from(&arena, redeemer),
                PlutusData::constr(
                    &arena,
                    0,
                    arena.alloc_slice_copy(&[PlutusData::byte_string(&arena, b"policy")]),
                ),
            ]),
        )
    };
    let threshold = PlutusData::integer_from(&arena, 10);
    assert!(succeeds(&arena, alpha, &[threshold, context(12)]));
    assert!(!succeeds(&arena, alpha, &[threshold, context(9)]));
    assert!(succeeds(
        &arena,
        beta,
        &[PlutusData::integer_from(&arena, 17)]
    ));
    assert!(!succeeds(
        &arena,
        beta,
        &[PlutusData::integer_from(&arena, 18)]
    ));
    let output_dir = directory.path().join("output");
    write_outputs(&output_dir, &artifacts).await.unwrap();
    for output in &artifacts {
        assert_eq!(
            std::fs::read(output_dir.join(format!("{}.flat", output.output_name))).unwrap(),
            output.flat
        );
    }
    let preview = outputs(directory.path(), Some("preview")).await;
    let preview_beta = preview
        .iter()
        .find(|output| output.metadata.as_ref().unwrap().id.name == "beta")
        .unwrap();
    assert_eq!(preview_beta.output_name, beta.output_name);
    assert!(succeeds(
        &arena,
        preview_beta,
        &[PlutusData::integer_from(&arena, 18)]
    ));
    assert!(!succeeds(
        &arena,
        preview_beta,
        &[PlutusData::integer_from(&arena, 17)]
    ));
}

#[tokio::test]
async fn workspace_members_check_through_explicit_locked_packages() {
    let dependency = prepare("dependency-project");
    let workspace = tempfile::tempdir().unwrap();
    copy_source(dependency.path(), &workspace.path().join("consumer"));
    copy_source(
        &dependency.path().join("build/packages/sample-direct"),
        &workspace.path().join("producer"),
    );
    std::fs::write(
        workspace.path().join("aiken.toml"),
        "members = [\"consumer\", \"producer\"]\n",
    )
    .unwrap();
    nash_project_aiken::load_with_package_access(
        nash_frontend::ProjectLoadRequest {
            location: workspace.path(),
            environment: None,
            mode: ProjectMode::Build,
        },
        &nash_project_aiken::PreparedPackages {
            cache: fixtures().join("packages"),
        },
    )
    .unwrap();
    let artifacts = outputs(workspace.path(), None).await;
    let names = artifacts
        .iter()
        .map(|output| {
            let id = &output.metadata.as_ref().unwrap().id;
            (id.module.package.name.as_deref(), id.name.as_str())
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from([(Some("acceptance/dependencies"), "boxed_value")])
    );
}

#[tokio::test]
async fn complete_language_fixture_checks_without_running_tools() {
    let directory = prepare("full-language");
    assert!(outputs(directory.path(), None).await.is_empty());
}
