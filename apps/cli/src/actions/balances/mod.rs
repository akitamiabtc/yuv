use std::collections::HashMap;

use crate::context::Context;
use bitcoin::Network;
use color_eyre::eyre;
use yuv_pixels::Chroma;

pub async fn run(mut ctx: Context) -> eyre::Result<()> {
    let wallet = ctx.wallet().await?;
    let network = ctx.config()?.network();
    let balances = wallet.balances().await?;

    println!("YUV balances:");
    print_balances(balances.yuv, network);

    #[cfg(feature = "bulletproof")]
    {
        println!("Bulletproof balances:");
        print_balances(balances.bulletproof, network);
    }

    println!("Tweaked satoshis: {}", balances.tweaked_satoshis);

    Ok(())
}

fn print_balances(balances: HashMap<Chroma, u128>, network: Network) {
    for (chroma, balance) in balances.iter() {
        println!("{}: {}", chroma.to_address(network), balance);
    }
}
