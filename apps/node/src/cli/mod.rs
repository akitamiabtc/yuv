mod actions;
mod arguments;
mod node;
use clap::Parser;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub enum Cli {
    /// Run p2p node, see `node --help` for more information
    Run(arguments::Run),
}

impl Cli {
    pub async fn exec(self) -> eyre::Result<()> {
        match self {
            Self::Run(args) => actions::run(args).await,
        }
    }
}

pub async fn run() -> eyre::Result<()> {
    Cli::parse().exec().await
}
