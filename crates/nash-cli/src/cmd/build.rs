use miette::{IntoDiagnostic, Result};
use nash_codegen::build::{TraceConfig, TraceLevel};
use nash_driver::{Database, FileSystemSource, Project, build_graph, build_with};
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
    #[arg(long, value_enum, default_value = "silent")]
    pub trace_level: TraceLevelArg,
    /// Include compiler traces for failed casts and pattern checks.
    #[arg(long)]
    pub compiler_traces: bool,
    /// Select the Aiken environment used by env and config imports.
    #[arg(long)]
    pub env: Option<String>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum TraceLevelArg {
    Silent,
    Compact,
    Verbose,
}

impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        let project = Project::load_with_options(
            &self.path,
            self.env.as_deref(),
            nash_driver::ProjectMode::Build,
        )
        .await
        .into_diagnostic()?;
        super::check::project_warnings(
            &project.loaded.warnings,
            super::check::ReportFormat::Human,
            false,
        );
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let modules = project
            .discover_modules(&*db.lock().await)
            .await
            .into_diagnostic()?;
        let graph = build_graph(db.clone(), &modules).await.into_diagnostic()?;
        let trace = TraceConfig {
            user: match self.trace_level {
                TraceLevelArg::Silent => TraceLevel::Silent,
                TraceLevelArg::Compact => TraceLevel::Compact,
                TraceLevelArg::Verbose => TraceLevel::Verbose,
            },
            compiler: self.compiler_traces,
        };
        let (report, output) = build_with(db, &graph, &modules, move |solved| {
            nash_driver::build::build_validators(solved, trace)
        })
        .await;
        super::check::Args {
            path: self.path,
            report: super::check::ReportFormat::Human,
            no_warnings: false,
            env: self.env,
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
            eprintln!(
                "Built {} ({} Flat bytes)",
                output.output_name,
                output.flat.len()
            );
        }
        eprintln!(
            "Finished {} validators in {}",
            outputs.len(),
            directory.display()
        );
        Ok(())
    }
}
