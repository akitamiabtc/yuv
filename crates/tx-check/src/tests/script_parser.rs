use std::str::FromStr;

use bitcoin::{
    hashes::{hash160, sha256, Hash},
    opcodes::{all::OP_PUSHBYTES_71, OP_0},
    script::Builder,
    PublicKey, ScriptBuf, TxIn, WPubkeyHash, WScriptHash, Witness,
};
use once_cell::sync::Lazy;

use crate::script_parser::{ScriptParseError, SpendingCondition};

static PUBKEY: Lazy<PublicKey> = Lazy::new(|| {
    PublicKey::from_str("02a1f1ad0fe384b05504f8233209bad9e396f3f86b591e877dc1f95394306d9b94")
        .expect("valid public key")
});

fn create_p2wpkh_script_pub_key(pubkey: &PublicKey) -> ScriptBuf {
    let pubkey_hash = hash160::Hash::hash(&pubkey.to_bytes());
    ScriptBuf::new_v0_p2wpkh(&WPubkeyHash::from_raw_hash(pubkey_hash))
}

fn create_p2wsh_script_pub_key(pubkey: &PublicKey) -> ScriptBuf {
    let redeem_script = Builder::new().push_key(pubkey).into_script();
    let script_hash = sha256::Hash::hash(&redeem_script.to_bytes());
    ScriptBuf::new_v0_p2wsh(&WScriptHash::from_raw_hash(script_hash))
}

#[test]
fn test_from_txin_p2pkh() {
    let script_pub_key =
        ScriptBuf::from_hex("76a91455ae51684c43435da751ac8d2173b2652eb6410588ac").unwrap();
    let script_sig = ScriptBuf::from_hex("483045022100c233c3a8a510e03ad18b0a24694ef00c78101bfd5ac075b8c1037952ce26e91e02205aa5f8f88f29bb4ad5808ebc12abfd26bd791256f367b04c6d955f01f28a7724012103f0609c81a45f8cab67fc2d050c21b1acd3d37c7acfd54041be6601ab4cef4f31").unwrap();

    let txin = TxIn {
        script_sig: script_sig.clone(),
        ..Default::default()
    };

    let spending_condition = SpendingCondition::from_txin(&txin, bitcoin::AddressType::P2pkh)
        .expect("Failed to derive P2PKH scriptPubKey");

    assert_eq!(spending_condition.into_script(), script_pub_key);
}

#[test]
fn test_from_txin_p2sh() {
    let script_pub_key =
        ScriptBuf::from_hex("a914748284390f9e263a4b766a75d0633c50426eb87587").unwrap();
    let script_sig = ScriptBuf::from_hex("00473044022100d0ed946330182916da16a6149cd313a4b1a7b41591ee52fb3e79d64e36139d66021f6ccf173040ef24cb45c4db3e9c771c938a1ba2cf8d2404416f70886e360af401475121022afc20bf379bc96a2f4e9e63ffceb8652b2b6a097f63fbee6ecec2a49a48010e2103a767c7221e9f15f870f1ad9311f5ab937d79fcaeee15bb2c722bca515581b4c052ae").unwrap();

    let txin = TxIn {
        script_sig: script_sig.clone(),
        ..Default::default()
    };

    let spending_condition = SpendingCondition::from_txin(&txin, bitcoin::AddressType::P2sh)
        .expect("Failed to derive P2SH scriptPubKey");

    assert_eq!(spending_condition.into_script(), script_pub_key);
}

#[test]
fn test_from_txin_p2wpkh() {
    let script_pub_key = create_p2wpkh_script_pub_key(&PUBKEY);
    let witness = Witness::from_slice(&[vec![], PUBKEY.to_bytes()]);

    let txin = TxIn {
        witness: witness.clone(),
        ..Default::default()
    };

    let spending_condition = SpendingCondition::from_txin(&txin, bitcoin::AddressType::P2wpkh)
        .expect("Failed to derive P2WPKH scriptPubKey");

    assert_eq!(spending_condition.into_script(), script_pub_key);
}

#[test]
fn test_from_txin_p2wsh() {
    let redeem_script = Builder::new().push_key(&PUBKEY).into_script();
    let script_pub_key = create_p2wsh_script_pub_key(&PUBKEY);

    let witness = Witness::from_slice(&[vec![], redeem_script.to_bytes()]);

    let txin = TxIn {
        witness: witness.clone(),
        ..Default::default()
    };

    let spending_condition = SpendingCondition::from_txin(&txin, bitcoin::AddressType::P2wsh)
        .expect("Failed to derive P2WSH scriptPubKey");

    assert_eq!(spending_condition.into_script(), script_pub_key);
}

#[test]
fn parse_invalid_scripts() {
    struct TestData {
        txin: TxIn,
        err: ScriptParseError,
    }

    let inputs = vec![
        TestData {
            txin: TxIn {
                script_sig: ScriptBuf::from_bytes(Vec::from([1, 2, 3, 4])),
                ..Default::default()
            },
            err: ScriptParseError::InvalidInstruction(bitcoin::script::Error::EarlyEndOfScript),
        },
        TestData {
            txin: TxIn {
                script_sig: ScriptBuf::from_bytes(Vec::new()),
                ..Default::default()
            },
            err: ScriptParseError::InsufficientInstructions(0),
        },
        TestData {
            txin: TxIn {
                script_sig: Builder::new()
                    .push_opcode(OP_0)
                    .push_opcode(OP_PUSHBYTES_71)
                    .into_script(),
                ..Default::default()
            },
            err: ScriptParseError::InvalidInstruction(bitcoin::script::Error::EarlyEndOfScript),
        },
    ];

    for input in inputs {
        match SpendingCondition::from_txin(&input.txin, bitcoin::AddressType::P2pkh) {
            Err(err) => {
                assert_eq!(err, input.err);
            }
            err => {
                panic!("Unexpected result: {:?}", err);
            }
        }
    }
}
