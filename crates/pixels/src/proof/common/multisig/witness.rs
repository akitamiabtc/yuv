use crate::proof::p2wsh::witness::P2WSHWitness;
use alloc::vec::Vec;
use bitcoin::ecdsa;

pub type MultisigWitness = P2WSHWitness<Vec<ecdsa::Signature>>;
