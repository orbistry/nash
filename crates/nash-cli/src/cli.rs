use clap::Parser;
use std::io::IsTerminal;

use crate::cmd;

#[derive(Parser)]
#[command(name = "nash", version, about = "The Nash programming language compiler", long_about = Some(crate::BANNER))]
#[command(propagate_version = true)]
pub struct Cli {
    /// Control diagnostic colors. NO_COLOR disables automatic colors.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub color: Color,
    #[command(subcommand)]
    pub cmd: cmd::Cmd,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Color {
    Auto,
    Always,
    Never,
}

impl Default for Cli {
    fn default() -> Self {
        Self::parse()
    }
}

impl Cli {
    pub async fn exec(self) -> miette::Result<()> {
        let color = match self.color {
            Color::Always => true,
            Color::Never => false,
            Color::Auto => {
                std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
            }
        };
        miette::set_hook(Box::new(move |_| Box::new(nash_report::handler(color))))?;
        self.cmd.exec(color).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn optimization_flags() {
        for command in ["build", "test"] {
            for flag in ["-O0", "-O1", "-O2"] {
                let cli = Cli::try_parse_from(["nash", command, flag]).unwrap();
                let level = match cli.cmd {
                    crate::cmd::Cmd::Build(args) => args.optimize,
                    crate::cmd::Cmd::Test(args) => args.optimize,
                    _ => unreachable!(),
                };
                assert_eq!(u8::from(level.unwrap()), flag.as_bytes()[2] - b'0');
            }
            assert!(Cli::try_parse_from(["nash", command, "--optimize", "1"]).is_ok());
            assert!(Cli::try_parse_from(["nash", command, "-O3"]).is_err());
        }
    }

    #[test]
    fn o2_settings_merge_before_test_defaults() {
        let mut rendered = Vec::new();
        for command in ["build", "test"] {
            for args in [
                vec!["-O2"],
                vec!["-O2", "--trace-level", "silent"],
                vec!["-O2", "--trace-level", "compact"],
                vec!["-O2", "--trace-level", "verbose"],
                vec!["-O1"],
            ] {
                for (setting, trace_level, compiler_traces) in [
                    ("default", nash_config::TraceLevel::Silent, false),
                    ("verbose project", nash_config::TraceLevel::Verbose, false),
                    ("compiler traces", nash_config::TraceLevel::Silent, true),
                ] {
                    let cli = Cli::try_parse_from(
                        ["nash", command].into_iter().chain(args.iter().copied()),
                    )
                    .unwrap();
                    let build = nash_config::Build {
                        trace_level,
                        compiler_traces,
                        trace_level_explicit: setting != "default",
                        ..Default::default()
                    };
                    let resolved = match cli.cmd {
                        crate::cmd::Cmd::Build(args) => args.resolve_config(build),
                        crate::cmd::Cmd::Test(args) => args
                            .resolve_config(build)
                            .map(nash_config::Build::for_tests),
                        _ => unreachable!(),
                    };
                    rendered.push(format!(
                        "{command} {} ({setting}): {}",
                        args.join(" "),
                        match resolved {
                            Ok(config) => format!(
                                "O{} {:?}, compiler traces: {}",
                                u8::from(config.optimize),
                                config.trace_level,
                                config.compiler_traces
                            ),
                            Err(error) => error.to_string(),
                        }
                    ));
                }
            }
        }
        insta::with_settings!({omit_expression => true}, { insta::assert_snapshot!(rendered.join("\n")); });
    }

    #[test]
    fn o2_compiler_trace_cli_overrides() {
        let mut rows = Vec::new();
        for flag in ["--compiler-traces", "--compiler-traces=false"] {
            let cli = Cli::try_parse_from(["nash", "build", "-O2", flag]).unwrap();
            let crate::cmd::Cmd::Build(args) = cli.cmd else {
                unreachable!()
            };
            let result = args.resolve_config(nash_config::Build {
                compiler_traces: true,
                ..Default::default()
            });
            rows.push(format!(
                "{flag}: {}",
                match result {
                    Ok(config) => format!(
                        "O{}; compiler traces: {}",
                        u8::from(config.optimize),
                        config.compiler_traces
                    ),
                    Err(error) => error.to_string(),
                }
            ));
        }
        insta::with_settings!({omit_expression => true}, { insta::assert_snapshot!(rows.join("\n")); });
    }

    #[test]
    fn command_schema_is_consistent() {
        Cli::command().debug_assert();
    }
}
