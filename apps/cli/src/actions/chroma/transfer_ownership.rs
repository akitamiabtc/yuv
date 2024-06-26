use std::str::FromStr;

use crate::{actions::announcement_args::broadcast_announcement, context::Context};
use bitcoin::Address;
use clap::Args;
use color_eyre::eyre::{self};
use yuv_pixels::Chroma;
use yuv_types::Announcement;

/// Arguments to make a transfer ownership announcement. See [`yuv_types::announcements::TransferOwnershipAnnouncement`].
#[derive(Clone, Args, Debug)]
pub struct TransferOwnershipArgs {
    /// The chroma to transfer.
    #[clap(long, short, value_parser = Chroma::from_address)]
    pub chroma: Option<Chroma>,
    /// The address of the new owner of the chroma.
    #[clap(long, short)]
    pub new_owner: String,
}

pub async fn run(args: TransferOwnershipArgs, mut context: Context) -> eyre::Result<()> {
    let wallet = context.wallet().await?;
    let chroma = args
        .chroma
        .unwrap_or_else(|| Chroma::from(wallet.public_key()));

    let new_owner_address = Address::from_str(&args.new_owner)?.assume_checked();

    let announcement =
        Announcement::transfer_ownership_announcement(chroma, new_owner_address.script_pubkey());
    broadcast_announcement(announcement, context).await
}
