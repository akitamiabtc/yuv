use bitcoin::Txid;
use bitcoin_client::BitcoinRpcApi;
use event_bus::{typeid, EventBus};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio_util::sync::CancellationToken;
use yuv_types::{TxCheckerMessage, TxConfirmMessage, YuvTransaction, DEFAULT_CONFIRMATIONS_NUMBER};

/// `TxConfirmator` is responsible for waiting confirmations of transactions in Bitcoin.
pub struct TxConfirmator<BC>
where
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    event_bus: EventBus,
    bitcoin_client: Arc<BC>,
    /// Confirmations queue. Contains transactions that are waiting confirmation.
    queue: HashMap<Txid, UnconfirmedTransaction>,
    /// Max time that transaction can wait confirmation before it will be removed from the queue.
    max_confirmation_time: Duration,
    /// Interval between waiting txs clean up.
    clean_up_interval: Duration,
    /// Contains the number of confirmations required to consider a transaction as confirmed.
    confirmations_number: u8,
}

impl<BC> TxConfirmator<BC>
where
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    pub fn new(
        event_bus: &EventBus,
        bitcoin_client: Arc<BC>,
        max_confirmation_time: Duration,
        clean_up_interval: Duration,
        confirmations_number: Option<u8>,
    ) -> Self {
        let event_bus = event_bus
            .extract(&typeid![TxCheckerMessage], &typeid![TxConfirmMessage])
            .expect("event channels must be presented");

        let confirmations_number = confirmations_number.unwrap_or(DEFAULT_CONFIRMATIONS_NUMBER);

        Self {
            event_bus,
            queue: Default::default(),
            max_confirmation_time,
            bitcoin_client,
            clean_up_interval,
            confirmations_number,
        }
    }

    pub async fn run(mut self, cancellation_token: CancellationToken) {
        let mut timer = tokio::time::interval(self.clean_up_interval);
        let events = self.event_bus.subscribe::<TxConfirmMessage>();

        loop {
            tokio::select! {
                event_received = events.recv() => {
                    let Ok(event) = event_received else {
                        tracing::trace!("All incoming events senders are dropped");
                        return;
                    };

                    if let Err(err) = self.handle_event(event).await {
                        tracing::error!("failed to handle event: {:#}", err);
                    };
                },
                _ = timer.tick() => {
                    if let Err(err) = self.clean_up_waiting_txs().await {
                        tracing::error!("failed to handle waiting transactions: {:#}", err);
                    };
                },
                _ = cancellation_token.cancelled() => {
                    tracing::trace!("cancellation received, stopping confirmator");
                    return;
                }
            }
        }
    }

    async fn handle_event(&mut self, event: TxConfirmMessage) -> eyre::Result<()> {
        match event {
            TxConfirmMessage::TxsToConfirm(yuv_txs) => {
                for yuv_tx in yuv_txs {
                    self.handle_tx_to_confirm(yuv_tx).await?;
                }
            }
            TxConfirmMessage::ConfirmedTxIds(tx_ids) => {
                // Find the transactions that are waiting confirmation in the queue to confirm them.
                let yuv_txs: Vec<YuvTransaction> = tx_ids
                    .iter()
                    .filter_map(|tx_id| {
                        self.queue
                            .get(tx_id)
                            .map(|unconfirmed_tx| unconfirmed_tx.yuv_tx.clone())
                    })
                    .collect::<Vec<YuvTransaction>>();

                for yuv_tx in yuv_txs {
                    self.new_confirmed_tx(yuv_tx).await;
                }
            }
        }

        Ok(())
    }

    /// Handle new transaction to confirm it. If transaction is already confirmed, then it will be
    /// sent to the `TxChecker`. Otherwise it will be added to the queue.
    async fn handle_tx_to_confirm(&mut self, yuv_tx: YuvTransaction) -> eyre::Result<()> {
        let got_tx = self
            .bitcoin_client
            .get_raw_transaction_info(&yuv_tx.bitcoin_tx.txid(), None)
            .await?;

        if let Some(confirmations) = got_tx.confirmations {
            if confirmations >= self.confirmations_number as u32 {
                self.new_confirmed_tx(yuv_tx).await;
                return Ok(());
            }
        }

        tracing::debug!(
            "Transaction {} is waiting for enough confirmations",
            yuv_tx.bitcoin_tx.txid()
        );

        self.queue
            .entry(yuv_tx.bitcoin_tx.txid())
            .or_insert(UnconfirmedTransaction {
                yuv_tx,
                created_at: SystemTime::now(),
            });

        Ok(())
    }

    /// Find transactions that are waiting confirmation in the block. If transaction is appeared in
    /// the block, then it is confirmed and can be sent to the checkers. Otherwise it will be
    /// removed from the queue if it is waiting confirmation for too long.
    pub async fn clean_up_waiting_txs(&mut self) -> eyre::Result<()> {
        if self.queue.is_empty() {
            return Ok(());
        }

        // Remove transactions that are waiting confirmation for too long.
        for (txid, unconfirmed_tx) in self.queue.clone().into_iter() {
            if unconfirmed_tx.created_at.elapsed().unwrap() > self.max_confirmation_time {
                tracing::debug!(
                    "Transaction {:?} is waiting confirmation for too long. Removing from queue.",
                    txid
                );

                self.queue.remove(&txid);
            } else {
                self.handle_tx_to_confirm(unconfirmed_tx.yuv_tx).await?;
            }
        }

        Ok(())
    }

    async fn new_confirmed_tx(&mut self, yuv_tx: YuvTransaction) {
        tracing::debug!("Transaction confirmed: {:?}", yuv_tx.bitcoin_tx.txid());
        self.queue.remove(&yuv_tx.bitcoin_tx.txid());

        self.event_bus
            .send(TxCheckerMessage::NewTxs {
                txs: vec![yuv_tx],
                sender: None,
            })
            .await;
    }
}

/// Transaction that is waiting confirmation. Contains timestamp of creation and transaction itself.
/// Timestamp is used to check that the transaction has been waiting for confirmation for too long (considering
/// several days) and should be removed from the queue.
#[derive(Clone)]
struct UnconfirmedTransaction {
    pub created_at: SystemTime,
    pub yuv_tx: YuvTransaction,
}
