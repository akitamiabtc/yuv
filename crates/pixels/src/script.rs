//! This module consists of definitions of Bitcoin scripts that are used for
//! transaction creation and validation.
//!
use alloc::vec::Vec;
use bitcoin::{
    blockdata::{
        opcodes::{
            self,
            all::{
                OP_CHECKSIG, OP_CSV, OP_DROP, OP_ELSE, OP_ENDIF, OP_IF, OP_PUSHBYTES_33,
                OP_PUSHNUM_2,
            },
        },
        script::{self, Builder, Instruction},
    },
    secp256k1::PublicKey,
    ScriptBuf,
};
use core::fmt::{self, Display};

use crate::errors::ToLocalScriptParseError;
use crate::PixelKey;

/// Represents parsed values inside a Lighting Network commitment transaction
/// `to_local` output script.
///
/// This script in general looks like this:
///
/// ```text
/// OP_IF
///    <revocationpubkey>
/// OP_ELSE
///    `to_self_delay`
///    OP_CSV # OP_CHECKSEQUENCEVERIFY
///    OP_DROP
///    <local_delayedpubkey>
/// OP_ENDIF
/// ```
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ToLocalScript {
    /// Derived from revocation secret key.
    ///
    /// By rules of the YUV protocol, this key is always tweaked by pixel in
    /// script.
    pub revocation_pubkey: PixelKey,

    /// Delay after which the `to_local` output can be spent by the local.
    pub to_self_delay: u16,

    /// Derived from local delayed secret key.
    pub local_delayed_pubkey: PublicKey,
}

impl Display for ToLocalScript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let script = ScriptBuf::from(self);

        write!(f, "{}", script)
    }
}

impl ToLocalScript {
    pub fn new(
        revocation_pubkey: PixelKey,
        to_self_delay: u16,
        local_delayed_pubkey: PublicKey,
    ) -> Self {
        Self {
            revocation_pubkey,
            to_self_delay,
            local_delayed_pubkey,
        }
    }
}

impl From<&ToLocalScript> for ScriptBuf {
    fn from(value: &ToLocalScript) -> Self {
        Builder::new()
            .push_opcode(OP_IF)
            .push_slice(value.revocation_pubkey.0.inner.serialize())
            .push_opcode(OP_ELSE)
            .push_int(value.to_self_delay as i64)
            .push_opcode(OP_CSV)
            .push_opcode(OP_DROP)
            .push_slice(value.local_delayed_pubkey.serialize())
            .push_opcode(OP_ENDIF)
            .push_opcode(OP_CHECKSIG)
            .into_script()
    }
}

impl From<ToLocalScript> for ScriptBuf {
    fn from(value: ToLocalScript) -> Self {
        Self::from(&value)
    }
}

const SCRIPT_OPCODES: [opcodes::All; 9] = [
    OP_IF,
    OP_PUSHBYTES_33, // revocationpubkey
    OP_ELSE,
    OP_PUSHNUM_2, // to_self_delay
    OP_CSV,
    OP_DROP,
    OP_PUSHBYTES_33, // local_delayedpubkey
    OP_ENDIF,
    OP_CHECKSIG,
];
const REVOCAION_PUBKEY_INDEX: usize = 1;
const TO_SELF_DELAY_INDEX: usize = 3;
const LOCAL_DELAYED_PUBKEY_INDEX: usize = 6;
const PUSH_INDEXES: [usize; 3] = [
    REVOCAION_PUBKEY_INDEX,
    TO_SELF_DELAY_INDEX,
    LOCAL_DELAYED_PUBKEY_INDEX,
];

impl TryFrom<&ScriptBuf> for ToLocalScript {
    type Error = ToLocalScriptParseError;

    /// Parse `to_local` output script of the commitment transaction.
    fn try_from(script: &ScriptBuf) -> Result<Self, Self::Error> {
        let mut instructions = script.instructions();

        // Variables for storing parsed values.
        let mut pubkeys = Vec::new();
        let mut to_self_delay = None;

        // Iterate through required instructions to match them with the given
        // script.
        for (index, opcode) in SCRIPT_OPCODES.iter().enumerate() {
            let instruction =
                instructions
                    .next()
                    .ok_or(ToLocalScriptParseError::Instruction {
                        expected: *opcode,
                        found: None,
                        index,
                    })??;

            match instruction {
                // In case of `PUSHBYTES` like opcodes, check that they are at
                // right place (indexes).
                Instruction::PushBytes(data) if PUSH_INDEXES.contains(&index) => match index {
                    // If opcode is at this index, then it is a public key, so
                    // we will parse it and store it in the `pubkeys` vector.
                    REVOCAION_PUBKEY_INDEX | LOCAL_DELAYED_PUBKEY_INDEX => {
                        let pubkey = PublicKey::from_slice(data.as_bytes());

                        pubkeys.push(pubkey);
                    }
                    // If opcode is at this index, then it is a `to_self_delay`
                    // value, so we will parse it and store it.
                    TO_SELF_DELAY_INDEX => {
                        if data.len() != 2 {
                            return Err(ToLocalScriptParseError::ToSelfDelay);
                        }

                        to_self_delay = Some(u16::from_le_bytes([data[0], data[1]]));
                    }
                    _ => unreachable!(
                        "As PUSH_INDEXES contains only REVOCAION_PUBKEY_INDEX,\
                         TO_SELF_DELAY_INDEX and LOCAL_DELAYED_PUBKEY_INDEX"
                    ),
                },
                // In case if ordinal opcode, check that it is equal to the
                // expected one, otherwise return an error.
                Instruction::Op(op) if &op == opcode => {
                    continue;
                }
                // Otherwise, we don't expect an other siutation, so return an
                // error.
                _ => {
                    return Err(ToLocalScriptParseError::Instruction {
                        expected: *opcode,
                        found: None,
                        index,
                    });
                }
            }
        }

        match (pubkeys.as_slice(), to_self_delay) {
            ([revocation_pubkey, local_delayed_pubkey], Some(to_self_delay)) => Ok(Self {
                revocation_pubkey: PixelKey(bitcoin::PublicKey::new(revocation_pubkey.unwrap())),
                to_self_delay,
                local_delayed_pubkey: local_delayed_pubkey.unwrap(),
            }),
            _ => Err(ToLocalScriptParseError::Script(
                script::Error::EarlyEndOfScript,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::ScriptBuf;

    use crate::script::ToLocalScript;
    use crate::ToLocalScriptParseError;

    /// Test simple example from [BOLT 3]
    ///
    /// [BOLT3]: https://github.com/lightning/bolts/blob/master/03-transactions.md.
    #[test]
    fn test_simple_commitment_script_sig_parsing() -> Result<(), ToLocalScriptParseError> {
        const SCRIPT_SIG: &str = "63210212a140cd0c6539d07cd08dfe09984dec3251ea808b89\
                                  2efeac3ede9402bf2b1967029000b2752103fd5960528dc152\
                                  014952efdb702a88f71e3c1653b2314431701ec77e57fde83c68\
                                  ac";

        let parsed_script = ScriptBuf::from_hex(SCRIPT_SIG).expect("Should always be valid");

        let to_local_script = ToLocalScript::try_from(&parsed_script)?;

        let _to_local_script_raw = ScriptBuf::from(&to_local_script);

        Ok(())
    }
}
