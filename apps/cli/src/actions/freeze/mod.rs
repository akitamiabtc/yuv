use bitcoin::{OutPoint, Txid};
use clap::Args;

use color_eyre::eyre;
use yuv_types::Announcement;

use crate::{actions::announcement_args::broadcast_announcement, context::Context};

#[derive(Args, Clone, Debug)]
pub struct FreezeArgs {
    /// Transaction id
    pub txid: Txid,
    /// Output index
    pub vout: u32,
}

pub type UnfreezeArgs = FreezeArgs;

pub async fn run(args: FreezeArgs, context: Context) -> eyre::Result<()> {
    broadcast_announcement(
        Announcement::freeze_announcement(OutPoint::new(args.txid, args.vout)),
        context,
    )
    .await
}
