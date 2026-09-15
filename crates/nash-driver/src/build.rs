//! Owned validator artifacts produced while solved canonical nodes are alive.
use crate::compile::Solved;
use nash_ast::{ModuleKind, QualifiedName};
use nash_codegen::build::{Build, Input, TraceConfig};
use nash_plutus::{arena::Arena, flat, pretty, program::Program};
use std::path::Path;

mod metadata;
pub use metadata::{
    ConstructorLayout, EntryPointId, SolvedBoundaryBinding, SolvedDataLayout,
    SolvedHandlerMetadata, SolvedType, TypeIdentity, ValidatorMetadata,
};

#[derive(Debug)]
pub struct ValidatorOutput {
    pub module: String,
    pub output_name: String,
    pub metadata: Option<ValidatorMetadata>,
    pub project: Option<std::sync::Arc<nash_frontend::ProjectMetadata>>,
    pub uplc: String,
    pub flat: Vec<u8>,
    /// A single CBOR byte string containing the Flat script.
    pub cbor: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
#[error("code generation failed in {module}: {message}")]
pub struct BuildError {
    pub module: String,
    pub message: String,
}

pub fn build_validators(
    solved: Solved<'_>,
    trace: TraceConfig,
) -> Result<Vec<ValidatorOutput>, BuildError> {
    let arena = Arena::new();
    let build = Build::new(solved.modules.iter().map(|module| Input {
        module: module.module,
        types: &module.types,
        tables: &module.tables,
    }));
    let mut outputs = Vec::new();
    for module in &solved.modules {
        if matches!(module.module.kind, ModuleKind::Validator(_)) {
            let name = module.module.name.name;
            let output = compile_entry(&arena, &build, module, "main", true, trace)?;
            outputs.push(ValidatorOutput {
                module: name.to_owned(),
                output_name: name.to_owned(),
                metadata: None,
                project: module.project.clone(),
                uplc: output.0,
                flat: output.1,
                cbor: output.2,
            });
        }
        for entry in module.source_entries {
            let metadata = metadata::validator(&solved, module, entry)?;
            let output = compile_entry(&arena, &build, module, entry.function, false, trace)?;
            let package = module.key.package.name.as_deref().unwrap_or("anonymous");
            let output_name = format!(
                "{}@{}.{}.{}",
                escape_component(package),
                escape_component(&module.key.package.version),
                escape_component(module.key.module.as_str()),
                escape_component(entry.name),
            );
            outputs.push(ValidatorOutput {
                module: module.module.name.name.to_owned(),
                output_name,
                metadata: Some(metadata),
                project: module.project.clone(),
                uplc: output.0,
                flat: output.1,
                cbor: output.2,
            });
        }
    }
    outputs.sort_by(|a, b| a.output_name.cmp(&b.output_name));
    if let Some(pair) = outputs
        .windows(2)
        .find(|pair| pair[0].output_name == pair[1].output_name)
    {
        return Err(BuildError {
            module: pair[0].module.clone(),
            message: format!(
                "validator identities collide at output filename {}",
                pair[0].output_name
            ),
        });
    }
    Ok(outputs)
}

fn compile_entry<'a>(
    arena: &'a Arena,
    build: &Build<'a, '_>,
    module: &crate::compile::SolvedModule<'a>,
    function: &'a str,
    native: bool,
    trace: TraceConfig,
) -> Result<(String, Vec<u8>, Vec<u8>), BuildError> {
    let error = |message: String| BuildError {
        module: module.module.name.name.to_owned(),
        message,
    };
    let annotation = module
        .annotations
        .get(function)
        .ok_or_else(|| error(format!("validator has no solved annotation for {function}")))?;
    if !native && !annotation.free_vars.is_empty() {
        return Err(error(
            "Aiken entry point has an unresolved public signature".to_owned(),
        ));
    }
    // Keep the native validator-root default; Aiken entries have concrete raw Data signatures.
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
            arena,
            QualifiedName {
                home: module.module.name,
                name: function,
            },
            Some(&args),
            trace,
        )
        .map_err(|error| BuildError {
            module: module.module.name.name.to_owned(),
            message: error.to_string(),
        })?;
    let compiled =
        nash_codegen::program::assemble_core(arena, core.core).map_err(|e| error(e.to_string()))?;
    let uplc = pretty::program(Program::new(
        arena,
        compiled.program.version,
        compiled.named,
    ));
    let flat = flat::encode(compiled.program).map_err(|e| error(e.to_string()))?;
    let cbor = flat::to_cbor(compiled.program).map_err(|e| error(e.to_string()))?;
    Ok((uplc, flat, cbor))
}

fn escape_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
            result.push(char::from(byte));
        } else {
            result.push('%');
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    result
}

/// Write only artifacts from a successful build. Native module stems are unchanged;
/// Aiken stems encode package/version, module and validator as separate components.
pub async fn write_outputs(directory: &Path, outputs: &[ValidatorOutput]) -> std::io::Result<()> {
    if outputs.is_empty() {
        return Ok(());
    }
    tokio::fs::create_dir_all(directory).await?;
    for output in outputs {
        for (extension, bytes) in [
            ("uplc", output.uplc.as_bytes()),
            ("flat", output.flat.as_slice()),
            ("cbor", output.cbor.as_slice()),
        ] {
            tokio::fs::write(
                directory.join(format!("{}.{}", output.output_name, extension)),
                bytes,
            )
            .await?;
        }
    }
    Ok(())
}
