//! General structure of the Lightning Hash Time Locked Contract (HTLC) for offered:
//!
//! ```text
//! # To remote node with revocation key
//! OP_DUP OP_HASH160 <RIPEMD160(SHA256(revocationpubkey))> OP_EQUAL
//! OP_IF
//!    OP_CHECKSIG
//! OP_ELSE
//!    <remote_htlcpubkey> OP_SWAP OP_SIZE 32 OP_EQUAL
//!    OP_NOTIF
//!        # To local node via HTLC-success transaction.
//!        OP_DROP 2 OP_SWAP <local_htlcpubkey> 2 OP_CHECKMULTISIG
//!    OP_ELSE
//!        # To remote node after timeout.
//!        OP_HASH160 <RIPEMD160(payment_hash)> OP_EQUALVERIFY
//!        OP_CHECKSIG
//!    OP_ENDIF
//! OP_ENDIF
//! ```
//!
//! and received HTLC:
//!
//! ```text
//! OP_DUP OP_HASH160 <RIPEMD160(SHA256(revocationpubkey))> OP_EQUAL
//! OP_IF
//!    OP_CHECKSIG
//! OP_ELSE
//!    <remote_htlcpubkey> OP_SWAP OP_SIZE 32 OP_EQUAL
//!    OP_IF
//!        # To local node via HTLC-success transaction.
//!        OP_HASH160 <RIPEMD160(payment_hash)> OP_EQUALVERIFY
//!        2 OP_SWAP <local_htlcpubkey> 2 OP_CHECKMULTISIG
//!    OP_ELSE
//!        # To remote node after timeout.
//!        OP_DROP <cltv_expiry> OP_CHECKLOCKTIMEVERIFY OP_DROP
//!        OP_CHECKSIG
//!    OP_ENDIF
//! OP_ENDIF
//! ```
//!
//! Implements serialization of the Lightning HTLC scripts using [`From`] trait.

use bitcoin::{
    blockdata::{
        opcodes::all::{
            OP_CHECKSIG, OP_CLTV, OP_DROP, OP_DUP, OP_ELSE, OP_ENDIF, OP_EQUAL, OP_HASH160, OP_IF,
            OP_NOTIF, OP_SIZE, OP_SWAP,
        },
        script::Builder,
    },
    hashes::{hash160, Hash},
    script::PushBytesBuf,
    secp256k1::PublicKey,
    ScriptBuf, WScriptHash,
};

use super::utils::HtlcBuilderExt;

/// As `Script` data and proof data are mostly
pub type LightningHtlcScript = LightningHtlcData;

/// Size of the payment hash (sha256) in bytes.
pub const PAYMENT_HASH_SIZE: usize = 32;

/// Structure that holds Lightning network Hash Time Locked Contract (HTLC)
/// script data.
#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightningHtlcData {
    /// Hash of the revocation public key.
    pub revocation_key_hash: hash160::Hash,

    pub remote_htlc_key: PublicKey,

    pub local_htlc_key: PublicKey,

    pub payment_hash: hash160::Hash,

    pub kind: HtlcScriptKind,
}

/// There are two kinds of HTLC scripts: offered and received.
///
/// Offered HTLC script is the one that is offered by local node to remote node.
/// Received HTLC script is the one that is offered by remote node to local
/// node.
#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HtlcScriptKind {
    /// HTLC offered by local node.
    Offered,
    /// HTLC received by local node.
    Received { cltv_expiry: u32 },
}

impl LightningHtlcData {
    pub fn new(
        revocation_key_hash: hash160::Hash,
        tweaked_remote_htlc_key: PublicKey,
        local_htlc_key: PublicKey,
        payment_hash: hash160::Hash,
        kind: HtlcScriptKind,
    ) -> Self {
        Self {
            revocation_key_hash,
            remote_htlc_key: tweaked_remote_htlc_key,
            local_htlc_key,
            payment_hash,
            kind,
        }
    }

    pub fn received(
        revocation_key_hash: hash160::Hash,
        tweaked_remote_htlc_key: PublicKey,
        local_htlc_key: PublicKey,
        payment_hash: hash160::Hash,
        cltv_expiry: u32,
    ) -> Self {
        Self::new(
            revocation_key_hash,
            tweaked_remote_htlc_key,
            local_htlc_key,
            payment_hash,
            HtlcScriptKind::Received { cltv_expiry },
        )
    }

    pub fn offered(
        revocation_key_hash: hash160::Hash,
        tweaked_remote_htlc_key: PublicKey,
        local_htlc_key: PublicKey,
        payment_hash: hash160::Hash,
    ) -> Self {
        Self::new(
            revocation_key_hash,
            tweaked_remote_htlc_key,
            local_htlc_key,
            payment_hash,
            HtlcScriptKind::Offered,
        )
    }

    /// Converts the HTLC script to [`ScriptBuf`].
    ///
    /// Used in [`From`] trait implementation.
    fn to_script(self) -> ScriptBuf {
        let mut data = PushBytesBuf::new();
        // We don't expect this error to ever occur based on provided data
        let _ = data.extend_from_slice(&self.revocation_key_hash.to_byte_array());
        let mut builder = Builder::new()
            .push_opcode(OP_DUP)
            .push_opcode(OP_HASH160)
            .push_slice(&data)
            .push_opcode(OP_EQUAL)
            .push_opcode(OP_IF)
            .push_opcode(OP_CHECKSIG)
            .push_opcode(OP_ELSE)
            .push_key(&bitcoin::PublicKey::new(self.remote_htlc_key))
            .push_opcode(OP_SWAP)
            .push_opcode(OP_SIZE)
            .push_int(PAYMENT_HASH_SIZE as i64)
            .push_opcode(OP_EQUAL);

        builder = match self.kind {
            HtlcScriptKind::Offered => builder
                .push_opcode(OP_NOTIF)
                .push_opcode(OP_DROP)
                .push_local_key_multisig_check(&self.local_htlc_key)
                .push_opcode(OP_ELSE)
                .push_payment_hash_check(&self.payment_hash),
            HtlcScriptKind::Received { cltv_expiry } => builder
                .push_opcode(OP_IF)
                .push_payment_hash_check(&self.payment_hash)
                .push_local_key_multisig_check(&self.local_htlc_key)
                .push_opcode(OP_ELSE)
                .push_opcode(OP_DROP)
                .push_int(cltv_expiry as i64)
                .push_opcode(OP_CLTV)
                .push_opcode(OP_DROP),
        };

        // TODO: in future we should add support for anchors scripts here.

        builder = builder
            .push_opcode(OP_CHECKSIG)
            .push_opcode(OP_ENDIF)
            .push_opcode(OP_ENDIF);

        builder.into_script()
    }
}

impl From<LightningHtlcData> for ScriptBuf {
    fn from(value: LightningHtlcData) -> Self {
        Self::from(&value)
    }
}

impl From<&LightningHtlcData> for ScriptBuf {
    fn from(value: &LightningHtlcData) -> Self {
        value.to_script()
    }
}

impl From<LightningHtlcData> for WScriptHash {
    fn from(value: LightningHtlcData) -> Self {
        Self::from(&value)
    }
}

/// For comparing with value from `script_pubkey.
impl From<&LightningHtlcData> for WScriptHash {
    fn from(value: &LightningHtlcData) -> Self {
        let script = ScriptBuf::from(value);

        script.wscript_hash()
    }
}
