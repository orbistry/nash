pub mod build;
pub mod check;
pub mod lsp;

#[derive(clap::Subcommand)]
pub enum Cmd {
    /// Check a Nash project for errors
    #[clap(visible_alias = "c")]
    Check(check::Args),
    /// Compile validator modules to Plutus V3 scripts
    #[clap(visible_alias = "b")]
    Build(build::Args),
    /// Start the Nash language server over stdio
    Lsp(lsp::Args),
}

impl Cmd {
    pub async fn exec(self, color: bool) -> miette::Result<()> {
        match self {
            Cmd::Check(args) => args.exec(color).await,
            Cmd::Build(args) => args.exec(color).await,
            Cmd::Lsp(args) => lsp::exec(args).await,
        }
    }
}
