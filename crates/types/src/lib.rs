#![cfg_attr(feature = "no-std", no_std)]

extern crate alloc;
extern crate core;

pub use announcements::{Announcement, AnyAnnouncement};
#[cfg(all(feature = "messages", feature = "std"))]
pub use messages::{
    ControllerMessage, ControllerP2PMessage, GraphBuilderMessage, TxCheckerMessage,
    TxConfirmMessage,
};
#[cfg(feature = "bulletproof")]
pub use proofs::is_bulletproof;
pub use proofs::{ProofMap, TransferProofs};
pub use transactions::{YuvTransaction, YuvTxType};

#[cfg(not(any(feature = "std", feature = "no-std")))]
compile_error!("at least one of the `std` or `no-std` features must be enabled");

pub mod announcements;
pub mod network;
mod transactions;

#[cfg(feature = "consensus")]
mod consensus;
#[cfg(all(feature = "messages", feature = "std"))]
pub mod messages;

mod proofs;

/// The default number of confirmations required to consider a block or transaction as confirmed.
pub const DEFAULT_CONFIRMATIONS_NUMBER: u8 = 6;
