use bdk::blockchain::Blockchain;
use color_eyre::eyre::{self, Context as EyreContext};
use yuv_types::Announcement;

use crate::context::Context;

/// Creates an announcement from the args and broadcasts it.
pub async fn broadcast_announcement(
    announcement: Announcement,
    mut context: Context,
) -> eyre::Result<()> {
    let blockchain = context.blockchain()?;

    let wallet = context.wallet().await?;
    let config = context.config()?;

    let yuv_tx = wallet
        .create_announcement_tx(announcement, config.fee_rate_strategy, &blockchain)
        .wrap_err("failed to create transfer ownership announcement tx")?;

    blockchain
        .broadcast(&yuv_tx.bitcoin_tx)
        .wrap_err("failed to broadcast tx")?;

    println!("Transaction broadcasted: {}", yuv_tx.bitcoin_tx.txid());

    Ok(())
}
