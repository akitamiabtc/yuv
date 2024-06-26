use eyre::Result;

mod cli;
pub(crate) mod config;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
