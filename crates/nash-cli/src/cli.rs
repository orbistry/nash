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
            for flag in ["-O0", "-O1"] {
                let cli = Cli::try_parse_from(["nash", command, flag]).unwrap();
                let level = match cli.cmd {
                    crate::cmd::Cmd::Build(args) => args.optimize,
                    crate::cmd::Cmd::Test(args) => args.optimize,
                    _ => unreachable!(),
                };
                assert_eq!(u8::from(level.unwrap()), flag.as_bytes()[2] - b'0');
            }
            assert!(Cli::try_parse_from(["nash", command, "--optimize", "1"]).is_ok());
            assert!(Cli::try_parse_from(["nash", command, "-O2"]).is_err());
        }
    }

    #[test]
    fn command_schema_is_consistent() {
        Cli::command().debug_assert();
    }
}
