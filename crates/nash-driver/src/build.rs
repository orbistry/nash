//! Owned validator artifacts produced while solved canonical nodes are alive.
use crate::compile::Solved;
use nash_ast::{ModuleKind, QualifiedName};
use nash_codegen::build::{Build, Input, TraceConfig};
use nash_plutus::{arena::Arena, flat, pretty, program::Program};
use std::path::Path;

#[derive(Debug)]
pub struct ValidatorOutput {
    pub module: String,
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
        if !matches!(module.module.kind, ModuleKind::Validator(_)) {
            continue;
        }
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
        let compiled = nash_codegen::program::assemble_core(&arena, core.core)
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
            cbor,
        });
    }
    outputs.sort_by(|a, b| a.module.cmp(&b.module));
    if let Some(pair) = outputs
        .windows(2)
        .find(|pair| pair[0].module == pair[1].module)
    {
        return Err(BuildError {
            module: pair[0].module.clone(),
            message: "multiple validator modules would write the same output filename".into(),
        });
    }
    Ok(outputs)
}

/// Write only artifacts from a successful build. Module names retain their
/// dots, matching `build/Module.Name.{uplc,flat,cbor}`.
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
                directory.join(format!("{}.{}", output.module, extension)),
                bytes,
            )
            .await?;
        }
    }
    Ok(())
}
