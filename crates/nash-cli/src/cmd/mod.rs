pub mod build;
pub mod check;
pub mod docs;
pub mod format;
pub mod lsp;
pub mod proof;
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
    /// Verify proof properties with Lean-blaster and compiled UPLC semantics
    #[clap(visible_alias = "verify")]
    Proof(proof::Args),
    /// Format Nash source files
    #[clap(visible_alias = "fmt")]
    Format(format::Args),
    /// Generate public API documentation
    Docs(docs::Args),
    /// Start the Nash language server over stdio
    Lsp(lsp::Args),
}

impl Cmd {
    pub async fn exec(self, color: bool) -> miette::Result<()> {
        match self {
            Cmd::Docs(args) => args.exec(color).await,
            Cmd::Check(args) => args.exec(color).await,
            Cmd::Build(args) => args.exec(color).await,
            Cmd::Test(args) => args.exec(color).await,
            Cmd::Proof(args) => args.exec(color).await,
            Cmd::Format(args) => args.exec(color).await,
            Cmd::Lsp(args) => lsp::exec(args).await,
        }
    }
}
