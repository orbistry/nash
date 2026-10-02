use super::build::PlutusVersionArg;
use miette::{IntoDiagnostic, Result};
use nash_driver::{Database, FileSystemSource, Project, build_graph_with_tests, test_with};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(clap::Args)]
pub struct Args {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Directory for a managed Lean project; each run preserves its own sources.
    #[arg(long, default_value = "build/proofs")]
    pub output: PathBuf,
    /// Export without invoking Lean or a solver.
    #[arg(long)]
    pub emit_only: bool,
    /// Existing Lake project with the pinned dependencies already built.
    #[arg(long)]
    pub lean_project: Option<PathBuf>,
    /// CEK execution step limit; exhaustion counts as rejection.
    #[arg(long, default_value = "10000", value_parser = positive)]
    pub fuel: u32,
    /// Separate CEK limit for evaluating a partial-correctness postcondition.
    #[arg(long, default_value = "10000", value_parser = positive)]
    pub postcondition_fuel: u32,
    #[arg(long, default_value = "30", value_parser = positive)]
    pub timeout: u32,
    #[arg(long, default_value = "120", value_parser = positive)]
    pub wall_timeout: u32,
    #[arg(short = 'm', long = "match")]
    pub matches: Vec<String>,
    #[arg(long)]
    pub exact: bool,
    #[arg(long, value_enum)]
    pub plutus_version: Option<PlutusVersionArg>,
    #[arg(long)]
    pub json: bool,
}
fn positive(value: &str) -> std::result::Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "expected a positive integer".into())
}
impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        let project = Project::load(&self.path).await.into_diagnostic()?;
        let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
        let modules = project
            .discover_modules(&*db.lock().await)
            .await
            .into_diagnostic()?;
        let roots = project
            .discover_own_modules(&*db.lock().await)
            .await
            .into_diagnostic()?;
        let graph = build_graph_with_tests(
            db.clone(),
            &modules.keys().cloned().collect::<Vec<_>>(),
            &roots.keys().cloned().collect::<Vec<_>>(),
        )
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
        let configs: std::collections::BTreeMap<_, _> = roots
            .keys()
            .map(|uri| {
                let path = uri.to_file_path().expect("discovered file URL");
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
                (uri.clone(), config)
            })
            .collect();
        let patterns = self.matches.clone();
        let exact = self.exact;
        let (report, programs) = test_with(db, &graph, &modules, move |solved| {
            nash_driver::build::compile_proofs_matching_with(
                solved,
                |uri| configs.get(uri).copied(),
                |module, name| super::test::matches_filter(&patterns, exact, module, name),
            )
        })
        .await;
        if !report.is_success() || !self.json {
            super::check::Args {
                path: self.path.clone(),
                report: if self.json {
                    super::check::ReportFormat::Json
                } else {
                    super::check::ReportFormat::Human
                },
                no_warnings: false,
            }
            .report_result(Ok((project.root.clone(), report)), color)?;
        }
        let programs = programs
            .expect("successful frontend invokes finish")
            .into_diagnostic()?;
        if programs.is_empty() {
            return Err(miette::miette!("No proof declarations matched."));
        }
        let output = if self.output.is_absolute() {
            self.output.clone()
        } else {
            project.root.join(&self.output)
        };
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).into_diagnostic()?;
        }
        let files = nash_proof::export(
            &programs,
            &output,
            self.fuel,
            self.postcondition_fuel,
            self.timeout,
        )
        .into_diagnostic()?;
        if self.emit_only {
            if self.json {
                println!(
                    "{}",
                    serde_json::json!({"status":"exported", "output":output, "files":files})
                );
            } else {
                eprintln!("Exported {} proof(s) to {}", files.len(), output.display());
            }
            return Ok(());
        }
        let lean_project = self.lean_project.as_ref().map_or_else(
            || output.clone(),
            |p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    project.root.join(p)
                }
            },
        );
        if self.lean_project.is_none() {
            let mut setup = Vec::new();
            if !lean_project.join("lake-manifest.json").exists() {
                setup.push(vec!["update"]);
            }
            setup.push(vec!["build", "CardanoLedgerApi"]);
            for args in setup {
                let status = std::process::Command::new("lake")
                    .current_dir(&lean_project)
                    .args(args)
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .into_diagnostic()?;
                if !status.success() {
                    return Err(miette::miette!(
                        "Lean backend setup failed. Sources are preserved at {}",
                        output.display()
                    ));
                }
            }
        }
        let config = nash_proof::Config {
            lean_project,
            fuel: self.fuel,
            postcondition_fuel: self.postcondition_fuel,
            solver_timeout: self.timeout,
            wall_timeout: Duration::from_secs(u64::from(self.wall_timeout)),
        };
        let mut outcomes = Vec::new();
        for (program, file) in programs.iter().zip(files) {
            let outcome = nash_proof::run(program, &file, &config).into_diagnostic()?;
            if !self.json {
                let limits = if program.postcondition.is_some() {
                    format!(
                        "execution fuel {}, postcondition fuel {}",
                        outcome.fuel, outcome.postcondition_fuel
                    )
                } else {
                    format!("execution fuel {}", outcome.fuel)
                };
                eprintln!(
                    "{}: {:?} (SMT verification, {limits})",
                    outcome.name, outcome.status
                );
                for entry in &outcome.counterexample {
                    eprintln!("  {entry}");
                }
                if matches!(outcome.status, nash_proof::Status::BackendError) {
                    eprintln!("{}", outcome.diagnostics);
                }
            }
            outcomes.push(outcome);
        }
        if self.json {
            println!("{}", serde_json::to_string(&outcomes).into_diagnostic()?);
        }
        if outcomes.iter().any(|outcome| !outcome.passed()) {
            return Err(miette::miette!(
                "One or more proof obligations remain unverified. Sources are preserved at {}",
                output.display()
            ));
        }
        Ok(())
    }
}
