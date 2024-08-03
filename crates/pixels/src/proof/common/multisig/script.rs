use alloc::vec::Vec;
use bitcoin::{
    blockdata::{
        opcodes,
        script::{Builder, Instruction},
    },
    secp256k1, Script,
};

use super::errors::MultisigScriptError;

pub struct MultisigScript {
    /// Number of required signatures
    pub required_signatures_number: u8,

    /// Public keys
    pub pubkeys: Vec<secp256k1::PublicKey>,
}

impl MultisigScript {
    pub fn new(required_signatures_number: u8, pubkeys: Vec<secp256k1::PublicKey>) -> Self {
        MultisigScript {
            required_signatures_number,
            pubkeys,
        }
    }

    /// NOTE: we assume that public keys are already sorted and compressed.
    pub fn to_script(&self) -> bitcoin::ScriptBuf {
        let mut builder = Builder::new().push_smallint(self.pubkeys.len() as u8);

        for pubkey in &self.pubkeys {
            builder = builder.push_slice(pubkey.serialize());
        }

        builder
            .push_smallint(self.required_signatures_number)
            .push_opcode(opcodes::all::OP_CHECKMULTISIG)
            .into_script()
    }

    /// Try to parse a multisig script from a bitcoin script.
    pub fn from_script(script: &Script) -> Result<Self, MultisigScriptError> {
        use MultisigScriptError as Error;

        let mut instructions = script.instructions();

        let pubkeys_number = instructions
            .next()
            .ok_or(Error::InvalidScript)?
            .map_err(|_| Error::InvalidScript)
            .and_then(instruction_to_smallint)?;

        let mut pubkeys = Vec::with_capacity(pubkeys_number as usize);

        for _ in 0..pubkeys_number {
            let instruction = instructions
                .next()
                .ok_or(Error::InvalidScript)?
                .map_err(|_| Error::InvalidScript)?;

            let Instruction::PushBytes(pubkey_bytes) = instruction else {
                return Err(Error::InvalidScript);
            };

            pubkeys.push(secp256k1::PublicKey::from_slice(pubkey_bytes.as_bytes())?);
        }

        let required_signatures_number = instructions
            .next()
            .ok_or(Error::InvalidScript)?
            .map_err(|_| Error::InvalidScript)
            .and_then(instruction_to_smallint)?;

        if instructions.next().is_some() {
            return Err(Error::InvalidScript);
        }

        Ok(MultisigScript {
            required_signatures_number,
            pubkeys,
        })
    }
}

impl From<MultisigScript> for bitcoin::ScriptBuf {
    fn from(multisig_script: MultisigScript) -> Self {
        multisig_script.to_script()
    }
}

impl TryFrom<bitcoin::ScriptBuf> for MultisigScript {
    type Error = MultisigScriptError;

    fn try_from(script: bitcoin::ScriptBuf) -> Result<Self, Self::Error> {
        MultisigScript::from_script(&script)
    }
}

/// Parse an instruction to a small integer.
fn instruction_to_smallint(instruction: Instruction) -> Result<u8, MultisigScriptError> {
    use MultisigScriptError as Error;

    const FROM_OPCODE: u8 = opcodes::all::OP_PUSHNUM_1.to_u8();
    const TO_OPCODE: u8 = opcodes::all::OP_PUSHNUM_16.to_u8();

    match instruction {
        Instruction::Op(op) => match op.to_u8() {
            FROM_OPCODE..=TO_OPCODE => Ok(op.to_u8() - FROM_OPCODE + 1),
            _ => Err(Error::InvalidScript),
        },
        Instruction::PushBytes(..) => Err(Error::InvalidScript),
    }
}

/// Utility trait that gives method for [`Builder`] to push small integer using
/// [`opcodes::all::OP_1`]..[`opcodes::all::OP_16`].
pub(crate) trait PushSmallInt {
    fn push_smallint(self, n: u8) -> Self;
}

impl PushSmallInt for Builder {
    fn push_smallint(self, n: u8) -> Self {
        match n {
            1..=16 => self.push_opcode((opcodes::all::OP_PUSHNUM_1.to_u8() - 1 + n).into()),
            _ => panic!("Can't push small int > 16"),
        }
    }
}
