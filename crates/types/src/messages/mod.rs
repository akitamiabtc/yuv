use alloc::vec::Vec;
use bitcoin::Txid;
use bitcoin_client::json::GetBlockTxResult;
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
    InvalidTxs(Vec<Txid>),
    /// Ask for data about transactions in P2P network.
    GetData {
        /// Ids of transactions to get.
        inv: Vec<Inventory>,
        /// Peer id of the sender.
        receiver: SocketAddr,
    },
    /// Tranactions that passed the isolated check and are ready to be sent for confirmation.
    PartiallyCheckedTxs(Vec<Txid>),
    /// Tranactions that passed the full check and are ready to be sent to tx attacher.
    FullyCheckedTxs(Vec<YuvTransaction>),
    /// Share transactions with one confirmation with the P2P peers.
    MinedTxs(Vec<Txid>),
    /// Send confirmed transactions to the tx checker for a full check.
    ConfirmedTxs(Vec<Txid>),
    /// Send signed transactions for on-chain confirmation.
    InitializeTxs(Vec<YuvTransaction>),
    /// Handle a reorg.
    Reorganization {
        txs: Vec<Txid>,
        new_indexing_height: usize,
    },
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
    /// New transactions to pass the full check. The transactions come along with the peer id of
    /// the sender:
    /// * Some if transactions received from p2p network
    /// * None if transactions received via json rpc
    FullCheck(Vec<(YuvTransaction, Option<SocketAddr>)>),
    /// New transactions to pass the isolated check.
    IsolatedCheck(Vec<YuvTransaction>),
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
    Txs(Vec<Txid>),
    /// Transactions that are confirmed.
    Block(Box<GetBlockTxResult>),
}

/// Message to Indexer service.
#[derive(Clone, Debug, Event)]
pub enum IndexerMessage {
    /// New height to index blocks from. Sent from the controller in case of reorg.
    Reorganization(usize),
}
