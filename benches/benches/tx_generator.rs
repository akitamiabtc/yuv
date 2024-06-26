use bitcoin::secp256k1::{rand::thread_rng, All, Message, PublicKey, Secp256k1, SecretKey};
use bitcoin::{
    blockdata::locktime::absolute::LockTime, ecdsa::Signature as EcdsaSig, OutPoint, ScriptBuf,
    Transaction, TxIn, TxOut, Txid,
};
use lazy_static::lazy_static;
use std::collections::BTreeMap;
use std::str::FromStr;
use yuv_pixels::{Chroma, Pixel, PixelKey, PixelProof};
use yuv_types::announcements::IssueAnnouncement;
use yuv_types::YuvTxType::Transfer;
use yuv_types::{announcements::FreezeAnnouncement, ProofMap, YuvTransaction, YuvTxType};

lazy_static! {
    static ref TXID: Txid = {
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap()
    };
}

lazy_static! {
    static ref MOCKED_PUB_KEY: PublicKey = {
        PublicKey::from_str("02e437e139d3e3d6d9784c137e778355da3f582125e289efe1e73b1d98bcdea9e8")
            .unwrap()
    };
}

/// Structure for generating random transactions.
///
/// Generates Issue, Transfer and Freeze txs with different public keys.
/// Also generates Transfer transactions without parent.
#[derive(Default)]
pub struct TxGenerator {
    /// Amount of generated txs
    tx_amount: u64,
    /// Ids of issue transaction (used for generating transfers)
    issue_tx_ids: Vec<Txid>,
    /// Current index of tx id to be used
    tx_ids_ind: usize,
    /// Counter that is used for generating txs with no parent
    no_parent_tx_counter: u64,
    /// Parent tx that waits for it`s turn to be sent
    parent_tx: Option<YuvTransaction>,
    /// Bitcoin curve to generate keypairs for transactions
    secp: Secp256k1<All>,
    /// Transfer transactions ids that are stored for freezing
    transfer_txs: Vec<Txid>,
    /// Current index of transaction for freezing
    freezing_tx_ind: usize,
}

impl TxGenerator {
    /// Generate random tx based on the current amount of transactions
    pub fn get_next_yuv_tx(&mut self) -> YuvTransaction {
        // Generate keypair
        let (seckey, pubkey) = self.secp.generate_keypair(&mut thread_rng());

        // Select type of the transaction
        let tx = match self.tx_amount % 5 {
            0 => {
                let tx = new_issuance_tx(pubkey, seckey);
                self.issue_tx_ids.push(tx.bitcoin_tx.txid());
                tx
            }
            1..=3 => {
                // Create different transfer tx
                match self.no_parent_tx_counter {
                    0 => {
                        // Create parent tx, store it and create child tx
                        let mut tx =
                            new_transfer_tx(pubkey, seckey, self.issue_tx_ids[self.tx_ids_ind]);
                        self.parent_tx = Some(tx.clone());
                        tx = new_transfer_tx(pubkey, seckey, tx.bitcoin_tx.txid());
                        self.no_parent_tx_counter += 1;
                        tx
                    }
                    1 => {
                        // Create tx to be frozen
                        let tx =
                            new_transfer_tx(pubkey, seckey, self.issue_tx_ids[self.tx_ids_ind]);
                        self.no_parent_tx_counter += 1;
                        self.transfer_txs.push(tx.bitcoin_tx.txid());
                        tx
                    }
                    2 => {
                        // Create tx that uses frozen tx as an input
                        let tx = new_transfer_tx(
                            pubkey,
                            seckey,
                            self.transfer_txs[self.freezing_tx_ind],
                        );

                        self.freezing_tx_ind += 1;
                        self.no_parent_tx_counter += 1;
                        tx
                    }
                    3 => {
                        // Get parent tx
                        let tx = self.parent_tx.clone().unwrap();
                        self.no_parent_tx_counter += 1;
                        self.parent_tx = None;
                        tx
                    }
                    _ => {
                        // Dummy tx
                        self.no_parent_tx_counter = 0;
                        let mut dummy_tx = self.get_rand_transfer();
                        add_witness(&mut dummy_tx, seckey, pubkey, *TXID, 0);
                        dummy_tx.tx_type = Transfer {
                            input_proofs: new_proof(vec![new_pixel_proof(*MOCKED_PUB_KEY, 100)]),
                            output_proofs: Default::default(),
                        };
                        dummy_tx
                    }
                }
            }
            _ => {
                if self.no_parent_tx_counter - 1 == 1 {
                    new_freeze_tx(pubkey, self.transfer_txs[self.freezing_tx_ind])
                } else {
                    new_freeze_tx(
                        pubkey,
                        Txid::from_str(
                            "0000000000000000000000000000000000000000000000000000000000000000",
                        )
                        .unwrap(),
                    )
                }
            }
        };
        self.tx_amount += 1;
        tx
    }

    /// Yuv transfer transaction with random public key
    pub fn get_rand_transfer(&self) -> YuvTransaction {
        let (seckey, pubkey) = self.secp.generate_keypair(&mut thread_rng());

        new_transfer_tx(pubkey, seckey, self.issue_tx_ids[self.tx_ids_ind])
    }
}

/// Add witness to the transaction
fn add_witness(
    tx: &mut YuvTransaction,
    seckey: SecretKey,
    pubkey: PublicKey,
    txid: Txid,
    vin: usize,
) {
    let secp = Secp256k1::new();

    let mes = Message::from_slice(txid.as_ref()).unwrap();

    let ecdsasig = EcdsaSig::sighash_all(secp.sign_ecdsa(&mes, &seckey));
    tx.bitcoin_tx.input[vin].witness.push(ecdsasig.serialize());
    tx.bitcoin_tx.input[vin].witness.push(pubkey.serialize());
}

/// Generate random issuance tx
fn new_issuance_tx(pubkey: PublicKey, seckkey: SecretKey) -> YuvTransaction {
    let proofs = vec![new_pixel_proof(pubkey, 100)];
    let announcement = IssueAnnouncement::new(Chroma::from(pubkey.x_only_public_key().0), 100);
    let mut tx = new_yuv_tx(
        vec![TxIn::default()],
        vec![TxOut {
            value: 10000,
            script_pubkey: new_pixel_script_pub_key(pubkey, 100),
        }],
        YuvTxType::Issue {
            output_proofs: Some(new_proof(proofs)),
            announcement,
        },
    );

    add_witness(&mut tx, seckkey, pubkey, *TXID, 0);

    tx
}

/// Generate random transfer tx that is a child of some parent tx
fn new_transfer_tx(pubkey: PublicKey, seckey: SecretKey, txid: Txid) -> YuvTransaction {
    let tx_inp_proof = new_pixel_proof(pubkey, 100);
    let mut tx = new_yuv_tx(
        vec![TxIn {
            previous_output: OutPoint::new(txid, 0),
            script_sig: Default::default(),
            sequence: Default::default(),
            witness: Default::default(),
        }],
        vec![TxOut {
            value: 10000,
            script_pubkey: new_pixel_script_pub_key(pubkey, 100),
        }],
        Transfer {
            input_proofs: new_proof(vec![tx_inp_proof.clone()]),
            output_proofs: new_proof(vec![new_pixel_proof(pubkey, 100)]),
        },
    );

    let pixel_key = PixelKey::new(tx_inp_proof.pixel(), &pubkey).unwrap();
    add_witness(&mut tx, seckey, pixel_key.0.inner, txid, 0);

    tx
}

/// Generate random freeze tx
fn new_freeze_tx(pubkey: PublicKey, txid_for_freeze: Txid) -> YuvTransaction {
    let freeze_announcement = FreezeAnnouncement::new(OutPoint::new(txid_for_freeze, 0)).into();
    new_yuv_tx(
        vec![TxIn::default()],
        vec![TxOut {
            value: 10000,
            script_pubkey: new_pixel_script_pub_key(pubkey, 100),
        }],
        YuvTxType::Announcement(freeze_announcement),
    )
}

/// Create new yuv transaction
pub fn new_yuv_tx(inputs: Vec<TxIn>, outputs: Vec<TxOut>, tx_type: YuvTxType) -> YuvTransaction {
    YuvTransaction {
        bitcoin_tx: Transaction {
            version: 0,
            lock_time: LockTime::from_height(0).expect("failed to create lock time"),
            input: inputs,
            output: outputs,
        },
        tx_type,
    }
}

/// Generate new proof
pub fn new_proof(proofs: Vec<PixelProof>) -> ProofMap {
    let mut map = BTreeMap::new();

    for (ind, proof) in proofs.iter().enumerate() {
        map.insert(ind as u32, proof.clone());
    }

    map
}

/// Generate new pixel script pub key
pub fn new_pixel_script_pub_key(pubkey: PublicKey, amount: u128) -> ScriptBuf {
    let (xonly, _parity) = pubkey.x_only_public_key();

    let pixel = Pixel::new(amount, xonly);
    let pixel_key = PixelKey::new(pixel, &pubkey).expect("failed to create PixelKey");
    pixel_key.to_p2wpkh().expect("failed to create script")
}

/// Generate new pixel proof
pub fn new_pixel_proof(pubkey: PublicKey, amount: u128) -> PixelProof {
    let (xonly, _parity) = pubkey.x_only_public_key();

    PixelProof::sig(Pixel::new(amount, xonly), pubkey)
}
