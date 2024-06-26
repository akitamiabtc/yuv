use std::sync::Arc;

use bdk::blockchain::GetTx;
use bitcoin::Txid;
use clap::Args;
use color_eyre::eyre;
use ydk::txbuilder::form_issue_announcement;
use yuv_tx_check::{check_transaction, CheckError};
use yuv_types::{ProofMap, TransferProofs, YuvTransaction, YuvTxType};

use crate::context::Context;

use super::ProofListArgs;

#[derive(Args, Debug)]
pub struct CheckFetchArgs {
    /// Transaction hash.
    #[clap(long, short)]
    pub txid: Txid,
}

pub(crate) async fn run(
    proofs: ProofListArgs,
    CheckFetchArgs { txid }: CheckFetchArgs,
    mut context: Context,
) -> eyre::Result<()> {
    let blockchain = context.blockchain()?;

    let TransferProofs {
        input: input_proofs_map,
        output: output_proofs_map,
    } = proofs.into_proof_maps()?;

    log::debug!("Input proofs: {:?}", input_proofs_map);
    log::debug!("Output proofs: {:?}", output_proofs_map);

    check_p2wpkh_tx_by_id(blockchain, &txid, input_proofs_map, output_proofs_map).await?;

    println!("Transaction is valid!");

    Ok(())
}

pub async fn check_p2wpkh_tx_by_id(
    bitcoin_provider: Arc<bdk::blockchain::AnyBlockchain>,
    tx_id: &Txid,
    inputs: ProofMap,
    outputs: ProofMap,
) -> eyre::Result<()> {
    // Check that transaction exists at all
    let Some(tx) = bitcoin_provider.get_tx(tx_id)? else {
        return Err(CheckError::TxNotFound(*tx_id).into());
    };

    let yuv_tx_type = match inputs.is_empty() {
        true => YuvTxType::Issue {
            output_proofs: Some(outputs.clone()),
            announcement: form_issue_announcement(outputs.into_values().collect())?,
        },
        false => YuvTxType::Transfer {
            input_proofs: inputs,
            output_proofs: outputs,
        },
    };

    check_transaction(&YuvTransaction {
        bitcoin_tx: tx,
        tx_type: yuv_tx_type,
    })?;

    Ok(())
}
