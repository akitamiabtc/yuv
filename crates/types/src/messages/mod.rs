use alloc::vec::Vec;
use bitcoin::Txid;
use core::fmt::Debug;
use event_bus::Event;
use std::net::SocketAddr;

use crate::YuvTransaction;

use self::p2p::Inventory;

pub mod p2p;

/// Messages to Controller service.
#[derive(Clone, Debug, Event)]
pub enum ControllerMessage {
    /// Notification about invalid transactions.
    InvalidTxs {
        /// Transactions that were invalid.
        tx_ids: Vec<Txid>,
        /// Peer id of the sender.
        sender: Option<SocketAddr>,
    },
    /// Ask for data about transactions in P2P network.
    GetData {
        /// Ids of transactions to get.
        inv: Vec<Inventory>,
        /// Peer id of the sender.
        receiver: SocketAddr,
    },
    /// Send signed transactions for on-chain confirmation.
    ConfirmBatchTx(Vec<YuvTransaction>),
    /// Remove checked announcement from handling transactions.
    CheckedAnnouncement(Txid),
    /// New inventory to share with peers.
    AttachedTxs(Vec<Txid>),
    /// Data that is received from p2p.
    P2P(ControllerP2PMessage),
}

/// Message from P2P to Controller.
#[derive(Clone, Debug, Event)]
pub enum ControllerP2PMessage {
    /// Ask current state of the node's inventory.
    Inv {
        inv: Vec<Inventory>,
        /// Address of the sender.
        sender: SocketAddr,
    },
    /// Provide transactions data to the node.
    GetData {
        inv: Vec<Inventory>,
        /// Address of the sender.
        sender: SocketAddr,
    },
    /// Response of [`ControllerP2PMessage::GetData`].
    YuvTx {
        txs: Vec<YuvTransaction>,
        /// Address of the sender.
        sender: SocketAddr,
    },
}

/// Message to TxChecker service.
#[derive(Clone, Debug, Event)]
pub enum TxCheckerMessage {
    /// New transaction to check.
    NewTxs {
        /// New Transactions.
        txs: Vec<YuvTransaction>,
        /// Peer id of the sender:
        /// * Some if transactions received from p2p network
        /// * None if transactions received via json rpc
        sender: Option<SocketAddr>,
    },
}

/// Message to GraphBuilder service.
#[derive(Clone, Debug, Event)]
pub enum GraphBuilderMessage {
    /// Transactions to attach that already have been checked.
    CheckedTxs(Vec<YuvTransaction>),
}

/// Message to ConfirmationIndexer.
#[derive(Clone, Debug, Event)]
pub enum TxConfirmMessage {
    /// Transactions that should be confirmed before sending to the tx checker.
    TxsToConfirm(Vec<YuvTransaction>),
    /// Transactions that are confirmed.
    ConfirmedTxIds(Vec<Txid>),
}
