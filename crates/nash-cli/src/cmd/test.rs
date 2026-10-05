use std::{path::PathBuf, sync::Arc, time::Instant};

use miette::{IntoDiagnostic, Result};
use nash_driver::{Database, FileSystemSource, Project, build_graph_with_tests, test_with};
use tokio::{sync::Mutex, task::spawn_blocking};

use super::build::{PlutusVersionArg, TraceLevelArg};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Coverage {
    Labels,
    Tests,
}

#[derive(clap::Args)]
pub struct Args {
    /// Optimization level: 0 (baseline), 1 (preserves traces), or 2 (silent).
    #[arg(short = 'O', long)]
    pub optimize: Option<nash_config::OptimizationLevel>,
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub seed: Option<u32>,
    #[arg(long, default_value = "100", value_parser = positive)]
    pub max_success: usize,
    #[arg(short = 'm', long = "match")]
    pub matches: Vec<String>,
    #[arg(long)]
    pub exact: bool,
    #[arg(long, value_enum)]
    pub trace_level: Option<TraceLevelArg>,
    #[arg(long, value_enum)]
    pub plutus_version: Option<PlutusVersionArg>,
    #[arg(long, default_value_t = default_jobs(), value_parser = positive)]
    pub jobs: usize,
    #[arg(long, value_enum, default_value = "labels")]
    pub coverage: Coverage,
    #[arg(long)]
    pub json: bool,
}

fn positive(value: &str) -> std::result::Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "expected a positive integer".into())
}
fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

impl Args {
    pub(crate) fn resolve_config(
        &self,
        mut config: nash_config::Build,
    ) -> std::result::Result<nash_config::Build, nash_config::BuildConflict> {
        if let Some(level) = self.optimize {
            config.optimize = level;
        }
        if let Some(level) = self.trace_level {
            config.trace_level_explicit = true;
            config.trace_level = match level {
                TraceLevelArg::Silent => nash_config::TraceLevel::Silent,
                TraceLevelArg::Compact => nash_config::TraceLevel::Compact,
                TraceLevelArg::Verbose => nash_config::TraceLevel::Verbose,
            };
        }
        if let Some(version) = self.plutus_version {
            config.plutus_version = match version {
                PlutusVersionArg::V1 => nash_config::PlutusVersion::V1,
                PlutusVersionArg::V2 => nash_config::PlutusVersion::V2,
                PlutusVersionArg::V3 => nash_config::PlutusVersion::V3,
            };
        }
        config.validate()?;
        Ok(config)
    }

    pub async fn exec(self, color: bool) -> Result<()> {
        let json = self.json;
        let path = self.path.clone();
        match self.exec_inner(color).await {
            Err(error) if json => {
                println!(
                    "{}",
                    serde_json::json!({"type":"error", "path":path, "title":"TEST ERROR", "message":[error.to_string()]})
                );
                std::process::exit(1);
            }
            result => result,
        }
    }

    async fn exec_inner(self, color: bool) -> Result<()> {
        let project = Project::load(&self.path).await.into_diagnostic()?;
        // Reject conflicts even when a project/member contains no source roots.
        self.resolve_config(project.config.build())
            .into_diagnostic()?;
        for member in &project.members {
            self.resolve_config(member.config.build())
                .into_diagnostic()?;
        }
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
                let config = member_settings
                    .iter()
                    .filter(|(directory, _)| path.starts_with(directory))
                    .max_by_key(|(directory, _)| directory.components().count())
                    .map_or_else(|| project.config.build(), |(_, config)| *config);
                Ok((uri.clone(), self.resolve_config(config)?))
            })
            .collect::<std::result::Result<_, nash_config::BuildConflict>>()
            .into_diagnostic()?;
        let patterns = self.matches.clone();
        let exact = self.exact;
        let (report, programs) = test_with(db, &graph, &modules, move |solved| {
            nash_driver::build::compile_tests_matching_with(
                solved,
                |uri| configs.get(uri).copied(),
                |module, name| matches_filter(&patterns, exact, module, name),
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
        let seed = self.seed.unwrap_or_else(|| {
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            (time ^ (time >> 32) ^ u128::from(std::process::id())) as u32
        });
        let started = Instant::now();
        let config = nash_test::Config {
            seed,
            max_success: self.max_success,
            jobs: self.jobs,
        };
        let outcomes = spawn_blocking(move || nash_test::run_all(programs, &config))
            .await
            .into_diagnostic()?;
        if self.json {
            println!(
                "{}",
                nash_test::report::json::render(seed, self.max_success, &outcomes)
            );
        } else {
            eprint!(
                "{}",
                nash_test::report::terminal::render(
                    &outcomes,
                    match self.coverage {
                        Coverage::Labels => nash_test::Coverage::Labels,
                        Coverage::Tests => nash_test::Coverage::Tests,
                    },
                    seed,
                    started.elapsed()
                )
            );
        }
        if outcomes
            .iter()
            .any(|o| matches!(o.status, nash_test::Status::Fail(_)))
        {
            std::process::exit(1);
        }
        Ok(())
    }
}

fn matches_filter(patterns: &[String], exact: bool, module: &str, name: &str) -> bool {
    patterns.is_empty()
        || patterns.iter().any(|pattern| {
            if let Some((wanted_module, wanted_name)) = pattern
                .split_once(".{")
                .and_then(|(m, n)| n.strip_suffix('}').map(|n| (m, n)))
            {
                if exact {
                    module == wanted_module && name == wanted_name
                } else {
                    module.contains(wanted_module) && name.contains(wanted_name)
                }
            } else if exact {
                module == pattern || name == pattern
            } else {
                module.contains(pattern) || name.contains(pattern)
            }
        })
}
