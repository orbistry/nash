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
    fn command_schema_is_consistent() {
        Cli::command().debug_assert();
    }
}
