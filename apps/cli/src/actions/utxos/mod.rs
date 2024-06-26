use bitcoin::{Network, OutPoint};
use clap::Args;
use color_eyre::eyre;
use ydk::wallet::StorageWallet;
use yuv_pixels::Chroma;

use crate::context::Context;

#[derive(Args, Debug)]
pub struct UtxosArgs {
    /// Chroma of the token
    #[clap(long, value_parser = Chroma::from_address)]
    pub chroma: Option<Chroma>,
}

pub async fn run(UtxosArgs { chroma }: UtxosArgs, mut ctx: Context) -> eyre::Result<()> {
    let wallet = ctx.wallet().await?;

    match chroma {
        Some(chroma) => {
            show_utxos_by_chroma(&wallet, chroma);
        }
        None => {
            show_all_utxos(&wallet, ctx.config()?.network());
        }
    }

    Ok(())
}

fn show_all_utxos(wallet: &StorageWallet, network: Network) {
    let utxos = wallet.yuv_utxos();

    for (OutPoint { txid, vout }, proof) in utxos {
        let pixel = proof.pixel();

        println!(
            "{txid}:{vout:0>2} {chroma} {amount}",
            chroma = pixel.chroma.to_address(network),
            amount = pixel.luma.amount
        );
    }
}

fn show_utxos_by_chroma(wallet: &StorageWallet, chroma: Chroma) {
    let utxos = wallet.utxos_by_chroma(chroma);

    for (OutPoint { txid, vout }, amount) in utxos {
        println!("{}:{} {}", txid, vout, amount);
    }
}
