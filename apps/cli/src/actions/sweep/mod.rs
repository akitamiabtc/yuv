use bdk::blockchain::Blockchain;
use color_eyre::eyre;

use crate::context::Context;

pub async fn run(mut ctx: Context) -> eyre::Result<()> {
    let wallet = ctx.wallet().await?;
    let blockchain = ctx.blockchain()?;
    let cfg = ctx.config()?;

    let tx = {
        let mut builder = wallet.build_sweep()?;

        builder.set_fee_rate_strategy(cfg.fee_rate_strategy);

        builder.finish(&blockchain).await?
    };

    let Some(tx) = tx else {
        println!("Address has no tweaked Bitcoin UTXOs");
        return Ok(());
    };

    blockchain.broadcast(&tx)?;

    println!("tx id: {}", tx.txid());

    Ok(())
}
