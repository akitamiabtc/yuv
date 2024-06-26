use std::net::SocketAddr;
use std::time::Duration;

use bitcoin::Txid;
use event_bus::{typeid, EventBus};
use eyre::{Result, WrapErr};
use tokio_util::sync::CancellationToken;
use tracing::trace;

use yuv_p2p::client::handle::Handle as ClientHandle;
use yuv_storage::{InventoryStorage, TransactionsStorage, TxState, TxStatesStorage};
use yuv_types::{
    messages::p2p::Inventory, Announcement, ControllerMessage, ControllerP2PMessage,
    TxConfirmMessage, YuvTransaction, YuvTxType,
};

/// Default inventory size.
const DEFAULT_INV_SIZE: usize = 100;

/// Default inventory sharing interval in seconds.
const DEFAULT_INV_SHARE_INTERVAL: u64 = 5;

/// Controller handles Inv, GetData, YuvTx P2P methods. Selects new transactions from outside
/// and provides it to the TransactionChecker.
#[derive(Clone)]
pub struct Controller<TxsStorage, StateStorage, P2pClient>
where
    TxsStorage: TransactionsStorage + Clone,
    StateStorage: InventoryStorage + Clone,
    P2pClient: ClientHandle,
{
    /// Node's persistent storage
    txs_storage: TxsStorage,

    /// Node's storage for state values. For example, inventory.
    state_storage: StateStorage,

    /// YUV transactions that are handled right now
    handling_txs: TxStatesStorage,

    /// Event bus for simplifying communication with services
    event_bus: EventBus,

    /// Max inventory size
    max_inv_size: usize,

    /// Inventory sharing interval
    inv_sharing_interval: Duration,

    /// P2P handle which is used for sending messages to other peers
    p2p_handle: P2pClient,
}

impl<TS, SS, P2P> Controller<TS, SS, P2P>
where
    TS: TransactionsStorage + Send + Sync + Clone + 'static,
    SS: InventoryStorage + Send + Sync + Clone + 'static,
    P2P: ClientHandle + Send + Sync + Clone + 'static,
{
    pub fn new(
        full_event_bus: &EventBus,
        txs_storage: TS,
        state_storage: SS,
        txstates_storage: TxStatesStorage,
        p2p_handle: P2P,
    ) -> Self {
        let event_bus = full_event_bus
            .extract(&typeid![TxConfirmMessage], &typeid![ControllerMessage])
            .expect("event channels must be presented");

        Self {
            txs_storage,
            state_storage,
            handling_txs: txstates_storage,
            max_inv_size: DEFAULT_INV_SIZE,
            inv_sharing_interval: Duration::from_secs(DEFAULT_INV_SHARE_INTERVAL),
            event_bus,
            p2p_handle,
        }
    }

    /// Sets max inventory size.
    pub fn set_max_inv_size(mut self, max_inv_size: usize) -> Self {
        self.max_inv_size = max_inv_size;

        self
    }

    /// Sets inventory sharing interval.
    pub fn set_inv_sharing_interval(mut self, interval: Duration) -> Self {
        self.inv_sharing_interval = interval;

        self
    }

    /// Runs the Controller. It listens to the events from the event bus to handle and
    /// inventory interval timer to share inventory.
    pub async fn run(mut self, cancellation: CancellationToken) {
        let events = self.event_bus.subscribe::<ControllerMessage>();
        let mut inv_ticker = tokio::time::interval(self.inv_sharing_interval);

        loop {
            tokio::select! {
                event_received = events.recv() => {
                    let Ok(event) = event_received else {
                        trace!("All incoming event senders are dropped");
                        return;
                    };

                    if let Err(err) = self.handle_event(event).await {
                        tracing::error!("Failed to handle an event: {}", err);
                    }
                }
                _ = inv_ticker.tick() => {
                    if let Err(err) = self.share_inv().await {
                        tracing::error!("Failed to share an inventory: {}", err);
                    }
                }
                _ = cancellation.cancelled() => {
                    trace!("Cancellation received, stopping controller");
                    return;
                }
            }
        }
    }

    /// Handles new events from the event bus.
    async fn handle_event(&mut self, event: ControllerMessage) -> Result<()> {
        use ControllerMessage as Message;
        trace!("New event: {:?}", event);

        match event {
            Message::InvalidTxs { tx_ids, sender } => self
                .handle_invalid_txs(tx_ids, sender)
                .await
                .wrap_err("failed to handle invalid txs")?,
            Message::GetData { inv, receiver } => self
                .send_get_data(receiver, inv.clone())
                .await
                .wrap_err("failed to handle get yuv tx data")?,
            Message::AttachedTxs(tx_ids) => self
                .handle_attached_txs(tx_ids.clone())
                .await
                .wrap_err_with(move || {
                    format!("failed to handle attached txs; txs={:?}", tx_ids)
                })?,
            Message::P2P(p2p_event) => self
                .handle_p2p_msg(p2p_event)
                .await
                .wrap_err("failed to handle p2p event")?,
            Message::ConfirmBatchTx(txs) => self
                .handle_new_yuv_txs(txs, None)
                .await
                .wrap_err("failed to handle transaction to confirm")?,
            Message::CheckedAnnouncement(txid) => self.handle_checked_announcement(txid).await,
        }

        Ok(())
    }

    /// Handles a P2P event.
    pub async fn handle_p2p_msg(&mut self, message: ControllerP2PMessage) -> Result<()> {
        match message {
            ControllerP2PMessage::Inv { inv, sender } => self
                .handle_inv(inv, sender)
                .await
                .wrap_err("failed to handle inbound inv")?,
            ControllerP2PMessage::GetData { inv, sender } => self
                .handle_get_data(inv, sender)
                .await
                .wrap_err("failed to handle inbound get data")?,
            ControllerP2PMessage::YuvTx { txs, sender } => self
                .handle_new_yuv_txs(txs, Some(sender))
                .await
                .wrap_err("failed to handle yuv txs")?,
        };

        Ok(())
    }

    /// Handles invalid transactions. It removes them from the
    /// [`handling_txs`](Controller::handling_txs) and if the transaction was received from the
    /// network, it will send event to the network service that the sender peer is malicious.
    async fn handle_invalid_txs(
        &self,
        tx_ids: Vec<Txid>,
        malicious_peer: Option<SocketAddr>,
    ) -> Result<()> {
        self.handling_txs.remove_many(&tx_ids).await;

        if let Some(malicious_peer) = malicious_peer {
            self.p2p_handle
                .ban_peer(malicious_peer)
                .await
                .wrap_err_with(|| {
                    format!(
                        "failed to panish peer; malicious_peer={:?}; tx_ids={:?}",
                        malicious_peer, tx_ids,
                    )
                })?;
        }

        Ok(())
    }

    /// Shares inventory with the network.
    async fn share_inv(&self) -> Result<()> {
        let inv: Vec<Inventory> = self
            .state_storage
            .get_inventory()
            .await?
            .iter()
            .map(|txid| Inventory::Ytx(*txid))
            .collect();

        self.p2p_handle
            .send_inv(inv.clone())
            .await
            .wrap_err_with(|| format!("failed to share inventory; inv={:?}", inv))?;

        tracing::debug!("Inventory has been shared");

        Ok(())
    }

    /// Handles an inv message from the network. It checks if the transaction is already
    /// handled. If not, it will request the transaction from the [`Inv`] sender.
    async fn handle_inv(&mut self, inv: Vec<Inventory>, sender: SocketAddr) -> Result<()> {
        let mut missing_tx_payload = Vec::<Inventory>::default();

        for inv_msg in inv {
            match inv_msg {
                Inventory::Ytx(ytx_id) => {
                    let is_tx_exist = self
                        .is_tx_exist(&ytx_id)
                        .await
                        .wrap_err("failed to check if tx exist")?;

                    if !is_tx_exist {
                        missing_tx_payload.push(Inventory::Ytx(ytx_id));
                    }
                }
            }
        }

        if !missing_tx_payload.is_empty() {
            self.p2p_handle
                .send_get_data(missing_tx_payload, sender)
                .await
                .wrap_err("failed to send getdata message")?;
        }

        tracing::debug!("Received inv from peer: {:?}", sender);

        Ok(())
    }

    /// Handles a get data message from the network. It checks if the transaction is presented
    /// in the storage. If yes, it sends the transaction to the [`GetData`] message sender.
    async fn handle_get_data(&mut self, payload: Vec<Inventory>, sender: SocketAddr) -> Result<()> {
        let mut response_txs = Vec::<YuvTransaction>::default();

        for payload_msg in payload {
            match payload_msg {
                Inventory::Ytx(ref ytx_id) => {
                    let yuv_tx = self
                        .txs_storage
                        .get_yuv_tx(ytx_id)
                        .await
                        .wrap_err("failed to get yuv tx")?;

                    if let Some(tx) = yuv_tx {
                        response_txs.push(tx);
                    };
                }
            }
        }

        if !response_txs.is_empty() {
            self.p2p_handle
                .send_yuv_txs(response_txs, sender)
                .await
                .wrap_err("failed to send yuvtx message")?;
        }
        tracing::info!("Received get data from peer: {:?}", sender);

        Ok(())
    }

    /// Handles yuv txs from the network. It checks if the transaction is already handled. If
    /// not, it sends the transaction to the `TxChecker`.
    async fn handle_new_yuv_txs(
        &mut self,
        yuv_txs: Vec<YuvTransaction>,
        sender: Option<SocketAddr>,
    ) -> Result<()> {
        let mut new_txs = Vec::<YuvTransaction>::default();

        for yuv_tx in yuv_txs {
            let tx_id = yuv_tx.bitcoin_tx.txid();

            let is_tx_exist = self
                .is_tx_exist(&tx_id)
                .await
                .wrap_err("failed to check if tx exist")?;

            if !is_tx_exist {
                self.handling_txs
                    .insert_if_not_exists(tx_id, TxState::Pending)
                    .await;

                tracing::debug!("Added pending tx to the state storage: {}", tx_id);

                new_txs.push(yuv_tx);
                continue;
            }
            tracing::debug!("Tx {} exists in the storage", tx_id);
        }

        if !new_txs.is_empty() {
            if let Some(sender) = sender {
                tracing::debug!("Received new yuv txs from {}: {:?}", sender, new_txs);
            } else {
                tracing::debug!("Received new yuv txs: {:?}", new_txs);
            }

            self.event_bus
                .send(TxConfirmMessage::TxsToConfirm(new_txs))
                .await;
        }

        Ok(())
    }

    /// Handles attached transactions. It removes them from the handling_txs list and update
    /// inventory in [`InventoryStorage`].
    pub async fn handle_attached_txs(&mut self, txids: Vec<Txid>) -> Result<()> {
        let mut inv = self.state_storage.get_inventory().await?;

        for txid in txids {
            self.handling_txs.remove(&txid).await;

            if inv.len() > self.max_inv_size {
                inv.rotate_left(1);
                inv.insert(0, txid);
                continue;
            }

            inv.push(txid);
        }

        self.state_storage.put_inventory(inv.clone()).await?;

        tracing::info!("Inventory has been updated with checked and attached txs");

        Ok(())
    }

    /// Handles checked announcement. It removes it from the handling_txs list.
    pub async fn handle_checked_announcement(&mut self, txid: Txid) {
        self.handling_txs.remove(&txid).await;

        tracing::info!("Announcement {} is handled", txid);
    }

    pub async fn send_get_data(
        &mut self,
        receiver: SocketAddr,
        tx_ids: Vec<Inventory>,
    ) -> Result<()> {
        self.p2p_handle
            .send_get_data(tx_ids.clone(), receiver)
            .await
            .wrap_err_with(|| {
                format!(
                    "failed to send get data request; receiver={:?}; tx_ids={:?}",
                    receiver.clone(),
                    tx_ids,
                )
            })?;

        tracing::info!("Sent get data request to peer: {:?}", receiver);

        Ok(())
    }

    async fn is_tx_exist(&self, tx_id: &Txid) -> Result<bool> {
        if self.handling_txs.get(tx_id).await.is_some() {
            return Ok(true);
        }

        let yuv_tx = self
            .txs_storage
            .get_yuv_tx(tx_id)
            .await
            .wrap_err("failed to get yuv tx")?;

        if let Some(yuv_tx) = yuv_tx {
            // If the transaction exists, but it's an [IssueAnnouncement], we should still
            // mark it as non-existing so an Issue transaction can override it.
            if let YuvTxType::Announcement(Announcement::Issue { .. }) = yuv_tx.tx_type {
                return Ok(false);
            }

            return Ok(true);
        }

        Ok(false)
    }
}
