use miette::{IntoDiagnostic, Result};
use nash_driver::{Database, FileSystemSource, Project, build_graph_production, build_with};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

#[derive(clap::Args)]
pub struct Args {
    /// Path to the project.
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Directory for UPLC, Flat and CBOR files, relative to the project root.
    #[arg(long, default_value = "build")]
    pub out: PathBuf,
    /// Include user traces in the script.
    #[arg(long, value_enum)]
    pub trace_level: Option<TraceLevelArg>,
    /// Ledger language target (Plomin/protocol 10 compatibility).
    #[arg(long, value_enum)]
    pub plutus_version: Option<PlutusVersionArg>,
    /// Include compiler traces for failed casts and pattern checks.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub compiler_traces: Option<bool>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum TraceLevelArg {
    Silent,
    Compact,
    Verbose,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum PlutusVersionArg {
    V1,
    V2,
    V3,
}

impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        let project = Project::load(&self.path).await.into_diagnostic()?;
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let modules = project
            .discover_modules_production(&*db.lock().await)
            .await
            .into_diagnostic()?;
        let graph =
            build_graph_production(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
                .await
                .into_diagnostic()?;
        let member_settings: Vec<_> = project
            .members
            .iter()
            .flat_map(|member| {
                member.source_dirs.iter().map(move |directory| {
                    (
                        directory
                            .canonicalize()
                            .unwrap_or_else(|_| directory.clone()),
                        member.config.build(),
                    )
                })
            })
            .collect();
        let roots = project
            .discover_own_modules(&*db.lock().await)
            .await
            .into_diagnostic()?;
        let configs: std::collections::BTreeMap<_, _> = roots
            .keys()
            .map(|uri| {
                let path = uri
                    .to_file_path()
                    .expect("discovered source has a file URL");
                let path = path.canonicalize().unwrap_or(path);
                let mut config = member_settings
                    .iter()
                    .filter(|(directory, _)| path.starts_with(directory))
                    .max_by_key(|(directory, _)| directory.components().count())
                    .map_or_else(|| project.config.build(), |(_, config)| *config);
                if let Some(version) = self.plutus_version {
                    config.plutus_version = match version {
                        PlutusVersionArg::V1 => nash_config::PlutusVersion::V1,
                        PlutusVersionArg::V2 => nash_config::PlutusVersion::V2,
                        PlutusVersionArg::V3 => nash_config::PlutusVersion::V3,
                    };
                }
                if let Some(level) = self.trace_level {
                    config.trace_level = match level {
                        TraceLevelArg::Silent => nash_config::TraceLevel::Silent,
                        TraceLevelArg::Compact => nash_config::TraceLevel::Compact,
                        TraceLevelArg::Verbose => nash_config::TraceLevel::Verbose,
                    };
                }
                if let Some(enabled) = self.compiler_traces {
                    config.compiler_traces = enabled;
                }
                (uri.clone(), config)
            })
            .collect();
        let (report, output) = build_with(db, &graph, &modules, move |solved| {
            nash_driver::build::build_validators_matching_with(solved, |uri| {
                configs.get(uri).copied()
            })
        })
        .await;
        super::check::Args {
            path: self.path,
            report: super::check::ReportFormat::Human,
            no_warnings: false,
        }
        .report_result(Ok((project.root.clone(), report)), color)?;
        let outputs = output
            .expect("successful frontend invokes finish")
            .into_diagnostic()?;
        let directory = project.root.join(self.out);
        nash_driver::build::write_outputs(&directory, &outputs)
            .await
            .into_diagnostic()?;
        for output in &outputs {
            eprintln!("Built {} ({} Flat bytes)", output.module, output.flat.len());
            eprintln!("  hash {}", hex::encode(output.hash));
        }
        eprintln!(
            "Finished {} validators in {}",
            outputs.len(),
            directory.display()
        );
        Ok(())
    }
}
