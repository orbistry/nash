use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;
use tokio::{
    io::AsyncWriteExt,
    task::{JoinSet, spawn_blocking},
};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Format {
    Html,
    Markdown,
}

#[derive(clap::Args)]
pub struct Args {
    /// Project directory; defaults to the current directory.
    #[arg(conflicts_with = "base")]
    pub path: Option<PathBuf>,
    /// Document the compiler-bundled Base modules.
    #[arg(long)]
    pub base: bool,
    /// Documentation output format.
    #[arg(long, value_enum, default_value = "html")]
    pub format: Format,
    /// Directory for generated documentation.
    #[arg(long, default_value = "docs")]
    pub out: PathBuf,
}

impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        let path = self.path.unwrap_or_else(|| PathBuf::from("."));
        let input = if self.base {
            nash_docs::project::Input::Base
        } else {
            nash_docs::project::Input::Project(&path)
        };
        let docs = nash_docs::project::build(input).await.into_diagnostic()?;
        let success = docs.compiler.is_success();
        // Render diagnostics off the executor just like documentation pages.
        let (files, diagnostics) = spawn_blocking(move || {
            let mut diagnostics = String::new();
            for module in docs.compiler.ordered_reports() {
                let source = nash_report::Source::new(&module.source);
                for report in &module.reports {
                    diagnostics.push_str(&format!(
                        "{:?}\n",
                        miette::Report::new(report.render(&source, &module.path, color))
                    ));
                }
            }
            for (uri, state) in &docs.compiler.modules {
                if let nash_driver::ModuleResult::SourceUnavailable { message } = state {
                    diagnostics.push_str(&format!("Could not read {uri}: {message}\n"));
                }
            }
            for warning in &docs.warnings {
                let mut report = nash_report::Report::snippet(
                    "DOCUMENTATION WARNING",
                    nash_region::Region::zero(),
                    None,
                    nash_report::Doc::text(format!(
                        "{}.{}: {}",
                        warning.module, warning.name, warning.message
                    )),
                    nash_report::Doc::Empty,
                )
                .warning();
                report.primary_label = None;
                report.context = None;
                diagnostics.push_str(&format!(
                    "{:?}\n",
                    miette::Report::new(report.render(
                        &nash_report::Source::new(""),
                        &warning.module,
                        color
                    ))
                ));
            }
            let format = match self.format {
                Format::Html => nash_docs::Format::Html,
                Format::Markdown => nash_docs::Format::Markdown,
            };
            (
                if success {
                    nash_docs::render(&docs.modules, format)
                } else {
                    Default::default()
                },
                diagnostics,
            )
        })
        .await
        .into_diagnostic()?;
        let mut stderr = tokio::io::stderr();
        stderr
            .write_all(diagnostics.as_bytes())
            .await
            .into_diagnostic()?;
        stderr.flush().await.into_diagnostic()?;
        if !success {
            return Err(miette::miette!(
                "Documentation generation stopped because compilation failed"
            ));
        }
        let mut writes = JoinSet::new();
        for (relative, contents) in files {
            let path = self.out.join(relative);
            writes.spawn(async move {
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(path, contents).await
            });
        }
        let mut result = Ok(());
        while let Some(write) = writes.join_next().await {
            if let Err(error) = write
                .into_diagnostic()
                .and_then(|write| write.into_diagnostic())
            {
                result = Err(error);
            }
        }
        result
    }
}
