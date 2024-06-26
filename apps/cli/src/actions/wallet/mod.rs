use clap::Subcommand;
use color_eyre::eyre;

use crate::context::Context;

pub mod abort;
pub mod sync;

#[derive(Subcommand, Debug)]
pub enum WalletCommands {
    /// Aborts bitcoin wallet rescaning
    AbortRescan,
    /// Syncs yuv and bitcoin wallets  
    Sync,
}

pub async fn run(cmd: WalletCommands, context: Context) -> eyre::Result<()> {
    match cmd {
        WalletCommands::AbortRescan => abort::run(context).await,
        WalletCommands::Sync => sync::run(context).await,
    }
}
