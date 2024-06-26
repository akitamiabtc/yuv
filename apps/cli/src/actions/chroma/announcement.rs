use crate::{actions::announcement_args::broadcast_announcement, context::Context};

use clap::Args;
use color_eyre::eyre::{self};
use yuv_pixels::Chroma;
use yuv_types::Announcement;

/// Arguments to make a chroma announcement. See [`yuv_types::announcements::ChromaAnnouncement`].
#[derive(Clone, Args, Debug)]
pub struct ChromaAnnnouncementArgs {
    /// The [`Chroma`] to announce.
    #[clap(long, short, value_parser = Chroma::from_address)]
    pub chroma: Option<Chroma>,
    /// The name of the token.
    #[clap(long, short)]
    pub name: String,
    /// The symbol of the token.
    #[clap(long)]
    pub symbol: String,
    /// The decimals of the token.
    #[clap(long, short, default_value_t = 0)]
    pub decimal: u8,
    /// The maximum supply of the token. 0 - supply is unlimited.
    #[clap(long, default_value_t = 0)]
    pub max_supply: u128,
    /// Indicates whether the token can be frozen by the issuer.
    #[clap(long, default_value_t = true)]
    pub is_freezable: bool,
}

pub async fn run(args: ChromaAnnnouncementArgs, mut context: Context) -> eyre::Result<()> {
    let wallet = context.wallet().await?;
    let chroma = args
        .chroma
        .unwrap_or_else(|| Chroma::from(wallet.public_key()));

    let announcement = Announcement::chroma_announcement(
        chroma,
        args.name,
        args.symbol,
        args.decimal,
        args.max_supply,
        args.is_freezable,
    )?;

    broadcast_announcement(announcement, context).await
}
