use nash_driver::{Database, InMemorySource, build_graph, build_with};
use nash_plutus::{arena::Arena, flat, syn};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use url::Url;

async fn outputs(files: &[(&str, &str)]) -> Vec<nash_driver::build::ValidatorOutput> {
    outputs_with(files, |_| nash_config::Build::default()).await
}

async fn outputs_with(
    files: &[(&str, &str)],
    config_for: impl FnMut(&Url) -> nash_config::Build + Send + 'static,
) -> Vec<nash_driver::build::ValidatorOutput> {
    outputs_result_with(files, config_for).await.unwrap()
}

async fn outputs_result_with(
    files: &[(&str, &str)],
    config_for: impl FnMut(&Url) -> nash_config::Build + Send + 'static,
) -> Result<Vec<nash_driver::build::ValidatorOutput>, nash_driver::build::BuildError> {
    let source = InMemorySource::new();
    let modules: BTreeMap<_, _> = files
        .iter()
        .map(|(name, text)| {
            let uri = Url::parse(&format!("file:///project/src/{name}.nash")).unwrap();
            source.insert(uri.clone(), (*text).to_owned());
            (uri, None)
        })
        .collect();
    let db = Arc::new(Mutex::new(Database::new(source)));
    let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
        .await
        .unwrap();
    let (report, result) = build_with(db, &graph, &modules, move |solved| {
        nash_driver::build::build_validators_with(solved, config_for)
    })
    .await;
    assert!(report.is_success(), "{report:?}");
    result.unwrap()
}

#[tokio::test]
async fn source_dependency_compiles_to_roundtrippable_script() {
    let files = [
        (
            "Helper",
            "module Helper exposing (identity)\nidentity x = x\n",
        ),
        (
            "Main",
            "validator module Main exposing (main)\nimport Helper\nmain : Data -> unit\nmain _ = Helper.identity ()\n",
        ),
    ];
    let artifacts = outputs(&files).await;
    assert_eq!(artifacts.len(), 1);
    let output = &artifacts[0];
    assert_eq!(output.module, "Main");
    insta::with_settings!({ description => files.iter().map(|(_, source)| *source).collect::<Vec<_>>().join("\n"), omit_expression => true }, { insta::assert_snapshot!(output.uplc); });
    let arena = Arena::new();
    let parsed = syn::parse_program(&arena, &output.uplc).unwrap();
    assert_eq!(flat::encode(parsed).unwrap(), output.flat);
    assert_eq!(flat::to_cbor(parsed).unwrap(), output.cbor);
}

#[tokio::test]
async fn ordinary_modules_produce_no_scripts() {
    assert!(
        outputs(&[("Lib", "module Lib exposing (id)\nid x = x\n")])
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn unconstrained_validator_input_uses_data() {
    let outputs = outputs(&[(
        "Main",
        "validator module Main exposing (main)\nmain _ = ()\n",
    )])
    .await;
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].uplc.contains("lam"));
}

struct OutputDirectory(std::path::PathBuf);

impl OutputDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nash-driver-artifacts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for OutputDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn writes_dotted_names_hex_cbor_and_removes_only_owned_stale_outputs() {
    use nash_driver::build::write_outputs;
    let directory = OutputDirectory::new();
    let mut artifacts = outputs(&[(
        "Main",
        "validator module Main exposing (main)\nmain _ = ()\n",
    )])
    .await;
    artifacts[0].module = "Policy.Main".into();
    for name in [
        "notes.txt",
        "Unrelated.cbor",
        "Unrelated.flat",
        "Other.uplc",
    ] {
        std::fs::write(directory.0.join(name), "keep me").unwrap();
    }
    write_outputs(&directory.0, &artifacts).await.unwrap();
    assert_eq!(
        std::fs::read(directory.0.join("Policy.Main.flat")).unwrap(),
        artifacts[0].flat
    );
    assert_eq!(
        std::fs::read_to_string(directory.0.join("Policy.Main.cbor")).unwrap(),
        hex::encode(&artifacts[0].cbor)
    );
    assert_eq!(
        std::fs::read_to_string(directory.0.join("Policy.Main.uplc")).unwrap(),
        artifacts[0].uplc
    );
    artifacts[0].module = "Renamed".into();
    write_outputs(&directory.0, &artifacts).await.unwrap();
    for extension in ["uplc", "flat", "cbor"] {
        assert!(
            !directory
                .0
                .join(format!("Policy.Main.{extension}"))
                .exists()
        );
        assert!(directory.0.join(format!("Renamed.{extension}")).exists());
    }
    write_outputs(&directory.0, &[]).await.unwrap();
    for extension in ["uplc", "flat", "cbor"] {
        assert!(!directory.0.join(format!("Renamed.{extension}")).exists());
    }
    for name in [
        "notes.txt",
        "Unrelated.cbor",
        "Unrelated.flat",
        "Other.uplc",
    ] {
        assert_eq!(
            std::fs::read_to_string(directory.0.join(name)).unwrap(),
            "keep me"
        );
    }
}

#[tokio::test]
async fn rejects_manifest_traversal_without_touching_outputs() {
    let directory = OutputDirectory::new();
    let out = directory.0.join("build");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(directory.0.join("Outside.flat"), "keep me").unwrap();
    std::fs::write(out.join("Current.flat"), "existing output").unwrap();
    std::fs::write(
        out.join(".nash-artifacts"),
        "nash-artifacts-v1\n../Outside.flat\n",
    )
    .unwrap();
    assert!(nash_driver::build::write_outputs(&out, &[]).await.is_err());
    assert_eq!(
        std::fs::read_to_string(directory.0.join("Outside.flat")).unwrap(),
        "keep me"
    );
    assert_eq!(
        std::fs::read_to_string(out.join("Current.flat")).unwrap(),
        "existing output"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn refuses_symlink_artifacts_and_manifests_without_touching_their_targets() {
    let directory = OutputDirectory::new();
    let out = directory.0.join("build");
    std::fs::create_dir(&out).unwrap();
    let outside = directory.0.join("Outside.flat");
    std::fs::write(&outside, "keep me").unwrap();
    std::os::unix::fs::symlink(&outside, out.join("Main.flat")).unwrap();
    std::fs::write(
        out.join(".nash-artifacts"),
        "nash-artifacts-v1\nMain.flat\n",
    )
    .unwrap();
    assert!(nash_driver::build::write_outputs(&out, &[]).await.is_err());
    assert!(out.join("Main.flat").is_symlink());
    std::fs::remove_file(out.join(".nash-artifacts")).unwrap();
    std::os::unix::fs::symlink(&outside, out.join(".nash-artifacts")).unwrap();
    assert!(nash_driver::build::write_outputs(&out, &[]).await.is_err());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep me");
}

#[tokio::test]
async fn roots_use_their_own_target_settings_and_hashes() {
    let artifacts = outputs_with(
        &[
            (
                "First",
                "validator module First exposing (main)\nmain _ = ()\n",
            ),
            (
                "Second",
                "validator module Second exposing (main)\nmain _ = ()\n",
            ),
        ],
        |uri| nash_config::Build {
            plutus_version: if uri.path().ends_with("First.nash") {
                nash_config::PlutusVersion::V1
            } else {
                nash_config::PlutusVersion::V3
            },
            ..Default::default()
        },
    )
    .await;
    assert_eq!(artifacts.len(), 2);
    assert!(artifacts[0].uplc.contains("1.0.0"));
    assert!(artifacts[1].uplc.contains("1.1.0"));
    for (artifact, version) in artifacts.iter().zip([
        nash_plutus::machine::PlutusVersion::V1,
        nash_plutus::machine::PlutusVersion::V3,
    ]) {
        assert_eq!(
            artifact.hash,
            nash_plutus::script::script_hash(version, &artifact.cbor)
        );
    }
    assert_ne!(artifacts[0].hash, artifacts[1].hash);
}

#[tokio::test]
async fn unowned_destination_collision_preserves_every_file() {
    use nash_driver::build::write_outputs;
    let directory = OutputDirectory::new();
    let mut artifacts = outputs(&[(
        "Main",
        "validator module Main exposing (main)\nmain _ = ()\n",
    )])
    .await;
    // Preserve pre-manifest artifacts too: ownership cannot be inferred from a suffix.
    std::fs::write(directory.0.join("Main.cbor"), "unrelated content").unwrap();
    let before = directory_contents(&directory.0);
    let error = write_outputs(&directory.0, &artifacts).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(directory_contents(&directory.0), before);

    artifacts[0].module = "PreviouslyBuilt".into();
    write_outputs(&directory.0, &artifacts).await.unwrap();
    let before = directory_contents(&directory.0);
    artifacts[0].module = "Main".into();
    let error = write_outputs(&directory.0, &artifacts).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    // Includes the existing manifest and all three previously owned artifacts.
    assert_eq!(directory_contents(&directory.0), before);
}

fn directory_contents(path: &std::path::Path) -> BTreeMap<std::ffi::OsString, Vec<u8>> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect()
}

#[tokio::test]
async fn case_alias_validator_roots_are_rejected() {
    let error = outputs_result_with(
        &[
            (
                "Main",
                "validator module Main exposing (main)\nmain _ = ()\n",
            ),
            (
                "MAIN",
                "validator module MAIN exposing (main)\nmain _ = ()\n",
            ),
        ],
        |_| nash_config::Build::default(),
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("ignoring ASCII case"));
}

#[tokio::test]
async fn case_alias_outputs_and_renames_preserve_existing_artifacts() {
    use nash_driver::build::write_outputs;
    let directory = OutputDirectory::new();
    let mut artifacts = outputs(&[
        (
            "First",
            "validator module First exposing (main)\nmain _ = ()\n",
        ),
        (
            "Second",
            "validator module Second exposing (main)\nmain _ = ()\n",
        ),
    ])
    .await;
    artifacts[0].module = "Main".into();
    write_outputs(&directory.0, &artifacts[..1]).await.unwrap();
    let before = directory_contents(&directory.0);
    artifacts[1].module = "MAIN".into();
    let error = write_outputs(&directory.0, &artifacts).await.unwrap_err();
    assert!(error.to_string().contains("ignoring ASCII case"));
    assert_eq!(directory_contents(&directory.0), before);

    let error = write_outputs(&directory.0, &artifacts[1..])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("differ only by ASCII case"));
    assert_eq!(directory_contents(&directory.0), before);

    // Even a manifest containing aliases must not trigger ambiguous stale cleanup.
    let manifest = directory.0.join(".nash-artifacts");
    let mut contents = std::fs::read_to_string(&manifest).unwrap();
    contents.push_str("MAIN.flat\n");
    std::fs::write(manifest, contents).unwrap();
    let before = directory_contents(&directory.0);
    assert!(write_outputs(&directory.0, &[]).await.is_err());
    assert_eq!(directory_contents(&directory.0), before);
}
