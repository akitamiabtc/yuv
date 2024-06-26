use bitcoin::OutPoint;
use bulletproof::util::ecdh;
use clap::Args;
use color_eyre::eyre::{self, bail, OptionExt};
use yuv_pixels::{generate_bulletproof, Chroma};
use yuv_rpc_api::transactions::{GetRawYuvTransactionResponse, YuvTransactionsRpcClient};

use crate::context::Context;

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Amount to check
    #[clap(long)]
    pub amount: u128,

    #[clap(long)]
    pub outpoint: OutPoint,

    /// Sender public key
    #[clap(long, value_parser = Chroma::from_address)]
    pub sender: Chroma,
}

pub async fn run(
    CheckArgs {
        amount,
        outpoint,
        sender,
    }: CheckArgs,
    mut context: Context,
) -> eyre::Result<()> {
    let config = context.config()?;
    let yuv_client = context.yuv_client()?;

    let dh_key = ecdh(config.private_key, sender.public_key(), config.network())?;

    let raw_dh_key: [u8; 32] = dh_key
        .to_bytes()
        .try_into()
        .expect("should convert to array");
    let (_, commit) = generate_bulletproof(amount, raw_dh_key);

    let yuv_tx = yuv_client.get_raw_yuv_transaction(outpoint.txid).await?;

    let GetRawYuvTransactionResponse::Attached(attached_tx) = yuv_tx else {
        bail!(
            "Transaction {txid} is not attached by YUV node",
            txid = outpoint.txid
        )
    };

    let output_proofs = attached_tx
        .tx_type
        .output_proofs()
        .ok_or_eyre("The outpoint is frozen")?;

    let proof = output_proofs
        .get(&outpoint.vout)
        .ok_or_eyre("The tx vout is not valid")?;

    let bulletproof = proof
        .get_bulletproof()
        .ok_or_eyre("The tx pixel proof is not bulletproof")?;

    if commit != bulletproof.commitment {
        return Err(eyre::eyre!("Invalid commitment"));
    }

    println!("Commit valid!");

    Ok(())
}
