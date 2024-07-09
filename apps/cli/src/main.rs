use color_eyre::eyre;

use actions::Cli;
use clap::Parser;

mod actions;
mod config;
mod context;

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    Cli::parse().run().await
}
