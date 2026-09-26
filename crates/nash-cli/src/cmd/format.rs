use miette::{IntoDiagnostic, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::{JoinSet, spawn_blocking};

#[derive(clap::Args)]
pub struct Args {
    /// Nash files or directories to format recursively.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,
    /// Show formatting differences without changing files.
    #[arg(long)]
    pub check: bool,
    /// Format one module from stdin to stdout.
    #[arg(long, conflicts_with_all = ["paths", "check"])]
    pub stdin: bool,
}

impl Args {
    pub async fn exec(self, color: bool) -> Result<()> {
        if self.stdin {
            let mut source = String::new();
            tokio::io::stdin()
                .read_to_string(&mut source)
                .await
                .into_diagnostic()?;
            let formatted = spawn_blocking(move || format_source(&source, "<stdin>", color))
                .await
                .into_diagnostic()??;
            let mut stdout = tokio::io::stdout();
            stdout
                .write_all(formatted.as_bytes())
                .await
                .into_diagnostic()?;
            stdout.flush().await.into_diagnostic()?;
            return Ok(());
        }
        let files = collect(self.paths).await?;
        let root = std::env::current_dir().into_diagnostic()?;
        let links = supports_hyperlinks::on(supports_hyperlinks::Stream::Stderr);
        let mut tasks = JoinSet::new();
        for path in files {
            let name = crate::reporting::source_name(&root, links, &path.to_string_lossy());
            tasks.spawn(format_file(path, name, self.check, color));
        }
        let mut failed = false;
        let mut stderr = tokio::io::stderr();
        let mut output = Ok(());
        while let Some(result) = tasks.join_next().await {
            if let Err(report) = result.into_diagnostic().and_then(|result| result) {
                if output.is_ok() {
                    output = stderr
                        .write_all(format!("{report:?}\n").as_bytes())
                        .await
                        .into_diagnostic();
                }
                failed = true;
            }
        }
        output?;
        stderr.flush().await.into_diagnostic()?;
        if failed {
            std::process::exit(1);
        }
        Ok(())
    }
}

fn format_source(source: &str, name: &str, color: bool) -> Result<String> {
    nash_fmt::format(source).map_err(|report| {
        miette::Report::new(report.render(&nash_report::Source::new(source), name, color))
    })
}

async fn format_file(path: PathBuf, name: String, check: bool, color: bool) -> Result<()> {
    let source = tokio::fs::read_to_string(&path).await.into_diagnostic()?;
    let formatted = spawn_blocking(move || {
        let formatted = format_source(&source, &name, color)?;
        if check {
            if let Some(report) = nash_report::format::difference(&name, &source, &formatted) {
                return Err(miette::Report::new(report.render(
                    &nash_report::Source::new(&source),
                    &name,
                    color,
                )));
            }
            Ok(None)
        } else {
            Ok((source != formatted).then_some(formatted))
        }
    })
    .await
    .into_diagnostic()??;
    if let Some(formatted) = formatted {
        tokio::fs::write(path, formatted).await.into_diagnostic()?;
    }
    Ok(())
}

async fn collect(paths: Vec<PathBuf>) -> Result<HashSet<PathBuf>> {
    let mut pending = Vec::new();
    for path in paths {
        pending.push(tokio::fs::canonicalize(path).await.into_diagnostic()?);
    }
    let mut files = HashSet::new();
    while let Some(path) = pending.pop() {
        let metadata = tokio::fs::symlink_metadata(&path).await.into_diagnostic()?;
        if metadata.is_file() {
            if path
                .extension()
                .is_some_and(|extension| extension == "nash")
            {
                files.insert(tokio::fs::canonicalize(&path).await.into_diagnostic()?);
            }
        } else if metadata.is_dir() {
            let mut entries = tokio::fs::read_dir(&path).await.into_diagnostic()?;
            while let Some(entry) = entries.next_entry().await.into_diagnostic()? {
                if !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | ".jj" | "target" | "build" | "node_modules")
                ) {
                    pending.push(entry.path());
                }
            }
        }
    }
    Ok(files)
}
