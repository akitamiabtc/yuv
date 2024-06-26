use bitcoin::{ecdsa::Error as EcdsaSigError, Txid};

use yuv_pixels::{PixelProof, PixelProofError};

/// Errors that can occur during the transaction checking.
#[derive(thiserror::Error, Debug)]
pub enum CheckError {
    /// Proof provided to transaction is not valid.
    #[error("Invalid proof {proof:?} for {vout}: {error}")]
    InvalidProof {
        /// Proof that is not valid.
        ///
        /// `Box` is used here to reduce size of the enum.
        proof: Box<PixelProof>,
        /// Number of output in the transaction.
        vout: u32,
        /// Error that occurred during transaction checking.
        error: PixelProofError,
    },

    #[error("Number of pixel proofs must be the same as the number of Bitcoin outputs")]
    NotEnoughProofs { provided: usize, required: usize },

    /// There is no signature and/or pubkey in p2wpkh transaction.
    #[error("Invalid witness structure")]
    InvalidWitness,

    /// Input and/or output proofs has different chroma.
    #[error("Chroma of proofs is not the same")]
    NotSameChroma,

    /// Invalid public key.
    #[error("Invalid public key: {0}")]
    InvalidKey(#[from] bitcoin::key::Error),

    /// Invalid signature (in witness).
    #[error("Invalid signature : {0}")]
    InvalidSignature(#[from] EcdsaSigError),

    /// Sum of inputs is not equal to sum of outputs.
    #[error("Sum of inputs is not equal to sum of outputs")]
    ConservationRulesViolated,

    /// Issuer of tokens is not the owner of the chroma.
    #[error("Issuer is not the owner of the chroma")]
    IssuerNotOwner,

    #[error("Empty outputs")]
    EmptyOutputs,

    #[error("Empty inputs")]
    EmptyInputs,

    #[error("Input transaction not found")]
    InputNotFound,

    #[error("Transaction output not found")]
    OutputNotFound,

    /// Proof mapped to not existing input or outputm, which is considered as
    /// invalid proof for that transaction.
    #[error("Proof mapped to not existing input/output")]
    ProofMappedToNotExistingInputOutput,

    /// Transaction has the bulletproof pixel proofs and non-bulletproof one
    #[error("Mixed bulletproofs and non-bulletproofs")]
    MixedBulletproofsAndNonBulletproofs,

    /// To verify transaction, at least one commitment is needed.
    #[error("To verify transaction, at least one commitment is needed")]
    AtLeastOneCommitment,

    #[error("Invalid verifier")]
    InvalidVerifier,

    #[error("Tx not found {0}")]
    TxNotFound(Txid),

    #[error("Commitments result in an invalid public key")]
    InvalidPublicKey,

    #[error("Public key to verify a signature not found")]
    PublicKeyNotFound,

    #[error("Message to verify a signature not found")]
    MessageKeyNotFound,

    #[cfg(feature = "bulletproof")]
    #[error("Transaction type is not bulletproof")]
    NotBulletproof,

    #[error("Announced amount {0} does not match to amount in pixel proofs {1}")]
    AnnouncedAmountDoesNotMatch(u128, u128),

    #[error("Provided announcement mismatch with the announcement in transaction")]
    IssueAnnouncementMismatch,

    #[error("Provided transaction doesn't have an announcement")]
    IssueAnnouncementNotProvided,
}

/// [`TransactionChecker`](crate::TransactionChecker) errors.
#[derive(thiserror::Error, Debug)]
pub enum TxCheckerError {
    #[error("Check error: {0}")]
    Check(#[from] CheckError),

    #[error("Connection error: {0}")]
    Connection(#[from] bitcoin_client::Error),
}
