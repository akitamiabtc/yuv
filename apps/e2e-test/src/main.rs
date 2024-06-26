mod cli;
pub(crate) mod config;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    cli::run().await
}
