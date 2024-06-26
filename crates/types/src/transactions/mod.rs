use bitcoin::Transaction;

use crate::announcements::{Announcement, IssueAnnouncement};
use crate::ProofMap;

#[cfg(feature = "bulletproof")]
use crate::is_bulletproof;

/// Represents entries of the YUV transaction inside the node's storage and
/// P2P communication inventory
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct YuvTransaction {
    pub bitcoin_tx: Transaction,
    pub tx_type: YuvTxType,
}

impl YuvTransaction {
    /// Create a new YUV transaction.
    pub fn new(bitcoin_tx: Transaction, tx_type: YuvTxType) -> Self {
        Self {
            bitcoin_tx,
            tx_type,
        }
    }

    /// Checks if the transaction is bulletproof.
    ///
    /// Returns `true` if it is a bulletproof transaction, `false` otherwise.
    #[cfg(feature = "bulletproof")]
    pub fn is_bulletproof(&self) -> bool {
        match self.tx_type.output_proofs() {
            Some(proofs) => is_bulletproof(proofs.values()),
            None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", content = "data"))]
pub enum YuvTxType {
    Issue {
        output_proofs: Option<ProofMap>,
        announcement: IssueAnnouncement,
    },
    Transfer {
        input_proofs: ProofMap,
        output_proofs: ProofMap,
    },
    Announcement(Announcement),
}

impl YuvTxType {
    /// Return output proofs if possible
    pub fn output_proofs(&self) -> Option<&ProofMap> {
        match self {
            Self::Issue { output_proofs, .. } => output_proofs.as_ref(),
            Self::Transfer { output_proofs, .. } => Some(output_proofs),
            _ => None,
        }
    }

    /// Return input proofs if possible
    pub fn input_proofs(&self) -> Option<&ProofMap> {
        match self {
            Self::Transfer { input_proofs, .. } => Some(input_proofs),
            _ => None,
        }
    }
}

impl Default for YuvTxType {
    fn default() -> Self {
        Self::Transfer {
            output_proofs: Default::default(),
            input_proofs: Default::default(),
        }
    }
}

impl From<Announcement> for YuvTxType {
    fn from(value: Announcement) -> Self {
        Self::Announcement(value)
    }
}
