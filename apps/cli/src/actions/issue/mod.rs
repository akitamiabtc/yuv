use bdk::blockchain::Blockchain;
use clap::Args;
use color_eyre::eyre::{self, bail};
use yuv_pixels::Chroma;
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;

use crate::{actions::transfer::process_satoshis, context::Context};

pub const DEFAULT_SATOSHIS: u64 = 1000;

#[derive(Args, Debug)]
pub struct IssueArgs {
    /// [Chroma] of the token to issue.
    ///
    /// If not specified, the [Chroma] will be the same as the X only key derived from the
    /// private key.
    #[clap(long, value_parser = Chroma::from_address)]
    pub chroma: Option<Chroma>,
    /// Amount in satoshis that will be added to YUV UTXO.
    ///
    /// Default is 10,000 satoshis, if only one amount is provided it will be
    /// used for all recipients.
    #[clap(long, short, num_args = 1.., default_values_t = vec![DEFAULT_SATOSHIS])]
    pub satoshis: Vec<u64>,
    /// YUV token amount
    #[clap(long = "amount", num_args = 1..)]
    pub amounts: Vec<u128>,
    /// Public key of the recipient.
    #[clap(long = "recipient", num_args = 1.., value_parser = Chroma::from_address)]
    pub recipients: Vec<Chroma>,
    /// Provide proof of the transaction to YUV node.
    #[clap(long)]
    pub do_not_provide_proofs: bool,
    /// Drain tweaked satoshis to use for fees, instead of using regular satoshis.
    ///
    /// It's worth noting that change from regular satoshis will be tweaked.
    #[clap(long)]
    pub drain_tweaked_satoshis: bool,
}

pub async fn run(
    IssueArgs {
        amounts,
        recipients,
        satoshis,
        do_not_provide_proofs,
        drain_tweaked_satoshis,
        chroma,
    }: IssueArgs,
    mut ctx: Context,
) -> eyre::Result<()> {
    if amounts.len() != recipients.len() {
        bail!("Amounts and recipients must have the same length");
    }

    let satoshis = process_satoshis(satoshis, amounts.len())?;

    let wallet = ctx.wallet().await?;
    let blockchain = ctx.blockchain()?;
    let cfg = ctx.config()?;

    let tx = {
        let mut builder = wallet.build_issuance(chroma)?;

        for ((recipient, amount), satoshis) in recipients.iter().zip(amounts).zip(satoshis) {
            builder.add_recipient(&recipient.public_key().inner, amount, satoshis);
        }

        builder
            .set_fee_rate_strategy(cfg.fee_rate_strategy)
            .set_drain_tweaked_satoshis(drain_tweaked_satoshis);

        builder.finish(&blockchain).await?
    };

    let tx_type = tx.tx_type.clone();
    blockchain.broadcast(&tx.bitcoin_tx)?;
    if !do_not_provide_proofs {
        let client = ctx.yuv_client()?;

        client.provide_yuv_proof(tx.clone()).await?;
    }

    println!("tx id: {}", tx.bitcoin_tx.txid());
    println!("{}", serde_yaml::to_string(&tx_type)?);

    Ok(())
}
