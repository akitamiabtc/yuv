use std::{
    collections::HashMap,
    fs::File,
    ops::{AddAssign, SubAssign},
    str::FromStr,
    time::{Duration, SystemTime},
};

use bdk::{
    bitcoincore_rpc::RpcApi,
    blockchain::{GetHeight, RpcBlockchain},
};
use bitcoin::{
    address::NetworkChecked,
    secp256k1::{All, PublicKey, Secp256k1},
    Address, PrivateKey, Txid,
};
use chrono::{DateTime, Utc};
use csv::Writer;
use eyre::bail;
use jsonrpsee::http_client::HttpClient;
use once_cell::sync::Lazy;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument, warn, Level};
use yuv_pixels::Chroma;
use yuv_rpc_api::transactions::{YuvTransactionStatus, YuvTransactionsRpcClient};
use yuv_types::{ProofMap, YuvTransaction};

use crate::{cli::e2e::NETWORK, config::TestConfig};

/// A dummy address that is used to generate blocks.
static ADDRESS: Lazy<Address<NetworkChecked>> = Lazy::new(|| {
    Address::from_str("bcrt1p7re7k8hwapgh4l9a2hx39u8t8ltnvn93tfcqnm02e2qzzjpnmqwq4rk0ya")
        .unwrap()
        .assume_checked()
});
const BITCOIN_NODE_ERROR_SLEEP_DURATION: Duration = Duration::from_secs(30);
const DEFAULT_RESULT_PATH: &str = "result.dev.csv";

type Amounts = HashMap<PublicKey, HashMap<Chroma, u128>>;

#[derive(Debug)]
pub(crate) struct TxChecker {
    config: TestConfig,
    txs_state: HashMap<Txid, u32>,
    amounts: Amounts,
    yuv_client: HttpClient,
    rpc_blockchain: RpcBlockchain,
    total_txs_attached: u32,
    total_txs_failed: u32,
}

impl TxChecker {
    pub fn new(config: TestConfig, yuv_client: HttpClient, rpc_blockchain: RpcBlockchain) -> Self {
        Self {
            config,
            txs_state: HashMap::new(),
            amounts: HashMap::new(),
            yuv_client,
            rpc_blockchain,
            total_txs_attached: 0,
            total_txs_failed: 0,
        }
    }

    /// Run the `tx-checker` that will check the incoming transactions.
    ///
    /// The checker's jobs are:
    /// - Count attached and failed transactions and save the results to a CSV file.
    /// - Count the expected balances for the addresses.
    /// - Check if the actual and expected balances are the same.
    pub async fn run(
        mut self,
        cancellation_token: CancellationToken,
        mut tx_receiver: UnboundedReceiver<YuvTransaction>,
        mut balance_receiver: UnboundedReceiver<(PrivateKey, HashMap<Chroma, u128>)>,
    ) -> eyre::Result<()> {
        info!("Waiting for transactions");

        // Init the CSV writer.
        let writer_result = Writer::from_path(&self.config.report.result_path);
        let mut writer = writer_result.unwrap_or(Writer::from_path(DEFAULT_RESULT_PATH).unwrap());

        // Write the header of the table.
        writer.write_record([
            "Timestamp",
            "Height",
            "Attached",
            "Pending",
            "Failed",
            "Total attached",
            "Total failed",
        ])?;

        loop {
            tokio::select! {
                // If a tx received, add it to the pending transactions list and check its validity.
                tx = tx_receiver.recv() => {
                    if let Some(tx) = tx {
                        self.txs_state.insert(tx.bitcoin_tx.txid(), 0);

                        if (self.txs_state.len() as u64) < self.config.checker.threshold {
                            continue;
                        }

                        if let Err(err) = self.check_txs(&mut writer).await {
                            error!("Failed to check the transactions: {}", err);
                        };
                    }
                }
                // If a cancellation received, check all remaning transactions and retrieve actual balances
                // from the accounts to check if they match with the expected balances.
                _ = cancellation_token.cancelled() => {
                    info!("Cancellation received, performing the final check");

                    self.rpc_blockchain
                        .generate_to_address(5, &ADDRESS)?;

                    let mut balances = Vec::new();
                    while let Some(balance) = balance_receiver.recv().await {
                        balances.push(balance);
                    }

                    // Check all the remaining pending transactions.
                    while !self.txs_state.is_empty() {
                        if let Err(err) = self.check_txs(&mut writer).await {
                            error!("Failed to check the transactions: {}", err);
                        };
                    }

                    info!("The final check is over");
                    writer.flush()?;

                    // Check if balances match.
                    if self.config.checker.check_balances_matching {
                        info!("Checking balances matching");
                        let secp = Secp256k1::new();
                        for (private_key, balance) in balances {
                            self.check_balance(private_key, balance, &secp);
                        }
                    }

                    return Ok(());
                }
            }
        }
    }

    /// Check if the pending transactions are attached.
    async fn check_txs(&mut self, csv_writer: &mut Writer<File>) -> eyre::Result<()> {
        self.rpc_blockchain.generate_to_address(1, &ADDRESS)?;

        info!(
            "Generated blocks | Checking {} transactions",
            self.txs_state.len()
        );

        let (mut attached, mut pending, mut failed) = (0, 0, 0);

        for (txid, confirmations) in self.txs_state.clone().iter() {
            let tx_response = match self.yuv_client.get_yuv_transaction(*txid).await {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Rate limit error: {}", e);
                    tokio::time::sleep(BITCOIN_NODE_ERROR_SLEEP_DURATION).await;
                    self.yuv_client.get_yuv_transaction(*txid).await?
                }
            };

            // If the transaction is attached, remove it from the pending transactions list
            // and update the expected balances.
            //
            // If it's invalid, just remove it from the pending transactions list.
            //
            // If it's neither attached nor invalid, just increase its number of confirmations.
            // NOTE: the tx is considered invalid if it has many confirmations and is still not attached.
            if tx_response.status == YuvTransactionStatus::Attached {
                let Some(attached_tx) = tx_response.data else {
                    bail!("Tx {:?} is missing in the storage", txid);
                };
                info!("Tx {} is attached", txid);
                attached += 1;
                self.txs_state.remove(txid);

                self.update_amount(attached_tx.into())?;
            } else if *confirmations > 2 {
                failed += 1;
                error!("Tx {} is invalid", txid);
                self.txs_state.remove(txid);
            } else {
                pending += 1;
                info!("Tx {} has not reached enough confirmations yet", txid);
                self.txs_state.entry(*txid).and_modify(|entry| {
                    *entry += 1;
                });
            }
        }

        self.total_txs_attached += attached;
        self.total_txs_failed += failed;

        info!(
            "Iteration has finished with {} attached, {} pending and {} failed transactions",
            attached, pending, failed
        );

        info!(
            "Total TXs attached: {} | Total TXs failed: {}",
            self.total_txs_attached, self.total_txs_failed
        );

        let system_time = SystemTime::now();
        let datetime: DateTime<Utc> = system_time.into();

        csv_writer.write_record(&[
            datetime.format("%d/%m/%Y %T").to_string(),
            self.rpc_blockchain.get_height()?.to_string(),
            attached.to_string(),
            pending.to_string(),
            failed.to_string(),
            self.total_txs_attached.to_string(),
            self.total_txs_failed.to_string(),
        ])?;

        // Prevents syncing errors.
        tokio::time::sleep(Duration::from_secs(5)).await;

        Ok(())
    }

    /// `update_amount` updates expected balances using the passed YUV transaction.
    ///
    /// It decreases the expected balance for the input proofs of the transaction
    /// and increases it for the output proofs.
    fn update_amount(&mut self, tx: YuvTransaction) -> eyre::Result<()> {
        match &tx.tx_type {
            yuv_types::YuvTxType::Issue { output_proofs, .. } => {
                self.handle_proofs(
                    &output_proofs.clone().expect("Outputs should be present"),
                    u128::add_assign,
                );
            }
            yuv_types::YuvTxType::Transfer {
                input_proofs,
                output_proofs,
            } => {
                self.handle_proofs(input_proofs, u128::sub_assign);
                self.handle_proofs(output_proofs, u128::add_assign);
            }
            _ => return Ok(()),
        };

        Ok(())
    }

    /// `handle_proofs` changes the expected balances using the passed operation
    /// which is supposed to be either `AddAssign` or `SubAssign`.
    fn handle_proofs<F: Fn(&mut u128, u128)>(&mut self, pixel_proofs: &ProofMap, op: F) {
        for pixel_proof in pixel_proofs.values() {
            let (recipient, pixel) = match pixel_proof {
                yuv_pixels::PixelProof::Sig(proof) => (proof.inner_key, proof.pixel),
                _ => return,
            };

            let balances = self.amounts.entry(recipient).or_default();
            let balance = balances.entry(pixel.chroma).or_default();

            op(balance, pixel.luma.amount);

            if *balance == 0 {
                balances.remove(&pixel.chroma);
            }
        }
    }

    /// `check_balance` checks if the actual balances match the expected balances for a certain address.
    #[instrument(level = Level::INFO,
        name = "balance_checker",
        fields(private_key = private_key.to_string()),
        skip(self, balances, secp)
    )]
    fn check_balance(
        &self,
        private_key: PrivateKey,
        balances: HashMap<Chroma, u128>,
        secp: &Secp256k1<All>,
    ) {
        let pubkey = PublicKey::from_secret_key(secp, &private_key.inner);
        let actual_balances = balances;
        let mut do_balances_match = true;

        let Some(expected_balances) = self.amounts.get(&pubkey) else {
            warn!("Expected balances not found");
            return;
        };

        let mut mismatches = 1;
        for (chroma, actual_amount) in actual_balances.iter() {
            if let Some(expected_amount) = expected_balances.get(chroma) {
                if actual_amount == expected_amount {
                    continue;
                }

                do_balances_match = false;
                error!(
                    r#"
                        {} balance mismatch found
                        Chroma: {}
                        Actual balance: {} | Expected balance: {}"#,
                    mismatches,
                    chroma.to_address(NETWORK),
                    actual_amount,
                    expected_amount
                );
                mismatches += 1;
            } else {
                do_balances_match = false;
                error!("Chroma {} not found", chroma.to_address(NETWORK));
            };
        }

        if do_balances_match {
            info!("Balances match");
        }
    }
}
