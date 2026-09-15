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
    /// Select the Aiken environment used by env and config imports.
    #[arg(long)]
    pub env: Option<String>,
}

impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        self.report_result(self.check().await, color)
    }

    pub(crate) fn report_result(
        &self,
        result: std::result::Result<(PathBuf, nash_driver::BuildResult), nash_driver::DriverError>,
        color: bool,
    ) -> Result<()> {
        let (root, result) = match result {
            Ok(result) => result,
            Err(error) if self.report == ReportFormat::Json => {
                match &error {
                    nash_driver::DriverError::ProjectLoad { diagnostics } => {
                        for diagnostic in diagnostics {
                            println!("{}", project_diagnostic_json(diagnostic, "error"));
                        }
                    }
                    nash_driver::DriverError::ProjectDiagnostic(diagnostic) => {
                        println!("{}", project_diagnostic_json(diagnostic, "error"));
                    }
                    _ => println!(
                        "{}",
                        serde_json::json!({
                            "type": "error", "path": self.path, "title": "PROJECT ERROR", "message": [error.to_string()]
                        })
                    ),
                }
                std::process::exit(1);
            }
            Err(error) => return Err(error).into_diagnostic(),
        };
        let links = supports_hyperlinks::on(supports_hyperlinks::Stream::Stderr);
        let source_name = |name: &str| crate::reporting::source_name(&root, links, name);
        let uri_name = |uri: &url::Url| {
            source_name(&uri.to_file_path().map_or_else(
                |_| uri.to_string(),
                |path| path.to_string_lossy().into_owned(),
            ))
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
                        miette::Report::new(
                            report
                                .render(&source, &module.path, color)
                                .map_source_names(&source_name)
                        )
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
                        uri_name(uri),
                        dependencies
                            .iter()
                            .map(&uri_name)
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
                        eprintln!("Could not read {}: {}", uri_name(uri), message);
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

    async fn check(
        &self,
    ) -> std::result::Result<(PathBuf, nash_driver::BuildResult), nash_driver::DriverError> {
        let project = Project::load_with_options(
            &self.path,
            self.env.as_deref(),
            nash_driver::ProjectMode::Check,
        )
        .await?;
        project_warnings(&project.loaded.warnings, self.report, self.no_warnings);
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let modules = project.discover_modules(&*db.lock().await).await?;
        let graph = build_graph(db.clone(), &modules).await?;
        Ok((project.root, build(db, &graph, &modules).await))
    }
}

pub(crate) fn project_warnings(
    warnings: &[nash_driver::ProjectDiagnostic],
    format: ReportFormat,
    hidden: bool,
) {
    if hidden {
        return;
    }
    for warning in warnings {
        if format == ReportFormat::Json {
            eprintln!("{}", project_diagnostic_json(warning, "warning"));
        } else {
            eprintln!("warning: {warning}");
        }
    }
}

fn project_diagnostic_json(
    diagnostic: &nash_driver::ProjectDiagnostic,
    severity: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": severity,
        "code": diagnostic.code,
        "path": diagnostic.path,
        "title": "PROJECT DIAGNOSTIC",
        "message": [diagnostic.message],
        "region": diagnostic.region.map(|region| serde_json::json!({
            "start": {"line": region.start.line, "column": region.start.column},
            "end": {"line": region.end.line, "column": region.end.column},
        })),
    })
}
