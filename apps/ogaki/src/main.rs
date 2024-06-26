use clap::Parser;
use cli::Cli;
use color_eyre::eyre::Result;
use errors::Error;
use tracing::metadata::LevelFilter;
use tracing_subscriber::EnvFilter;
use utils::DEFAULT_SEARCH_PATHS;

mod actions;
mod cli;
mod constants;
mod errors;
mod github;
mod utils;

/// Main entry point for the ogaki.
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    color_eyre::install()?;

    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()?;

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(env_filter)
        .init();

    let Err(err) = cli.run().await else {
        return Ok(());
    };

    match err {
        Error::NoCompatibleAsset => {
            eprintln!(
                "No compatible asset found for OS: {}, ARCH: {}",
                constants::OS,
                std::env::consts::ARCH
            );
        }
        Error::YuvdNotFound => {
            eprintln!("Couldn't find path to YUVd in {:?}", DEFAULT_SEARCH_PATHS,);
        }
        Error::Other(report) => {
            return Err(report);
        }
    };

    Ok(())
}
