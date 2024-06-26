mod account;
mod actions;
mod arguments;
mod e2e;
mod faucet;
mod miner;
mod tx_checker;
use clap::Parser;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub enum Cli {
    /// Run the end-to-end test
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
