use std::collections::HashSet;
use std::time::Duration;
use std::{collections::VecDeque, net::SocketAddr};

use bitcoin::hashes::Hash;
use bitcoin::Txid;
use event_bus::{typeid, EventBus};
use eyre::{ContextCompat, Result, WrapErr};
use tokio_util::sync::CancellationToken;
use tracing::trace;

use yuv_p2p::client::handle::Handle as ClientHandle;
use yuv_storage::{
    InventoryStorage, MempoolEntryStorage, MempoolStatus, MempoolStorage, MempoolTxEntry,
    PagesNumberStorage, PagesStorage, TransactionsStorage,
};
use yuv_types::{
    messages::p2p::Inventory, ControllerMessage, ControllerP2PMessage, TxConfirmMessage,
    YuvTransaction, YuvTxType,
};
use yuv_types::{Announcement, GraphBuilderMessage, IndexerMessage, TxCheckerMessage};

/// Default inventory size.
const DEFAULT_INV_SIZE: usize = 100;

/// Default inventory sharing interval in seconds.
const DEFAULT_INV_SHARE_INTERVAL: Duration = Duration::from_secs(5);

/// Controller handles Inv, GetData, YuvTx P2P methods. Selects new transactions from outside
/// and provides it to the TransactionChecker.
#[derive(Clone)]
pub struct Controller<TxsStorage, StateStorage, P2pClient>
where
    TxsStorage: TransactionsStorage + PagesNumberStorage + PagesStorage + Clone,
    StateStorage: InventoryStorage + MempoolStorage + MempoolEntryStorage + Clone,
    P2pClient: ClientHandle,
{
    /// Node's persistent storage.
    txs_storage: TxsStorage,

    /// Node's storage for state values. For example, inventory.
    state_storage: StateStorage,

    /// Event bus for simplifying communication with services.
    event_bus: EventBus,

    /// Max inventory size.
    max_inv_size: usize,

    /// Inventory sharing interval.
    inv_sharing_interval: Duration,

    /// P2P handle which is used for sending messages to other peers.
    p2p_handle: P2pClient,

    /// Amount of transactions that fit one page.
    tx_per_page: u64,
}

impl<TS, SS, P2P> Controller<TS, SS, P2P>
where
    TS: TransactionsStorage + PagesNumberStorage + PagesStorage + Send + Sync + Clone + 'static,
    SS: InventoryStorage + MempoolStorage + MempoolEntryStorage + Send + Sync + Clone + 'static,
    P2P: ClientHandle + Send + Sync + Clone + 'static,
{
    pub fn new(
        full_event_bus: &EventBus,
        txs_storage: TS,
        state_storage: SS,
        p2p_handle: P2P,
        tx_per_page: u64,
    ) -> Self {
        let event_bus = full_event_bus
            .extract(
                &typeid![
                    TxConfirmMessage,
                    TxCheckerMessage,
                    GraphBuilderMessage,
                    IndexerMessage
                ],
                &typeid![ControllerMessage],
            )
            .expect("event channels must be presented");

        Self {
            txs_storage,
            state_storage,
            max_inv_size: DEFAULT_INV_SIZE,
            inv_sharing_interval: DEFAULT_INV_SHARE_INTERVAL,
            event_bus,
            p2p_handle,
            tx_per_page,
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
            Message::InvalidTxs(tx_ids) => self
                .handle_invalid_txs(tx_ids)
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
            Message::InitializeTxs(txs) => self
                .handle_new_yuv_txs(txs, None)
                .await
                .wrap_err("failed to handle transactions to initialize")?,
            Message::PartiallyCheckedTxs(txids) => {
                self.handle_partially_checked_txs(txids)
                    .await
                    .wrap_err("failed to handle partially checked transactions")?
            }
            Message::MinedTxs(txids) => self
                .handle_mined_txs(txids)
                .await
                .wrap_err("failed to handle mined transactions")?,
            Message::FullyCheckedTxs(txs) => self
                .handle_fully_checked_txs(txs)
                .await
                .wrap_err("failed to handle fully checked txs")?,
            Message::ConfirmedTxs(txids) => self
                .handle_confirmed_txs(txids)
                .await
                .wrap_err("failed to handle confirmed transactions")?,
            Message::Reorganization {
                txs,
                new_indexing_height,
            } => self
                .handle_reorganization(txs, new_indexing_height)
                .await
                .wrap_err("failed to handle reorged transactions")?,
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

    /// Fetch transactions from the mempool and distribute them among the workers depending on
    /// their statuses.
    pub async fn handle_mempool_txs(&mut self) -> eyre::Result<()> {
        let raw_mempool = self.state_storage.get_mempool().await?.unwrap_or_default();
        if raw_mempool.is_empty() {
            tracing::debug!("No transactions found in the mempool");
            return Ok(());
        }

        let mut handled_txs = Vec::new();
        for txid in raw_mempool {
            // If an entry is missing, it should be removed from the raw mempool.
            let Some(mempool_entry) = self.state_storage.get_mempool_entry(&txid).await? else {
                tracing::debug!(txid = txid.to_string(), "Tx is not present in the mempool");
                continue;
            };

            match mempool_entry.status {
                #[allow(deprecated)]
                MempoolStatus::Initialized | MempoolStatus::Pending => {
                    self.event_bus
                        .send(TxCheckerMessage::IsolatedCheck(vec![mempool_entry.yuv_tx]))
                        .await
                }
                MempoolStatus::Attaching => {
                    self.event_bus
                        .send(GraphBuilderMessage::CheckedTxs(vec![mempool_entry.yuv_tx]))
                        .await
                }
                // If the transaction is mined or waiting to be mined, just send it back to the
                // confrimator.
                _ => {
                    self.event_bus.send(TxConfirmMessage::Txs(vec![txid])).await;
                }
            }
            handled_txs.push(txid);
        }
        self.state_storage.put_mempool(handled_txs).await?;

        Ok(())
    }

    /// Handles invalid transactions. It removes them from the
    /// [`handling_txs`](Controller::handling_txs) and if the transaction was received from the
    /// network, it will send event to the network service that the sender peer is malicious.
    async fn handle_invalid_txs(&self, txids: Vec<Txid>) -> Result<()> {
        let mut raw_mempool = self.state_storage.get_mempool().await?.unwrap_or_default();
        clear_mempool(&mut raw_mempool, &txids);
        self.state_storage.put_mempool(raw_mempool).await?;

        for txid in &txids {
            self.state_storage.delete_mempool_entry(txid).await?;

            let sender_opt = self
                .state_storage
                .get_mempool_entry(txid)
                .await?
                .and_then(|entry| entry.sender);
            let Some(sender) = sender_opt else {
                continue;
            };
            self.p2p_handle.ban_peer(sender).await.wrap_err_with(|| {
                format!(
                    "failed to punish peer; malicious_peer={:?}; tx_ids={:?}",
                    sender, txids,
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
        tracing::debug!("Received inv from peer: {:?}", sender);

        let mut missing_tx_payload = Vec::<Inventory>::default();

        for inv_msg in inv {
            match inv_msg {
                Inventory::Ytx(ytx_id) => {
                    let existing_tx_opt = self
                        .is_tx_exist(&ytx_id)
                        .await
                        .wrap_err("failed to check if tx exist")?;

                    let Some(existing_tx) = existing_tx_opt else {
                        missing_tx_payload.push(Inventory::Ytx(ytx_id));
                        continue;
                    };

                    let is_announcement = matches!(
                        existing_tx.tx_type,
                        YuvTxType::Announcement(Announcement::Issue(_))
                    );

                    if is_announcement {
                        missing_tx_payload.push(Inventory::Ytx(ytx_id));
                    }
                }
            }
        }

        if !missing_tx_payload.is_empty() {
            tracing::debug!(
                "Requesting txs from peer {:?}: {:?}",
                sender,
                missing_tx_payload
            );

            self.p2p_handle
                .send_get_data(missing_tx_payload, sender)
                .await
                .wrap_err("failed to send getdata message")?;
        }

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
                        continue;
                    };

                    let mempool_entry = self.state_storage.get_mempool_entry(ytx_id).await?;
                    if let Some(mempool_entry) = mempool_entry {
                        response_txs.push(mempool_entry.yuv_tx);
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
        let mut new_txs = Vec::new();

        for yuv_tx in yuv_txs {
            let tx_id = yuv_tx.bitcoin_tx.txid();
            let existing_tx_opt = self
                .is_tx_exist(&tx_id)
                .await
                .wrap_err("failed to check if tx exists")?;

            let Some(existing_tx) = existing_tx_opt else {
                self.state_storage
                    .put_mempool_entry(MempoolTxEntry::new(
                        yuv_tx.clone(),
                        MempoolStatus::Initialized,
                        sender,
                    ))
                    .await?;

                let mut raw_mempool = self.state_storage.get_mempool().await?.unwrap_or_default();
                raw_mempool.push(tx_id);
                self.state_storage.put_mempool(raw_mempool).await?;

                tracing::debug!(
                    txid = tx_id.to_string(),
                    "Added initialized tx to the mempool"
                );

                new_txs.push(yuv_tx);

                continue;
            };

            if !matches!(existing_tx.tx_type, YuvTxType::Announcement(_)) {
                tracing::debug!(txid = tx_id.to_string(), "Tx exists in the storage");
                continue;
            }

            self.state_storage
                .put_mempool_entry(MempoolTxEntry::new(
                    yuv_tx.clone(),
                    MempoolStatus::Initialized,
                    sender,
                ))
                .await?;

            new_txs.push(yuv_tx);
        }

        if !new_txs.is_empty() {
            let txids: Vec<Txid> = new_txs.iter().map(|tx| tx.bitcoin_tx.txid()).collect();
            if let Some(sender) = sender {
                tracing::debug!("Received new yuv txs from {}: {:?}", sender, txids);
            } else {
                tracing::debug!("Received new yuv txs: {:?}", txids);
            }

            self.event_bus
                .send(TxCheckerMessage::IsolatedCheck(new_txs))
                .await;
        }

        Ok(())
    }

    /// Handles YUV transactions that passed the isolated checks and changes their statuses from
    /// `Initialized` to `WaitingMined`, then sends them to the tx confirmator.
    pub async fn handle_partially_checked_txs(&mut self, txids: Vec<Txid>) -> Result<()> {
        let mut yuv_txs = Vec::new();

        for txid in txids {
            let mut tx_entry = self
                .state_storage
                .get_mempool_entry(&txid)
                .await?
                .wrap_err("Initialized tx is not present in the mempool")?;

            tx_entry.status = MempoolStatus::WaitingMined;
            self.state_storage.put_mempool_entry(tx_entry).await?;

            tracing::debug!(
                txid = txid.to_string(),
                "Tx has passed the isolated check and is waiting to be mined"
            );

            yuv_txs.push(txid);
        }

        self.event_bus.send(TxConfirmMessage::Txs(yuv_txs)).await;

        Ok(())
    }

    /// Handles YUV transactions that passed the full check and changes their statuses from
    /// `Mined` to `Attaching`, then sends them to the graph builder.
    pub async fn handle_fully_checked_txs(&mut self, yuv_txs: Vec<YuvTransaction>) -> Result<()> {
        let mut non_announcement_txs = Vec::new();
        let mut announcement_txs = Vec::new();

        for yuv_tx in yuv_txs {
            tracing::debug!(
                txid = yuv_tx.bitcoin_tx.txid().to_string(),
                "Tx has passed the full check and is waiting to be attached"
            );

            if matches!(yuv_tx.tx_type, YuvTxType::Announcement(_)) {
                announcement_txs.push(yuv_tx);
                continue;
            }

            let mut mempool_entry = self
                .state_storage
                .get_mempool_entry(&yuv_tx.bitcoin_tx.txid())
                .await?
                .wrap_err("Confirmed tx is not present in the mempool")?;
            mempool_entry.status = MempoolStatus::Attaching;
            self.state_storage.put_mempool_entry(mempool_entry).await?;
            non_announcement_txs.push(yuv_tx);
        }

        if !announcement_txs.is_empty() {
            self.handle_checked_announcements(announcement_txs).await?;
        }

        if !non_announcement_txs.is_empty() {
            self.event_bus
                .send(GraphBuilderMessage::CheckedTxs(non_announcement_txs))
                .await;
        }

        Ok(())
    }

    /// Sends transactions that appeared in reorged blocks back to the confirmator.
    pub async fn handle_reorganization(
        &mut self,
        txids: Vec<Txid>,
        new_indexing_height: usize,
    ) -> Result<()> {
        self.event_bus
            .send(IndexerMessage::Reorganization(new_indexing_height))
            .await;

        if txids.is_empty() {
            return Ok(());
        }

        tracing::debug!("Reorged YUV transactions: {:?}", txids);

        for txid in &txids {
            let mut entry = self
                .state_storage
                .get_mempool_entry(txid)
                .await?
                .wrap_err("Reorged tx is not present in the mempool")?;
            entry.status = MempoolStatus::WaitingMined;
            self.state_storage.put_mempool_entry(entry).await?;
        }

        self.event_bus.send(TxConfirmMessage::Txs(txids)).await;

        Ok(())
    }

    /// Handles YUV transactions that reached enough confirmations and sends them to the tx checker
    /// for a full check.
    pub async fn handle_confirmed_txs(&mut self, txids: Vec<Txid>) -> Result<()> {
        let mut announcement_yuv_txs = Vec::new();
        let mut yuv_txs = Vec::new();

        for txid in txids {
            let tx_entry = self
                .state_storage
                .get_mempool_entry(&txid)
                .await?
                .wrap_err("Mined tx is not present in the mempool")?;

            tracing::debug!(
                txid = txid.to_string(),
                "Tx has reached enough confirmations"
            );

            let yuv_tx = tx_entry.yuv_tx;
            if matches!(yuv_tx.tx_type, YuvTxType::Announcement(_)) {
                announcement_yuv_txs.push((yuv_tx, tx_entry.sender));
            } else {
                yuv_txs.push((yuv_tx, tx_entry.sender));
            }
        }

        announcement_yuv_txs.extend(yuv_txs);
        self.event_bus
            .send(TxCheckerMessage::FullCheck(announcement_yuv_txs))
            .await;

        Ok(())
    }

    /// Handles YUV transactions that reached one confirmation and changes their statuses from
    /// `WaitingMined` to `Mined`, then adds them to the inventory so they can be broadcasted
    /// via P2P.
    pub async fn handle_mined_txs(&mut self, txids: Vec<Txid>) -> Result<()> {
        let mut txids_to_share = Vec::new();

        for txid in txids {
            let mut tx_entry = self
                .state_storage
                .get_mempool_entry(&txid)
                .await?
                .wrap_err("Waiting tx is not present in the mempool")?;

            if !matches!(tx_entry.yuv_tx.tx_type, YuvTxType::Announcement(_)) {
                txids_to_share.push(txid);
            }

            tx_entry.status = MempoolStatus::Mined;
            self.state_storage.put_mempool_entry(tx_entry).await?;
        }

        let mut inv = self.state_storage.get_inventory().await?;
        update_inv(&mut inv, &txids_to_share, self.max_inv_size);
        self.state_storage.put_inventory(inv).await?;

        tracing::info!("Inventory has been updated with checked and mined txs");

        Ok(())
    }

    /// Handles attached transactions by removing them from the mempool.
    pub async fn handle_attached_txs(&mut self, txids: Vec<Txid>) -> Result<()> {
        for txid in &txids {
            tracing::info!(txid = txid.to_string(), "Tx is attached");
            let entry = self
                .state_storage
                .get_mempool_entry(txid)
                .await?
                .wrap_err("Attaching tx is not present in the mempool")?;

            self.txs_storage.put_yuv_tx(entry.yuv_tx).await?;
            self.state_storage.delete_mempool_entry(txid).await?;
        }

        // Handle that number of transactions in batch could be more than
        // a number of transactions in page.
        for txs in txids.chunks(self.tx_per_page as usize) {
            self.put_txs_ids_to_page(txs)
                .await
                .wrap_err("Failed to store transactions in pages")?;
        }

        let mut raw_mempool = self.state_storage.get_mempool().await?.unwrap_or_default();
        clear_mempool(&mut raw_mempool, &txids);
        self.state_storage.put_mempool(raw_mempool).await?;

        Ok(())
    }

    /// Put attached transactions ids to page storage.
    async fn put_txs_ids_to_page(&self, txids: &[Txid]) -> eyre::Result<()> {
        let last_page_num = self
            .txs_storage
            .get_pages_number()
            .await?
            .unwrap_or_default();

        let mut last_page = self
            .txs_storage
            .get_page_by_num(last_page_num)
            .await?
            .unwrap_or_default();

        // Get space that is left in current page
        let left_space = self.tx_per_page.saturating_sub(last_page.len() as u64);

        // And split attached txs into two arrays, where the first ones will
        // be stored to current page, and other in next.
        let (in_current_page, in_next_page) = split_at(txids, left_space as usize);

        // If there is some left space to store in current page, store it:
        if !in_current_page.is_empty() {
            last_page.extend(in_current_page);

            self.txs_storage.put_page(last_page_num, last_page).await?;
        }

        // If there is some, store them in next page, and increment the page number.
        if !in_next_page.is_empty() {
            let next_page_num = last_page_num + 1;

            self.txs_storage
                .put_page(next_page_num, in_next_page.to_vec())
                .await?;

            self.txs_storage.put_pages_number(next_page_num).await?;
        }

        Ok(())
    }

    /// Handles checked announcement. It removes it from the mempool.
    pub async fn handle_checked_announcements(
        &mut self,
        announcement_txs: Vec<YuvTransaction>,
    ) -> Result<()> {
        let txids: Vec<Txid> = announcement_txs
            .iter()
            .map(|tx| tx.bitcoin_tx.txid())
            .collect();
        for txs in txids.chunks(self.tx_per_page as usize) {
            self.put_txs_ids_to_page(txs)
                .await
                .wrap_err("Failed to store announcements in pages")?;
        }

        for announcement_tx in announcement_txs {
            let announcement_txid = announcement_tx.bitcoin_tx.txid();
            self.state_storage
                .delete_mempool_entry(&announcement_txid)
                .await?;

            let mut raw_mempool = self.state_storage.get_mempool().await?.unwrap_or_default();
            raw_mempool.retain(|txid| *txid != announcement_txid);
            self.state_storage.put_mempool(raw_mempool).await?;

            self.txs_storage.put_yuv_tx(announcement_tx).await?;

            tracing::info!(
                txid = announcement_txid.to_string(),
                "Announcement is handled"
            );
        }

        Ok(())
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

    async fn is_tx_exist(&self, tx_id: &Txid) -> Result<Option<YuvTransaction>> {
        let entry = self.state_storage.get_mempool_entry(tx_id).await?;
        if let Some(entry) = entry {
            return Ok(Some(entry.yuv_tx));
        }

        let yuv_tx = self
            .txs_storage
            .get_yuv_tx(tx_id)
            .await
            .wrap_err("failed to get yuv tx")?;
        if let Some(yuv_tx) = yuv_tx {
            return Ok(Some(yuv_tx));
        }

        Ok(None)
    }
}

pub fn update_inv<T: Copy>(inv: &mut VecDeque<T>, mut txs: &[T], max_inv_size: usize) {
    if inv.len() + txs.len() < max_inv_size {
        inv.extend(txs);
        return;
    }

    // Shrink incoming txs we can add to inventory only
    // last `max_inv_size` items
    if txs.len() >= max_inv_size {
        txs = &txs[txs.len() - max_inv_size..];
    }

    let excess = (inv.len() + txs.len()).saturating_sub(max_inv_size);

    // Pop front elements to make space for new transactions
    inv.drain(..excess);

    // Add new transactions
    inv.extend(txs);
}

fn clear_mempool<T: Eq + Hash>(raw_mempool: &mut Vec<T>, txs: &[T]) {
    let txs_set = HashSet::<&T>::from_iter(txs.iter());
    raw_mempool.retain(|txid| !txs_set.contains(txid));
}

/// Split at array without panic
fn split_at<T>(txids: &[T], left_space: usize) -> (&[T], &[T]) {
    txids.split_at(left_space.min(txids.len()))
}
