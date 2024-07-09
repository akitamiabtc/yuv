use bitcoin::{OutPoint, Txid};
use clap::Args;

use color_eyre::eyre;
use yuv_pixels::Chroma;
use yuv_types::Announcement;

use crate::{actions::announcement_args::broadcast_announcement, context::Context};

#[derive(Args, Clone, Debug)]
pub struct FreezeArgs {
    /// The [`Chroma`] to freeze
    #[clap(long, short, value_parser = Chroma::from_address)]
    pub chroma: Option<Chroma>,
    /// Transaction id
    pub txid: Txid,
    /// Output index
    pub vout: u32,
}

pub async fn run(args: FreezeArgs, mut context: Context) -> eyre::Result<()> {
    let wallet = context.wallet().await?;
    let chroma = args
        .chroma
        .unwrap_or_else(|| Chroma::from(wallet.public_key()));

    broadcast_announcement(
        Announcement::freeze_announcement(chroma, OutPoint::new(args.txid, args.vout)),
        context,
    )
    .await
}
