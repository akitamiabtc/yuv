use crate::context::Context;
use bdk::descriptor;
use bdk::descriptor::calc_checksum;
use bitcoin::PrivateKey;
use bitcoin_client::BitcoinRpcApi;
use color_eyre::eyre::{self, bail};
use ydk::bitcoin_provider::BitcoinProviderConfig;

pub async fn run(mut ctx: Context) -> eyre::Result<()> {
    let cfg = ctx.config()?;

    if let BitcoinProviderConfig::Esplora(_) = cfg.bitcoin_provider {
        bail!("The wallet abort command is not available for Esplora");
    };

    let wallet_name = get_wallet_name(cfg.private_key)?;
    let route = format!("/wallet/{}", wallet_name);
    let bitcoin_client = ctx.bitcoin_client(None, None, Some(route)).await?;

    match bitcoin_client.abort_rescan().await? {
        true => println!("Wallet scanning aborted"),
        false => println!("Nothing to abort, wallet isn't scanning"),
    }

    Ok(())
}

/// Returns wallet name for abort rescan call from private key
fn get_wallet_name(pk: PrivateKey) -> eyre::Result<String> {
    let descriptor = descriptor!(wpkh(pk))?;
    let checksum = calc_checksum(descriptor.0.to_string().as_str())?;

    Ok(checksum)
}
