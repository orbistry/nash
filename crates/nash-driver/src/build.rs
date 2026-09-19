//! Owned validator artifacts produced while solved canonical nodes are alive.
use crate::compile::Solved;
use nash_ast::{ModuleKind, QualifiedName};
use nash_codegen::build::{Build, Input, TraceConfig};
use nash_plutus::{arena::Arena, flat, pretty, program::Program};
use std::{collections::BTreeSet, io, path::Path};

#[derive(Debug)]
pub struct ValidatorOutput {
    pub module: String,
    pub uplc: String,
    pub flat: Vec<u8>,
    /// A single CBOR byte string containing the Flat script.
    pub cbor: Vec<u8>,
    pub hash: [u8; 28],
}

#[derive(Debug, thiserror::Error)]
#[error("code generation failed in {module}: {message}")]
pub struct BuildError {
    pub module: String,
    pub message: String,
}

pub fn build_validators(
    solved: Solved<'_>,
    config: nash_config::Build,
) -> Result<Vec<ValidatorOutput>, BuildError> {
    build_validators_with(solved, |_| config)
}

/// Resolve settings separately for each validator root (including workspace members).
pub fn build_validators_with(
    solved: Solved<'_>,
    mut config_for: impl FnMut(&url::Url) -> nash_config::Build,
) -> Result<Vec<ValidatorOutput>, BuildError> {
    build_validators_matching_with(solved, |uri| Some(config_for(uri)))
}

/// Compile selected validator roots while retaining all dependency definitions.
/// Return `None` for dependency modules that must not emit artifacts.
pub fn build_validators_matching_with(
    solved: Solved<'_>,
    mut config_for: impl FnMut(&url::Url) -> Option<nash_config::Build>,
) -> Result<Vec<ValidatorOutput>, BuildError> {
    let arena = Arena::new();
    let build = Build::new(solved.modules.iter().map(|module| Input {
        module: module.module,
        types: &module.types,
        tables: &module.tables,
    }));
    let mut outputs = Vec::new();
    for module in &solved.modules {
        if !matches!(module.module.kind, ModuleKind::Validator(_)) {
            continue;
        }
        let Some(config) = config_for(&module.uri) else {
            continue;
        };
        let trace = TraceConfig {
            user: match config.trace_level {
                nash_config::TraceLevel::Silent => nash_codegen::build::TraceLevel::Silent,
                nash_config::TraceLevel::Compact => nash_codegen::build::TraceLevel::Compact,
                nash_config::TraceLevel::Verbose => nash_codegen::build::TraceLevel::Verbose,
            },
            compiler: config.compiler_traces,
        };
        let version = match config.plutus_version {
            nash_config::PlutusVersion::V1 => nash_plutus::machine::PlutusVersion::V1,
            nash_config::PlutusVersion::V2 => nash_plutus::machine::PlutusVersion::V2,
            nash_config::PlutusVersion::V3 => nash_plutus::machine::PlutusVersion::V3,
        };
        let name = module.module.name.name;
        let error = |message: String| BuildError {
            module: name.to_owned(),
            message,
        };
        let annotation = module
            .annotations
            .get("main")
            .ok_or_else(|| error("validator has no main annotation".into()))?;
        // Validator callers supply constants. Unconstrained root variables use
        // Data; ordinary program roots still require an explicit instance.
        let data = arena.alloc(nash_region::Located::at_zero(nash_ast::Type::Named {
            reference: QualifiedName {
                home: nash_ast::primitives::builtin_home(),
                name: "Data",
            },
            args: &[],
        }));
        let args = vec![&*data; annotation.free_vars.len()];
        let core = build
            .compile(
                &arena,
                QualifiedName {
                    home: module.module.name,
                    name: "main",
                },
                Some(&args),
                trace,
            )
            .map_err(|e| error(e.to_string()))?;
        let compiled = nash_codegen::program::assemble_core_for_version(&arena, core.core, version)
            .map_err(|e| error(e.to_string()))?;
        let uplc = pretty::program(Program::new(
            &arena,
            compiled.program.version,
            compiled.named,
        ));
        let bytes = flat::encode(compiled.program).map_err(|e| error(e.to_string()))?;
        let cbor = flat::to_cbor(compiled.program).map_err(|e| error(e.to_string()))?;
        outputs.push(ValidatorOutput {
            module: name.to_owned(),
            uplc,
            flat: bytes,
            hash: nash_plutus::script::script_hash(version, &cbor),
            cbor,
        });
    }
    outputs.sort_by(|a, b| a.module.cmp(&b.module));
    let mut module_names = BTreeSet::new();
    for output in &outputs {
        if !module_names.insert(output.module.to_ascii_lowercase()) {
            return Err(BuildError {
                module: output.module.clone(),
                message: "multiple validator modules would write the same output filename (ignoring ASCII case)".into(),
            });
        }
    }
    Ok(outputs)
}

const MANIFEST: &str = ".nash-artifacts";
const MANIFEST_HEADER: &str = "nash-artifacts-v1";

fn valid_module_name(name: &str) -> bool {
    name.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

fn valid_artifact_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(module, extension)| {
        valid_module_name(module) && matches!(extension, "uplc" | "flat" | "cbor")
    })
}

// Reject links before either reading the manifest or overwriting artifacts.
// Cleanup never follows paths supplied by the manifest into subdirectories.
async fn check_regular_or_missing(path: &Path) -> io::Result<bool> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing non-regular artifact {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Write only artifacts from a successful build. Module names retain their
/// dots, matching `build/Module.Name.{uplc,flat,cbor}`. CBOR is hex text.
/// A manifest records ownership so stale cleanup preserves unrelated files.
pub async fn write_outputs(directory: &Path, outputs: &[ValidatorOutput]) -> io::Result<()> {
    let mut names = BTreeSet::new();
    let mut module_names = BTreeSet::new();
    for output in outputs {
        if !valid_module_name(&output.module) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid validator module name",
            ));
        }
        if !module_names.insert(output.module.to_ascii_lowercase()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "duplicate validator module name (ignoring ASCII case)",
            ));
        }
        for extension in ["uplc", "flat", "cbor"] {
            if !names.insert(format!("{}.{}", output.module, extension)) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "duplicate validator module name",
                ));
            }
        }
    }
    let manifest_path = directory.join(MANIFEST);
    check_regular_or_missing(&manifest_path).await?;
    let mut previous = BTreeSet::new();
    match tokio::fs::read_to_string(&manifest_path).await {
        Ok(contents) => {
            let mut lines = contents.lines();
            if lines.next() != Some(MANIFEST_HEADER) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Nash artifact manifest",
                ));
            }
            for name in lines {
                if !valid_artifact_name(name) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid artifact filename in Nash manifest",
                    ));
                }
                previous.insert(name.to_owned());
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    // Apply one portable filename identity even on case-sensitive filesystems.
    // Case-only renames must first remove the old owned outputs with an empty build.
    let mut folded_names = BTreeSet::new();
    for name in names.union(&previous) {
        if !folded_names.insert(name.to_ascii_lowercase()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "artifact filenames differ only by ASCII case near {name}; remove old owned outputs before renaming"
                ),
            ));
        }
    }
    // Validate all destinations before changing any artifact.
    for name in names.union(&previous) {
        let exists = check_regular_or_missing(&directory.join(name)).await?;
        if exists && !previous.contains(name) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "refusing to overwrite unowned artifact {}",
                    directory.join(name).display()
                ),
            ));
        }
    }
    if outputs.is_empty() && previous.is_empty() {
        return Ok(());
    }
    tokio::fs::create_dir_all(directory).await?;
    for output in outputs {
        let cbor_hex = hex::encode(&output.cbor);
        for (extension, bytes) in [
            ("uplc", output.uplc.as_bytes()),
            ("flat", output.flat.as_slice()),
            ("cbor", cbor_hex.as_bytes()),
        ] {
            tokio::fs::write(
                directory.join(format!("{}.{}", output.module, extension)),
                bytes,
            )
            .await?;
        }
    }
    for stale in previous.difference(&names) {
        match tokio::fs::remove_file(directory.join(stale)).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let mut manifest = format!("{MANIFEST_HEADER}\n");
    for name in names {
        manifest.push_str(&name);
        manifest.push('\n');
    }
    tokio::fs::write(manifest_path, manifest).await
}

/// Compile selected module tests to owned programs before dropping the solved arena.
/// Return `None` for dependency modules whose tests must not execute.
pub fn compile_tests_with(
    solved: Solved<'_>,
    config_for: impl FnMut(&url::Url) -> Option<nash_config::Build>,
) -> Result<Vec<nash_test::TestProgram>, BuildError> {
    compile_tests_matching_with(solved, config_for, |_, _| true)
}

/// Select test roots before code generation so excluded tests cannot cause backend errors.
pub fn compile_tests_matching_with(
    solved: Solved<'_>,
    mut config_for: impl FnMut(&url::Url) -> Option<nash_config::Build>,
    mut include: impl FnMut(&str, &str) -> bool,
) -> Result<Vec<nash_test::TestProgram>, BuildError> {
    let arena = Arena::new();
    let build = Build::new(solved.modules.iter().map(|module| Input {
        module: module.module,
        types: &module.types,
        tables: &module.tables,
    }));
    let mut outputs = Vec::new();
    for module in &solved.modules {
        let Some(config) = config_for(&module.uri) else {
            continue;
        };
        let config = config.for_tests();
        let trace = TraceConfig {
            user: match config.trace_level {
                nash_config::TraceLevel::Silent => nash_codegen::build::TraceLevel::Silent,
                nash_config::TraceLevel::Compact => nash_codegen::build::TraceLevel::Compact,
                nash_config::TraceLevel::Verbose => nash_codegen::build::TraceLevel::Verbose,
            },
            compiler: true,
        };
        let path = module
            .uri
            .to_file_path()
            .unwrap_or_else(|_| module.uri.path().into());
        let programs = nash_codegen::tests::compile_tests_matching(
            &arena,
            &build,
            module.module.name,
            module.source,
            &path,
            config.plutus_version,
            trace,
            |test| include(module.module.name.name, test.name.value),
        )
        .map_err(|error| BuildError {
            module: module.module.name.name.to_owned(),
            message: error.to_string(),
        })?;
        outputs.extend(programs);
    }
    Ok(outputs)
}
