use std::path::PathBuf;
use std::sync::Arc;

use miette::{IntoDiagnostic, Result};
use nash_driver::{Database, FileSystemSource, ModuleResult, Project, build, build_graph};
use nash_report::Severity;
use tokio::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ReportFormat {
    Human,
    Json,
}

#[derive(clap::Args)]
pub struct Args {
    /// Path to the project (defaults to current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Diagnostic output format.
    #[arg(long, value_enum, default_value = "human")]
    pub report: ReportFormat,
    /// Hide compiler warnings.
    #[arg(long)]
    pub no_warnings: bool,
}

impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        self.report_result(self.check().await, color)
    }

    pub(crate) fn report_result(
        &self,
        result: Result<nash_driver::BuildResult>,
        color: bool,
    ) -> Result<()> {
        let result = match result {
            Ok(result) => result,
            Err(error) if self.report == ReportFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "error", "path": self.path, "title": "PROJECT ERROR", "message": [error.to_string()]
                    })
                );
                std::process::exit(1);
            }
            Err(error) => return Err(error),
        };
        let modules = result.ordered_reports();
        if self.report == ReportFormat::Json {
            let select = |severity| {
                modules
                    .iter()
                    .filter_map(|module| {
                        let mut selected = (**module).clone();
                        selected
                            .reports
                            .retain(|report| report.severity == severity);
                        (!selected.reports.is_empty()).then_some(selected)
                    })
                    .collect::<Vec<_>>()
            };
            println!(
                "{}",
                nash_report::json::compile_errors(&select(Severity::Error))
            );
            if !self.no_warnings {
                let warnings = select(Severity::Warning);
                if !warnings.is_empty() {
                    eprintln!("{}", nash_report::json::compile_warnings(&warnings));
                }
            }
        } else {
            for module in &modules {
                let source = nash_report::Source::new(&module.source);
                for report in &module.reports {
                    if self.no_warnings && report.severity == Severity::Warning {
                        continue;
                    }
                    eprintln!(
                        "{:?}",
                        miette::Report::new(report.render(&source, &module.path, color))
                    );
                }
            }
        }
        let mut states: Vec<_> = result.modules.iter().collect();
        states.sort_by_key(|(uri, _)| *uri);
        for (uri, state) in states {
            match state {
                ModuleResult::Blocked { dependencies } if self.report == ReportFormat::Human => {
                    eprintln!(
                        "Skipped {} because these dependencies failed: {}",
                        uri.path(),
                        dependencies
                            .iter()
                            .map(|uri| uri.path())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                ModuleResult::SourceUnavailable { message } => {
                    if self.report == ReportFormat::Json {
                        eprintln!(
                            "{}",
                            serde_json::json!({"type":"error", "path":uri.path(), "title":"SOURCE UNAVAILABLE", "message":[message]})
                        );
                    } else {
                        eprintln!("Could not read {}: {}", uri.path(), message);
                    }
                }
                _ => {}
            }
        }
        if result.is_success() {
            if self.report == ReportFormat::Human {
                let declarations: usize = result
                    .modules
                    .values()
                    .filter_map(|result| match result {
                        ModuleResult::Success { decl_count } => Some(decl_count),
                        _ => None,
                    })
                    .sum();
                eprintln!(
                    "Success! Compiled {} modules ({} declarations)",
                    result.total, declarations
                );
            }
            Ok(())
        } else {
            if self.report == ReportFormat::Human {
                eprintln!(
                    "Compilation failed: {} succeeded, {} failed or blocked.",
                    result.success, result.failed
                );
            }
            std::process::exit(1);
        }
    }

    async fn check(&self) -> Result<nash_driver::BuildResult> {
        let project = Project::load(&self.path).await.into_diagnostic()?;
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let modules = project
            .discover_modules(&*db.lock().await)
            .await
            .into_diagnostic()?;
        let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>())
            .await
            .into_diagnostic()?;
        Ok(build(db, &graph, &modules).await)
    }
}
