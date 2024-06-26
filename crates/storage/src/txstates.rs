use std::{collections::HashMap, sync::Arc};

use bitcoin::Txid;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct TxStatesStorage {
    tx_states: Arc<RwLock<HashMap<Txid, TxState>>>,
}

impl TxStatesStorage {
    pub async fn get(&self, txid: &Txid) -> Option<TxState> {
        let tx_states = self.tx_states.read().await;
        tx_states.get(txid).cloned()
    }

    pub async fn insert(&self, txid: Txid, new_state: TxState) {
        let mut tx_states = self.tx_states.write().await;
        tx_states.insert(txid, new_state);
    }

    pub async fn insert_if_not_exists(&self, txid: Txid, new_state: TxState) -> bool {
        let mut tx_states = self.tx_states.write().await;

        if tx_states.contains_key(&txid) {
            return false;
        }

        tx_states.insert(txid, new_state);

        true
    }

    pub async fn update_many(&self, tx_ids: &[Txid], new_state: TxState) {
        let mut tx_states = self.tx_states.write().await;

        for txid in tx_ids {
            tx_states.insert(*txid, new_state);
        }
    }

    pub async fn remove(&self, txid: &Txid) {
        let mut tx_states = self.tx_states.write().await;
        tx_states.remove(txid);
    }

    pub async fn remove_many(&self, txids: &[Txid]) {
        let mut tx_states = self.tx_states.write().await;

        for txid in txids {
            tx_states.remove(txid);
        }
    }

    pub async fn contains(&self, txid: &Txid) -> bool {
        let tx_states = self.tx_states.read().await;
        tx_states.contains_key(txid)
    }

    pub async fn len(&self) -> usize {
        let tx_states = self.tx_states.read().await;
        tx_states.len()
    }

    pub async fn is_empty(&self) -> bool {
        let tx_states = self.tx_states.read().await;
        tx_states.is_empty()
    }
}

/// Transaction states that are stored in storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum TxState {
    /// Transaction is pending to be checked.
    Pending = 1,

    /// Transaction is checked and ready to be attached.
    Checked = 2,
}
