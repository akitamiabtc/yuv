//! -----------------------------------------------
//! Utils methods and function for this module only
//! -----------------------------------------------

use bitcoin::{
    blockdata::{
        opcodes::all::{OP_CHECKMULTISIG, OP_EQUALVERIFY, OP_HASH160, OP_SWAP},
        script::Builder,
    },
    hashes::{hash160, Hash},
    secp256k1::PublicKey,
};

/// Local extenstion for script Builder which adds methods for pushing chunks
/// of opcodes from HTLC scripts.
pub(super) trait HtlcBuilderExt {
    /// Push program:
    ///
    /// ```text
    /// 2 OP_SWAP <local_htlcpubkey> 2 OP_CHECKMULTISIG
    /// ```
    ///
    /// Onto the builder.
    fn push_local_key_multisig_check(self, local_htlc_pubkey: &PublicKey) -> Builder;

    /// Push program:
    ///
    /// ```text
    /// OP_HASH160 <payment_hash> OP_EQUALVERIFY
    /// ```
    ///
    /// Onto the builder.
    fn push_payment_hash_check(self, payment_hash: &hash160::Hash) -> Builder;
}

impl HtlcBuilderExt for Builder {
    fn push_local_key_multisig_check(self, local_htlc_pubkey: &PublicKey) -> Builder {
        self.push_int(2)
            .push_opcode(OP_SWAP)
            .push_key(&bitcoin::PublicKey::new(*local_htlc_pubkey))
            .push_int(2)
            .push_opcode(OP_CHECKMULTISIG)
    }

    fn push_payment_hash_check(self, payment_hash: &hash160::Hash) -> Builder {
        self.push_opcode(OP_HASH160)
            .push_slice(payment_hash.as_byte_array())
            .push_opcode(OP_EQUALVERIFY)
    }
}
