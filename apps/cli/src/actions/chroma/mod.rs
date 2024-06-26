use crate::context::Context;
use clap::Subcommand;
use color_eyre::eyre;

mod announcement;
mod info;
mod transfer_ownership;

#[derive(Subcommand, Debug)]
pub enum ChromaCommands {
    /// Make the Chroma announcement.
    Announcement(announcement::ChromaAnnnouncementArgs),
    /// Get the information about the token by its Chroma.
    Info(info::InfoArgs),
    /// Transfer ownership of the chroma to another address.
    TransferOwnership(transfer_ownership::TransferOwnershipArgs),
}

pub async fn run(cmd: ChromaCommands, context: Context) -> eyre::Result<()> {
    match cmd {
        ChromaCommands::Announcement(args) => announcement::run(args, context).await,
        ChromaCommands::Info(args) => info::run(args, context).await,
        ChromaCommands::TransferOwnership(args) => transfer_ownership::run(args, context).await,
    }
}
