use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug, Clone)]
pub struct Run {
    /// Path to config file
    #[clap(short, long, default_value = "config.toml")]
    pub config: PathBuf,
}
