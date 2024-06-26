use std::collections::HashSet;

use crate::check_equal_lengths;

use bitcoin::OutPoint;
use clap::Args;
use color_eyre::eyre::{self, bail};
use ydk::txbuilder::BulletproofRecipientParameters;

use yuv_pixels::Chroma;
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;

use crate::context::Context;

#[derive(Args, Debug)]
pub struct TransferArgs {
    /// Value to transfer
    #[clap(long, num_args = 1..)]
    pub amount: Vec<u128>,

    /// Value to transfer to sender
    #[clap(long, num_args = 1..)]
    pub residual: Vec<u128>,

    #[clap(long, num_args = 1..)]
    /// Satoshis to transfer
    pub satoshis: Vec<u64>,

    #[clap(long, num_args = 1..)]
    /// satoshis to transfer to sender
    pub residual_satoshis: Vec<u64>,

    /// Type of the token, public key of the issuer.
    #[clap(long, num_args = 1..)]
    #[arg(value_parser = Chroma::from_address)]
    pub chroma: Vec<Chroma>,

    /// The public key of the receiver.
    #[clap(long, num_args = 1..)]
    #[arg(value_parser = Chroma::from_address)]
    pub recipient: Vec<Chroma>,

    /// The input tx id and vout seperated with `:` symbol. For example `dcdd...eda45:0`
    #[clap(long, num_args = 1..)]
    pub outpoint: Vec<OutPoint>,
}

pub async fn run(
    TransferArgs {
        amount,
        residual,
        satoshis,
        residual_satoshis,
        chroma,
        recipient,
        outpoint,
    }: TransferArgs,
    mut context: Context,
) -> eyre::Result<()> {
    check_equal_lengths!(
        amount,
        residual,
        satoshis,
        residual_satoshis,
        chroma,
        recipient,
        outpoint
    );

    if HashSet::<OutPoint>::from_iter(outpoint.clone()).len() != outpoint.len() {
        bail!("A bulletproof transfer cannot contain the same outpoint multiple times")
    }

    let config = context.config()?;
    let wallet = context.wallet().await?;
    let blockchain = context.blockchain()?;
    let yuv_client = context.yuv_client()?;

    let mut builder = wallet.build_transfer()?;
    // Add the input tx
    builder.manual_selected_only();
    let sender = config.private_key.public_key(context.secp_ctx()).inner;

    for i in 0..chroma.len() {
        let recipient = recipient[i].public_key();

        builder.add_recipient_with_bulletproof(
            outpoint[i],
            chroma[i],
            BulletproofRecipientParameters {
                recipient: recipient.inner,
                amount: amount[i],
                satoshis: satoshis[i],
            },
        )?;

        if residual[i] != 0 && residual_satoshis[i] != 0 {
            builder.add_recipient_with_bulletproof(
                outpoint[i],
                chroma[i],
                BulletproofRecipientParameters {
                    recipient: sender,
                    amount: residual[i],
                    satoshis: residual_satoshis[i],
                },
            )?;
        }
    }

    builder.set_fee_rate_strategy(config.fee_rate_strategy);

    let tx = builder.finish(&blockchain).await?;

    println!("{}", tx.bitcoin_tx.txid());

    yuv_client.send_raw_yuv_tx(tx, None).await?;

    Ok(())
}
