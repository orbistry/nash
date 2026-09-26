use miette::{IntoDiagnostic, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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
            std::io::stdin()
                .read_to_string(&mut source)
                .into_diagnostic()?;
            let formatted = nash_fmt::format(&source).map_err(|report| {
                miette::Report::new(report.render(
                    &nash_report::Source::new(&source),
                    "<stdin>",
                    color,
                ))
            })?;
            std::io::stdout()
                .write_all(formatted.as_bytes())
                .into_diagnostic()?;
            return Ok(());
        }
        let mut files = Vec::new();
        for path in self.paths {
            collect(&path.canonicalize().into_diagnostic()?, &mut files)?;
        }
        files.sort();
        files.dedup();
        let root = std::env::current_dir().into_diagnostic()?;
        let links = supports_hyperlinks::on(supports_hyperlinks::Stream::Stderr);
        let mut failed = false;
        for path in files {
            let source = std::fs::read_to_string(&path).into_diagnostic()?;
            let name = crate::reporting::source_name(&root, links, &path.to_string_lossy());
            let formatted = match nash_fmt::format(&source) {
                Ok(formatted) => formatted,
                Err(report) => {
                    eprintln!(
                        "{:?}",
                        miette::Report::new(report.render(
                            &nash_report::Source::new(&source),
                            &name,
                            color
                        ))
                    );
                    failed = true;
                    continue;
                }
            };
            if self.check {
                if let Some(report) = nash_report::format::difference(&name, &source, &formatted) {
                    eprintln!(
                        "{:?}",
                        miette::Report::new(report.render(
                            &nash_report::Source::new(&source),
                            &name,
                            color
                        ))
                    );
                    failed = true;
                }
            } else if source != formatted {
                std::fs::write(&path, formatted).into_diagnostic()?;
            }
        }
        if failed {
            std::process::exit(1);
        }
        Ok(())
    }
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).into_diagnostic()?;
    if metadata.is_file() {
        if path
            .extension()
            .is_some_and(|extension| extension == "nash")
        {
            files.push(path.canonicalize().into_diagnostic()?);
        }
    } else if metadata.is_dir() {
        for entry in std::fs::read_dir(path).into_diagnostic()? {
            let entry = entry.into_diagnostic()?;
            if matches!(
                entry.file_name().to_str(),
                Some(".git" | ".jj" | "target" | "build" | "node_modules")
            ) {
                continue;
            }
            collect(&entry.path(), files)?;
        }
    }
    Ok(())
}
