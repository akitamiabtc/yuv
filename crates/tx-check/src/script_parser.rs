use core::fmt;

use bitcoin::{
    hashes::{hash160, sha256, Hash},
    AddressType, PubkeyHash, ScriptBuf, ScriptHash, TxIn, WPubkeyHash, WScriptHash, Witness,
};

/// Defines the spending condition (i.e. `scriptPubKey`) for diffrent type of addresses.
///
/// Supported address types:
///
/// - `P2PKH`
/// - `P2SH`
/// - `P2WPKH`
/// - `P2WSH`
///
/// ## Note
///
/// `P2TR` is coming soon.
#[derive(Debug, Clone)]
pub(crate) enum SpendingCondition {
    P2PKH(PubkeyHashData),
    P2SH(ScriptHashData),
    P2WPKH(PubkeyHashData),
    P2WSH(ScriptHashData),
}

impl SpendingCondition {
    /// Derive the spending condition from Bitcoin transaction input.
    pub(crate) fn from_txin(
        tx_in: &TxIn,
        address_type: AddressType,
    ) -> Result<Self, ScriptParseError> {
        match address_type {
            AddressType::P2wpkh => Ok(Self::P2WPKH(PubkeyHashData::from_witness(&tx_in.witness))),
            AddressType::P2wsh => Ok(Self::P2WSH(ScriptHashData::from_witness(&tx_in.witness)?)),
            AddressType::P2pkh | AddressType::P2sh => {
                Self::handle_legacy_address_type(tx_in, address_type)
            }
            _ => Err(ScriptParseError::UnsupportedScript),
        }
    }

    fn handle_legacy_address_type(
        tx_in: &TxIn,
        address_type: AddressType,
    ) -> Result<Self, ScriptParseError> {
        let instructions = tx_in.script_sig.instructions();

        let last_instruction = instructions
            .last()
            .transpose()?
            .ok_or(ScriptParseError::InsufficientInstructions(0))?;

        let last_instruction_bytes = last_instruction
            .push_bytes()
            .ok_or(ScriptParseError::NotDataPush)?
            .as_bytes();

        match address_type {
            AddressType::P2pkh => Ok(Self::P2PKH(PubkeyHashData::from_script_sig(
                last_instruction_bytes,
            )?)),
            AddressType::P2sh => Ok(Self::P2SH(ScriptHashData::from_script_sig(
                last_instruction_bytes,
            )?)),
            _ => Err(ScriptParseError::UnsupportedScript),
        }
    }

    pub fn into_script(self) -> ScriptBuf {
        match self {
            SpendingCondition::P2PKH(PubkeyHashData(script))
            | SpendingCondition::P2WPKH(PubkeyHashData(script))
            | SpendingCondition::P2SH(ScriptHashData(script))
            | SpendingCondition::P2WSH(ScriptHashData(script)) => script,
        }
    }
}

/// Used to derive `scriptPubKey` from `P2PKH` and `P2WPKH` inputs.
#[derive(Debug, Clone)]
pub struct PubkeyHashData(pub ScriptBuf);

impl PubkeyHashData {
    /// Derive the script from a `P2WPKH` input.
    pub fn from_witness(witness: &Witness) -> Self {
        let pubkey_hash = hash160::Hash::hash(&witness[1]);
        let script_pub_key = ScriptBuf::new_v0_p2wpkh(&WPubkeyHash::from_raw_hash(pubkey_hash));

        Self(script_pub_key)
    }

    /// Derive the script from a `P2PKH` input.
    pub fn from_script_sig(last_instruction_bytes: &[u8]) -> Result<Self, ScriptParseError> {
        let pubkey_hash = hash160::Hash::hash(last_instruction_bytes);
        let script_pub_key = ScriptBuf::new_p2pkh(&PubkeyHash::from_raw_hash(pubkey_hash));

        Ok(Self(script_pub_key))
    }
}

/// Used to derive `scriptPubKey` from `P2SH` and `P2WSH` inputs.
#[derive(Debug, Clone)]
pub struct ScriptHashData(pub ScriptBuf);

impl ScriptHashData {
    /// Derive the script from a `P2WSH` input.
    pub fn from_witness(witness: &Witness) -> Result<Self, ScriptParseError> {
        let redeem_script_bytes = witness
            .last()
            .ok_or(ScriptParseError::MissingWitnessRedeemScript)?;
        let script_hash = sha256::Hash::hash(redeem_script_bytes);
        let script_pub_key = ScriptBuf::new_v0_p2wsh(&WScriptHash::from_raw_hash(script_hash));

        Ok(Self(script_pub_key))
    }

    /// Derive the script from a `P2SH` input.
    pub fn from_script_sig(last_instruction_bytes: &[u8]) -> Result<Self, ScriptParseError> {
        let script_hash = hash160::Hash::hash(last_instruction_bytes);
        let script_pub_key = ScriptBuf::new_p2sh(&ScriptHash::from_raw_hash(script_hash));

        Ok(Self(script_pub_key))
    }
}

/// Error that can occur when parsing `scriptSig`.
#[derive(Debug, PartialEq, Eq)]
pub enum ScriptParseError {
    /// Provided script is not supported.
    UnsupportedScript,
    /// Instruction is invalid.
    InvalidInstruction(bitcoin::script::Error),
    // The last instruction is not a data push.
    NotDataPush,
    // The instruction set doesn't contain enough instructions.
    InsufficientInstructions(usize),
    // The witness doesn't contain a redeem script.
    MissingWitnessRedeemScript,
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for ScriptParseError {}

impl fmt::Display for ScriptParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScript => write!(f, "provided script is unsupported",),
            Self::InvalidInstruction(e) => write!(f, "invalid instruction: {}", e),
            Self::NotDataPush => write!(f, "the last instruction doesn't contain data"),
            Self::InsufficientInstructions(n) => write!(
                f,
                "not enough instructions provided in the scriptSig, provided {}, but should be at least two",
                n
            ),
            Self::MissingWitnessRedeemScript => write!(f, "witness is missing a redeem script")
        }
    }
}

impl From<bitcoin::script::Error> for ScriptParseError {
    fn from(err: bitcoin::script::Error) -> Self {
        Self::InvalidInstruction(err)
    }
}
