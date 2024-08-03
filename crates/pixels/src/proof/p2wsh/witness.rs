use super::errors::P2WSHWitnessParseError;
use crate::alloc::string::ToString;
use alloc::vec::Vec;
use bitcoin::{ecdsa::Signature as EcdsaSig, ScriptBuf, Witness};

/// Parsed spending witness data of a P2WSH output.
pub struct P2WSHWitness<T> {
    /// The witness stack.
    pub stack: T,

    /// The redeem script.
    pub redeem_script: ScriptBuf,
}

impl<T> P2WSHWitness<T>
where
    T: FromWitnessStack,
{
    /// Parse the structure from a witness.
    pub fn from_witness(witness: &Witness) -> Result<Self, P2WSHWitnessParseError> {
        let Some(redeem_script_bytes) = witness.last() else {
            return Err(P2WSHWitnessParseError::EmptyWitness);
        };

        let redeem_script = decode_script_from_vec(redeem_script_bytes)?;

        let stack = witness
            .iter()
            .take(witness.len() - 1)
            .map(|x| x.to_vec())
            .collect::<Vec<_>>();

        let stack = T::from_witness_stack(&stack)?;

        Ok(Self {
            stack,
            redeem_script,
        })
    }
}

impl<T> P2WSHWitness<T>
where
    T: IntoWitnessStack,
{
    pub fn new(stack: T, redeem_script: ScriptBuf) -> Self {
        Self {
            stack,
            redeem_script,
        }
    }

    /// Serialize the structure into a witness.
    pub fn into_witness(self) -> Witness {
        let mut witness = self.stack.into_witness_stack();
        witness.push(self.redeem_script.to_bytes());

        Witness::from_slice(&witness)
    }
}

fn decode_script_from_vec(script_bytes: &[u8]) -> Result<ScriptBuf, P2WSHWitnessParseError> {
    let script = ScriptBuf::from_bytes(script_bytes.to_vec());

    Ok(script)
}

impl<T> TryFrom<&Witness> for P2WSHWitness<T>
where
    T: FromWitnessStack,
{
    type Error = P2WSHWitnessParseError;

    fn try_from(witness: &Witness) -> Result<Self, Self::Error> {
        P2WSHWitness::from_witness(witness)
    }
}

/// A trait for types that can be parsed from a witness stack.
pub trait FromWitnessStack {
    fn from_witness_stack(stack: &[Vec<u8>]) -> Result<Self, P2WSHWitnessParseError>
    where
        Self: Sized;
}

impl<T> FromWitnessStack for Vec<T>
where
    T: FromWitnessStackElement,
{
    fn from_witness_stack(stack: &[Vec<u8>]) -> Result<Self, P2WSHWitnessParseError> {
        stack
            .iter()
            .map(|x| T::from_witness_stack_element(x))
            .collect()
    }
}

pub trait IntoWitnessStack {
    fn into_witness_stack(self) -> Vec<Vec<u8>>;
}

impl<T> IntoWitnessStack for Vec<T>
where
    T: IntoStackElement,
{
    fn into_witness_stack(self) -> Vec<Vec<u8>> {
        self.into_iter().map(|x| x.into_stack_element()).collect()
    }
}

/// Trait for types that can be parsed from a witness stack element.
pub trait FromWitnessStackElement {
    /// Parse the structure from a witness stack element.
    fn from_witness_stack_element(element: &[u8]) -> Result<Self, P2WSHWitnessParseError>
    where
        Self: Sized;
}

/// Trait for types which could be converted to a witness stack element.
pub trait IntoStackElement {
    fn into_stack_element(self) -> Vec<u8>;
}

impl FromWitnessStackElement for Vec<u8> {
    fn from_witness_stack_element(element: &[u8]) -> Result<Self, P2WSHWitnessParseError> {
        Ok(element.to_vec())
    }
}

impl FromWitnessStackElement for EcdsaSig {
    fn from_witness_stack_element(element: &[u8]) -> Result<Self, P2WSHWitnessParseError> {
        EcdsaSig::from_slice(element)
            .map_err(|_| P2WSHWitnessParseError::Custom("Invalid signature".to_string()))
    }
}

impl IntoStackElement for Vec<u8> {
    fn into_stack_element(self) -> Vec<u8> {
        self
    }
}

impl IntoStackElement for EcdsaSig {
    fn into_stack_element(self) -> Vec<u8> {
        self.serialize().to_vec()
    }
}
