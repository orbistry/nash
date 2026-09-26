pub mod build;
pub mod check;
pub mod format;
pub mod lsp;
pub mod test;

#[derive(clap::Subcommand)]
pub enum Cmd {
    /// Check a Nash project for errors
    #[clap(visible_alias = "c")]
    Check(check::Args),
    /// Compile validator modules to Plutus V3 scripts
    #[clap(visible_alias = "b")]
    Build(build::Args),
    /// Compile and execute module tests and properties
    #[clap(visible_alias = "t")]
    Test(test::Args),
    /// Format Nash source files
    #[clap(visible_alias = "fmt")]
    Format(format::Args),
    /// Start the Nash language server over stdio
    Lsp(lsp::Args),
}

impl Cmd {
    pub async fn exec(self, color: bool) -> miette::Result<()> {
        match self {
            Cmd::Check(args) => args.exec(color).await,
            Cmd::Build(args) => args.exec(color).await,
            Cmd::Test(args) => args.exec(color).await,
            Cmd::Format(args) => args.exec(color).await,
            Cmd::Lsp(args) => lsp::exec(args).await,
        }
    }
}
