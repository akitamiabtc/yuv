use std::collections::HashMap;

use bdk::miniscript::ToPublicKey;
use bitcoin::{OutPoint, PublicKey};
use eyre::Context;
use yuv_pixels::PixelProof;
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;
use yuv_storage::{PagesNumberStorage, TransactionsStorage};
use yuv_types::YuvTransaction;

use super::storage::UnspentYuvOutPointsStorage;

/// Indexer of YUV transactions got from YUV node.
pub struct YuvTransactionsIndexer<YuvRpcClient, TransactionStorage> {
    /// Fetcher of transactions to YUV node.
    node_client: YuvRpcClient,

    /// Storage for YUV transactions
    txs_storage: TransactionStorage,

    /// A map of out points of unspent transactions, if the value is true, then
    /// the transaction is unspent,
    indexed_txs: HashMap<OutPoint, bool>,

    /// Out points of current user
    user_outpoints: HashMap<OutPoint, PixelProof>,

    /// Public key of the user we are searching UTXOs
    pubkey: PublicKey,

    /// Last indexed page number.
    last_page_number: u64,
}

impl<C, TS> YuvTransactionsIndexer<C, TS>
where
    C: YuvTransactionsRpcClient + Send + Sync + 'static,
    TS: TransactionsStorage
        + PagesNumberStorage
        + UnspentYuvOutPointsStorage
        + Send
        + Sync
        + 'static,
{
    pub fn new(client: C, txs_storage: TS, pubkey: PublicKey) -> Self {
        Self {
            node_client: client,
            txs_storage,
            indexed_txs: HashMap::new(),
            last_page_number: 0,
            user_outpoints: HashMap::default(),
            pubkey,
        }
    }

    pub async fn sync(mut self) -> eyre::Result<Vec<(OutPoint, PixelProof)>> {
        self.last_page_number = self
            .txs_storage
            .get_pages_number()
            .await?
            .unwrap_or(0)
            .saturating_sub(1);

        self.user_outpoints = self.txs_storage.get_unspent_yuv_outpoints().await?;

        loop {
            let txs = self
                .node_client
                .list_yuv_transactions(self.last_page_number)
                .await
                .wrap_err("Failed to fetch transactions from node")?;

            tracing::debug!("Got transactions: {:?}", txs);

            if txs.is_empty() {
                break;
            }

            self.last_page_number += 1;

            for tx in txs {
                let yuv_tx = tx.into();
                self.index_transaction(&yuv_tx);

                self.txs_storage
                    .put_yuv_tx(yuv_tx)
                    .await
                    .wrap_err("Failed to insert transaction")?;
            }

            self.txs_storage
                .put_pages_number(self.last_page_number)
                .await?;
        }

        let utxos = self.cleanup().await?;

        self.txs_storage
            .put_unspent_yuv_outpoints(self.user_outpoints.clone())
            .await?;

        Ok(utxos)
    }

    /// Go through all outputs of current transactions and add them
    /// as indexed, then go through all inputs and mark outputs as spend.
    fn index_transaction(&mut self, tx: &YuvTransaction) {
        let txid = tx.bitcoin_tx.txid();
        let outpoints = tx
            .bitcoin_tx
            .output
            .iter()
            .enumerate()
            .map(|(index, _)| OutPoint::new(txid, index as u32))
            .collect::<Vec<_>>();

        // Skip freeze transactions that has no outputs
        let Some(output_proofs) = tx.tx_type.output_proofs() else {
            return;
        };

        let (self_x_only_pubkey, _parity) = self.pubkey.inner.x_only_public_key();

        for outpoint in outpoints {
            let Some(output_proof) = output_proofs.get(&outpoint.vout) else {
                continue;
            };

            match output_proof {
                PixelProof::Sig(proof) => {
                    let (proof_x_key, _parity) = proof.inner_key.x_only_public_key();

                    if proof_x_key == self_x_only_pubkey {
                        self.user_outpoints.insert(outpoint, output_proof.clone());
                    }
                }
                PixelProof::Multisig(proof) => {
                    let x_only_pubkeys = proof
                        .inner_keys
                        .iter()
                        .map(|key| key.x_only_public_key().0)
                        .collect::<Vec<_>>();

                    if x_only_pubkeys.contains(&self_x_only_pubkey) {
                        self.user_outpoints.insert(outpoint, output_proof.clone());
                    }
                }
                PixelProof::Lightning(proof) => {
                    let x_only = proof.local_delayed_pubkey.x_only_public_key().0;

                    if x_only == self.pubkey.inner.x_only_public_key().0 {
                        tracing::debug!("Adding lightning output proof: {:?}", output_proof);

                        self.user_outpoints.insert(outpoint, output_proof.clone());
                    }
                }
                #[cfg(feature = "bulletproof")]
                PixelProof::Bulletproof(proof) => {
                    let (proof_x_key, _parity) = proof.inner_key.x_only_public_key();

                    if proof_x_key == self_x_only_pubkey {
                        self.user_outpoints.insert(outpoint, output_proof.clone());
                    }
                }
                PixelProof::LightningHtlc(htlc_proof) => {
                    // NOTE: Lightning HTLC is spend only by LDK node.
                    let used_keys = [
                        htlc_proof.data.remote_htlc_key.to_x_only_pubkey(),
                        htlc_proof.data.local_htlc_key.to_x_only_pubkey(),
                    ];

                    if used_keys.contains(&self_x_only_pubkey) {
                        tracing::debug!("Adding lightning htlc output proof: {:?}", output_proof);

                        self.user_outpoints.insert(outpoint, output_proof.clone());
                    }
                }
                PixelProof::EmptyPixel(proof) => {
                    let (proof_x_only_pubkey, _parity) = proof.inner_key.x_only_public_key();

                    if proof_x_only_pubkey == self_x_only_pubkey {
                        self.user_outpoints.insert(outpoint, output_proof.clone());
                    }
                }
            }

            self.indexed_txs.entry(outpoint).or_insert(false);
        }

        for input in &tx.bitcoin_tx.input {
            self.indexed_txs.insert(input.previous_output, true);
        }
    }

    /// Clean up transaction that are spent, and not owned by user
    async fn cleanup(&mut self) -> eyre::Result<Vec<(OutPoint, PixelProof)>> {
        let mut utxos = Vec::new();

        for (outpoint, is_spent) in &self.indexed_txs {
            if *is_spent {
                // FIXME:
                // self.txs_storage.delete_yuv_tx(outpoint.txid).await?;
                continue;
            }

            let is_outpoint_frozen = self
                .node_client
                .is_yuv_txout_frozen(outpoint.txid, outpoint.vout)
                .await?;

            if is_outpoint_frozen {
                self.txs_storage.delete_yuv_tx(&outpoint.txid).await?;
                continue;
            }

            let Some(proof) = self.user_outpoints.get(outpoint) else {
                continue;
            };

            utxos.push((*outpoint, proof.clone()));
        }

        Ok(utxos)
    }
}
