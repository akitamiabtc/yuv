use std::{
    collections::{BTreeMap, HashMap},
    mem,
    sync::{Arc, RwLock},
};

use bitcoin::{
    key::XOnlyPublicKey,
    psbt,
    secp256k1::{self, All, Secp256k1},
    OutPoint, PrivateKey, PublicKey, ScriptBuf, Transaction, TxOut,
};
use eyre::{bail, eyre, Context, OptionExt};
#[cfg(feature = "bulletproof")]
use {
    bitcoin::secp256k1::schnorr::Signature,
    yuv_pixels::{k256::ProjectivePoint, Luma, RangeProof},
    yuv_types::is_bulletproof,
};

use bdk::{
    blockchain::Blockchain,
    descriptor,
    miniscript::{psbt::PsbtInputExt, Descriptor, DescriptorPublicKey, ToPublicKey},
    wallet::tx_builder::TxOrdering,
    FeeRate as BdkFeeRate, SignOptions,
};

use yuv_pixels::{
    Chroma, EmptyPixelProof, MultisigPixelProof, Pixel, PixelKey, PixelProof, SigPixelProof,
    ToEvenPublicKey,
};

use yuv_storage::TransactionsStorage as YuvTransactionsStorage;
use yuv_types::{announcements::IssueAnnouncement, AnyAnnouncement};
use yuv_types::{ProofMap, YuvTransaction, YuvTxType};

use crate::{
    bitcoin_provider::BitcoinProvider,
    txsigner::TransactionSigner,
    types::{FeeRateStrategy, Utxo, WeightedUtxo, YuvTxOut, YuvUtxo},
    yuv_coin_selection::{YUVCoinSelectionAlgorithm, YuvLargestFirstCoinSelection},
    Wallet,
};

#[cfg(feature = "bulletproof")]
mod bulletproof;
#[cfg(feature = "bulletproof")]
pub use bulletproof::BulletproofRecipientParameters;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum BuilderInput {
    Multisig2x2 {
        outpoint: OutPoint,
        second_signer_key: PrivateKey,
    },
    Pixel {
        outpoint: OutPoint,
    },
    TweakedSatoshis {
        outpoint: OutPoint,
    },
    #[cfg(feature = "bulletproof")]
    BulletproofPixel {
        outpoint: OutPoint,
    },
}

impl BuilderInput {
    fn outpoint(&self) -> OutPoint {
        match self {
            BuilderInput::Multisig2x2 { outpoint, .. }
            | BuilderInput::Pixel { outpoint }
            | BuilderInput::TweakedSatoshis { outpoint } => *outpoint,
            #[cfg(feature = "bulletproof")]
            BuilderInput::BulletproofPixel { outpoint, .. } => *outpoint,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum BuilderOutput {
    Satoshis {
        satoshis: u64,
        recipient: secp256k1::PublicKey,
    },
    Pixel {
        chroma: Chroma,
        satoshis: u64,
        amount: u128,
        recipient: secp256k1::PublicKey,
    },
    MultisigPixel {
        chroma: Chroma,
        satoshis: u64,
        amount: u128,
        participants: Vec<secp256k1::PublicKey>,
        required_signatures: u8,
    },
    #[cfg(feature = "bulletproof")]
    BulletproofPixel {
        chroma: Chroma,
        recipient: PublicKey,
        sender: PublicKey,
        luma: Luma,
        satoshis: u64,
        commitment: ProjectivePoint,
        proof: RangeProof,
        signature: Signature,
        chroma_signature: Signature,
    },
}

impl BuilderOutput {
    fn amount(&self) -> u128 {
        match self {
            BuilderOutput::Satoshis { .. } => 0,
            BuilderOutput::Pixel { amount, .. } | BuilderOutput::MultisigPixel { amount, .. } => {
                *amount
            }
            #[cfg(feature = "bulletproof")]
            BuilderOutput::BulletproofPixel { .. } => 0,
        }
    }

    fn chroma(&self) -> Option<Chroma> {
        match self {
            BuilderOutput::Satoshis { .. } => None,
            BuilderOutput::Pixel { chroma, .. } => Some(*chroma),
            BuilderOutput::MultisigPixel { chroma, .. } => Some(*chroma),
            #[cfg(feature = "bulletproof")]
            BuilderOutput::BulletproofPixel { chroma, .. } => Some(*chroma),
        }
    }
}

struct TransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase> {
    /// Defines if the transactions is issuance or not.
    ///
    /// By that [`TransactionBuilder`] will consider to whether add or not the
    /// inputs with YUV coins to satisfy consideration rules.
    is_issuance: bool,

    /// [`Chromas`]s of current transactions.
    chromas: Vec<Chroma>,

    /// Value of satoshis that will be attached to change output for YUV coins.
    change_satoshis: u64,

    /// The fee rate strategy. Possible values:
    /// - Estimate: The fee rate is fetched from Bitcoin RPC. If an error
    ///   occurs, the tx building process is interrupted.
    /// - Manual: Default fee rate is used.
    /// - TryEstimate: The fee rate is fetched
    /// automatically from Bitcoin RPC. If an error occurs, the default fee rate is used.
    /// NOTE: fee_rate is measured in sat/vb.
    fee_rate_strategy: FeeRateStrategy,

    yuv_txs_storage: YuvTxsDatabase,

    /// Inner wallet which will sign result transaction.
    inner_wallet: Arc<RwLock<bdk::Wallet<BitcoinTxsDatabase>>>,
    private_key: PrivateKey,

    /// Storage of transactions outputs that could be spent
    yuv_utxos: Arc<RwLock<HashMap<OutPoint, PixelProof>>>,

    /// Storage of outputs which will be formed into transaction outputs and
    /// proofs.
    outputs: Vec<BuilderOutput>,

    /// Storage of bulletproof outputs that will be mapped to `self.outputs` and then into transaction outputs and
    /// proofs.
    ///
    /// `OutPoint` is an `Option` as it may be absent in case the transaction is an issuance.
    #[cfg(feature = "bulletproof")]
    bulletproof_outputs:
        BTreeMap<Option<OutPoint>, Vec<(Chroma, bulletproof::BulletproofRecipientParameters)>>,

    /// Storage of inputs which will be formed into transaction inputs and
    /// proofs.
    inputs: Vec<BuilderInput>,

    /// Transaction Signer is responsible for signing the transaction.
    tx_signer: TransactionSigner,

    /// Indicated if inputs were selected by user.
    is_inputs_selected: bool,

    /// Instructs txbuilder to add tweaked satoshis as transaction inputs
    should_drain_tweaked_satoshis: bool,
}

unsafe impl<YuvTxsDatabase, BitcoinTxsDatabase> Sync
    for TransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>
where
    YuvTxsDatabase: Sync,
    BitcoinTxsDatabase: Sync,
{
}

unsafe impl<YuvTxsDatabase, BitcoinTxsDatabase> Send
    for TransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>
where
    YuvTxsDatabase: Send,
    BitcoinTxsDatabase: Send,
{
}

pub struct SweepTransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>(
    TransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>,
);

impl<YTDB, BDB, YC, BP> TryFrom<&Wallet<YC, YTDB, BP, BDB>> for SweepTransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
    BP: BitcoinProvider,
{
    type Error = eyre::Error;

    fn try_from(wallet: &Wallet<YC, YTDB, BP, BDB>) -> Result<Self, Self::Error> {
        Ok(Self(TransactionBuilder::new(true, wallet)?))
    }
}

impl<YTDB, BDB> SweepTransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
{
    /// Override the fee rate strategy.
    pub fn set_fee_rate_strategy(&mut self, fee_rate_strategy: FeeRateStrategy) -> &mut Self {
        self.0.set_fee_rate_strategy(fee_rate_strategy);

        self
    }

    /// Finish sweep building, and create a Bitcoin transaction.
    /// If the address has no tweaked Bitcoin outputs, `None` is returned.
    pub async fn finish(self, blockchain: &impl Blockchain) -> eyre::Result<Option<Transaction>> {
        self.0.build_sweep(blockchain).await
    }
}

pub struct IssuanceTransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase> {
    tx_builder: TransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>,
    chroma: Chroma,
}

impl<YTDB, BDB> IssuanceTransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
{
    pub fn new<BP: BitcoinProvider, YC>(
        wallet: &Wallet<YC, YTDB, BP, BDB>,
        chroma: Option<Chroma>,
    ) -> eyre::Result<Self> {
        let tx_builder = TransactionBuilder::new(true, wallet)?;
        let chroma = chroma.unwrap_or(tx_builder.issuance_chroma());
        Ok(Self { tx_builder, chroma })
    }
}

impl<YTDB, BDB> IssuanceTransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
{
    /// Add recipient to the transaction.
    pub fn add_recipient(
        &mut self,
        recipient: &secp256k1::PublicKey,
        amount: u128,
        satoshis: u64,
    ) -> &mut Self {
        self.tx_builder.outputs.push(BuilderOutput::Pixel {
            chroma: self.chroma,
            satoshis,
            amount,
            recipient: *recipient,
        });

        self
    }

    /// Override the fee rate strategy.
    pub fn set_fee_rate_strategy(&mut self, fee_rate_strategy: FeeRateStrategy) -> &mut Self {
        self.tx_builder.set_fee_rate_strategy(fee_rate_strategy);

        self
    }

    // Override spending tweaked satoshis
    pub fn set_drain_tweaked_satoshis(&mut self, should_drain_tweaked_satoshis: bool) -> &mut Self {
        self.tx_builder.should_drain_tweaked_satoshis = should_drain_tweaked_satoshis;
        self
    }

    /// Add satoshi recipient.
    pub fn add_sats_recipient(
        &mut self,
        recipient: &secp256k1::PublicKey,
        satoshis: u64,
    ) -> &mut Self {
        self.tx_builder.add_sats_recipient(recipient, satoshis);

        self
    }

    /// Add multisig recipient to the transaction.
    ///
    /// The transaction output will be formed as P2WSH output with
    /// multisignature script, that has tweaked first key.
    pub fn add_multisig_recipient(
        &mut self,
        participants: Vec<secp256k1::PublicKey>,
        required_signatures: u8,
        amount: u128,
        satoshis: u64,
    ) -> &mut Self {
        self.tx_builder.add_multisig_recipient(
            participants,
            required_signatures,
            amount,
            self.chroma,
            satoshis,
        );

        self
    }

    /// Finish issuance building, and create Bitcoin transactions with attached
    /// proofs for it in [`YuvTransaction`].
    pub async fn finish(self, blockchain: &impl Blockchain) -> eyre::Result<YuvTransaction> {
        self.tx_builder.finish(blockchain).await
    }
}

pub struct TransferTransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>(
    TransactionBuilder<YuvTxsDatabase, BitcoinTxsDatabase>,
);

impl<YTDB, BDB, YC, BP> TryFrom<&Wallet<YC, YTDB, BP, BDB>>
    for TransferTransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
    BP: BitcoinProvider,
{
    type Error = eyre::Error;

    fn try_from(wallet: &Wallet<YC, YTDB, BP, BDB>) -> Result<Self, Self::Error> {
        Ok(Self(TransactionBuilder::new(false, wallet)?))
    }
}

impl<YTDB, BDB> TransferTransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
{
    /// Add recipient to the transaction.
    pub fn add_recipient(
        &mut self,
        chroma: Chroma,
        recipient: &secp256k1::PublicKey,
        amount: u128,
        satoshis: u64,
    ) -> &mut Self {
        self.0.outputs.push(BuilderOutput::Pixel {
            chroma,
            satoshis,
            amount,
            recipient: *recipient,
        });

        self.0.chromas.push(chroma);

        self
    }

    /// Override the fee rate strategy.
    pub fn set_fee_rate_strategy(&mut self, fee_rate_strategy: FeeRateStrategy) -> &mut Self {
        self.0.fee_rate_strategy = fee_rate_strategy;
        self
    }

    // Override spending tweaked satoshis
    pub fn set_drain_tweaked_satoshis(&mut self, should_drain_tweaked_satoshis: bool) -> &mut Self {
        self.0.should_drain_tweaked_satoshis = should_drain_tweaked_satoshis;
        self
    }

    /// Add satoshi recipient.
    pub fn add_sats_recipient(
        &mut self,
        recipient: &secp256k1::PublicKey,
        satoshis: u64,
    ) -> &mut Self {
        self.0.add_sats_recipient(recipient, satoshis);

        self
    }

    /// Add a 2x2 multisignature input.
    pub fn add_2x2multisig_input(
        &mut self,
        outpoint: OutPoint,
        spender_key2: PrivateKey,
    ) -> &mut Self {
        self.0.add_2x2multisig_input(outpoint, spender_key2);

        self
    }

    /// Add multisig recipient to the transaction.
    ///
    /// The transaction output will be formed as P2WSH output with
    /// multisignature script, that has tweaked first key.
    pub fn add_multisig_recipient(
        &mut self,
        participants: Vec<secp256k1::PublicKey>,
        required_signatures: u8,
        amount: u128,
        chroma: Chroma,
        satoshis: u64,
    ) -> &mut Self {
        self.0
            .add_multisig_recipient(participants, required_signatures, amount, chroma, satoshis);

        self
    }

    /// Set flag that only selected inputs will be used for transaction
    pub fn manual_selected_only(&mut self) {
        self.0.manual_selected_only();
    }

    /// Set amount of satoshis that will be given to residual output for YUV coins.
    pub fn set_change_satoshis(&mut self, satoshis: u64) -> &mut Self {
        self.0.set_change_satoshis(satoshis);

        self
    }

    /// Add pixel input to the transaction with given outpoint.
    pub fn add_pixel_input(&mut self, outpoint: OutPoint) -> &mut Self {
        self.0.add_pixel_input(outpoint);

        self
    }

    /// Finish transfer building, and create Bitcoin transactions with attached
    /// proofs for it in [`YuvTransaction`].
    pub async fn finish(self, blockchain: &impl Blockchain) -> eyre::Result<YuvTransaction> {
        self.0.finish(blockchain).await
    }
}

impl<YTDB, BDB> TransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
{
    fn new<YC, BC>(is_issuance: bool, wallet: &Wallet<YC, YTDB, BC, BDB>) -> eyre::Result<Self> {
        let bitcoin_wallet = wallet.bitcoin_wallet.clone();

        let ctx = { bitcoin_wallet.read().unwrap().secp_ctx().clone() };

        Ok(Self {
            is_issuance,
            chromas: Vec::new(),
            change_satoshis: 1000,
            fee_rate_strategy: FeeRateStrategy::default(),
            inner_wallet: bitcoin_wallet,
            private_key: wallet.signer_key,
            yuv_txs_storage: wallet.yuv_txs_storage.clone(),
            yuv_utxos: wallet.utxos.clone(),
            outputs: Vec::new(),
            #[cfg(feature = "bulletproof")]
            bulletproof_outputs: BTreeMap::new(),
            inputs: Vec::new(),
            tx_signer: TransactionSigner::new(ctx, wallet.signer_key),
            is_inputs_selected: false,
            should_drain_tweaked_satoshis: false,
        })
    }
}

impl<YTDB, BDB> TransactionBuilder<YTDB, BDB>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
    BDB: bdk::database::BatchDatabase + Clone + Send,
{
    fn add_sats_recipient(&mut self, recipient: &secp256k1::PublicKey, satoshis: u64) -> &mut Self {
        self.outputs.push(BuilderOutput::Satoshis {
            satoshis,
            recipient: *recipient,
        });

        self
    }

    /// Add 2 from 2 multsig input to the transaction with given outpoint.
    ///
    /// The proof will be taken from synced YUV transactions.
    fn add_2x2multisig_input(&mut self, outpoint: OutPoint, spender_key2: PrivateKey) -> &mut Self {
        self.inputs.push(BuilderInput::Multisig2x2 {
            outpoint,
            second_signer_key: spender_key2,
        });

        self
    }

    /// Add multisig recipient to the transaction.
    ///
    /// The transaction output will be formed as P2WSH output with
    /// multisignature script, that has tweaked first key.
    pub fn add_multisig_recipient(
        &mut self,
        participants: Vec<secp256k1::PublicKey>,
        required_signatures: u8,
        amount: u128,
        chroma: Chroma,
        satoshis: u64,
    ) -> &mut Self {
        debug_assert!(
            participants.len() > 1 && participants.len() < 16,
            "Invalid number of participants"
        );
        self.outputs.push(BuilderOutput::MultisigPixel {
            chroma,
            satoshis,
            amount,
            required_signatures,
            participants,
        });

        self.chromas.push(chroma);

        self
    }

    /// Add pixel input to the transaction with given outpoint.
    fn add_pixel_input(&mut self, outpoint: OutPoint) -> &mut Self {
        self.inputs.push(BuilderInput::Pixel { outpoint });
        self
    }

    fn add_tweaked_satoshi_inputs(&mut self) {
        let tweaked_outputs = self
            .yuv_utxos
            .read()
            .unwrap()
            .iter()
            .filter_map(|(outpoint, proof)| {
                if proof.is_empty_pixelproof() {
                    Some(*outpoint)
                } else {
                    None
                }
            })
            .collect::<Vec<OutPoint>>();

        for output in tweaked_outputs {
            self.inputs
                .push(BuilderInput::TweakedSatoshis { outpoint: output });
        }
    }

    /// Set amount of satoshis that will be given to residual output for YUV coins.
    fn set_change_satoshis(&mut self, satoshis: u64) -> &mut Self {
        self.change_satoshis = satoshis;
        self
    }

    /// Override the fee rate strategy.
    fn set_fee_rate_strategy(&mut self, fee_rate_strategy: FeeRateStrategy) -> &mut Self {
        self.fee_rate_strategy = fee_rate_strategy;
        self
    }

    fn issuance_chroma(&self) -> Chroma {
        self.private_key
            .public_key(&Secp256k1::new())
            .to_x_only_pubkey()
            .into()
    }

    // === Finish transaction building ===
    async fn finish(mut self, blockchain: &impl Blockchain) -> eyre::Result<YuvTransaction> {
        let fee_rate = self
            .fee_rate_strategy
            .get_fee_rate(blockchain)
            .wrap_err("failed to estimate fee")?;

        if !self.is_inputs_selected {
            if self.should_drain_tweaked_satoshis {
                self.add_tweaked_satoshi_inputs();
            }
            if !self.is_issuance {
                for chroma in &self.chromas.clone() {
                    self.fill_missing_amount(*chroma).await?;
                }
            }
        }

        self.build_tx(fee_rate).await
    }

    /// Fill [`Self::inputs`] with missing utxos that will be used to satisfy
    /// sum in [`Self::outputs`].
    ///
    /// Also will add to [`Self::outputs`] self-recipient for residual YUV coins
    /// if need so.
    async fn fill_missing_amount(&mut self, chroma: Chroma) -> eyre::Result<()> {
        let output_sum = self
            .outputs
            .iter()
            .filter(|output| output.chroma() == Some(chroma))
            .map(|output| output.amount())
            .sum::<u128>();

        let input_sum = self.inputs_sum(chroma).await?;

        // No work is required if sum of inputs is equal to sum of outputs
        if input_sum == output_sum {
            return Ok(());
        }

        // If sum of inputs is greater than sum of outputs, then we need to
        // add self-recipient for residual amount.
        if input_sum > output_sum {
            let residual_amount = input_sum.saturating_sub(output_sum);

            // If remaining amount is not zero, add self-recipient
            self.add_change_output(chroma, residual_amount)?;

            return Ok(());
        }

        // Otherwise, we need to add inputs to satisfy sum of outputs
        let required_utxos = self
            .form_weighted_utxos(
                self.inputs.iter().map(BuilderInput::outpoint).collect(),
                chroma,
            )
            .await?;

        let optional_utxos = {
            let outpoints = {
                let yuv_utxos = self.yuv_utxos.read().unwrap();

                yuv_utxos.keys().cloned().collect()
            };

            self.form_weighted_utxos(outpoints, chroma).await?
        };

        let target_amount = output_sum.saturating_sub(input_sum);

        debug_assert!(target_amount > 0, "Target amount is zero");

        let selection_result = YuvLargestFirstCoinSelection.coin_select(
            required_utxos,
            optional_utxos,
            target_amount,
            &ScriptBuf::new(),
            chroma,
        )?;

        for selected in selection_result.selected {
            // Here we are sure, that selected utxo is single-sig pixel
            self.inputs.push(BuilderInput::Pixel {
                outpoint: selected.outpoint(),
            });
        }

        let filled_input_sum = input_sum + selection_result.amount;

        if filled_input_sum < output_sum {
            bail!(
                "Insufficient balance: inputs sum: {} output sum: {}",
                filled_input_sum,
                output_sum
            );
        }

        let change_amount = filled_input_sum.saturating_sub(output_sum);

        // If remaining amount is not zero, add self-recipient
        if change_amount > 0 {
            self.add_change_output(chroma, change_amount)?;
        }

        Ok(())
    }

    fn add_change_output(&mut self, chroma: Chroma, residual_amount: u128) -> eyre::Result<()> {
        debug_assert!(residual_amount > 0, "Residual amount is zero");

        let ctx = Secp256k1::new();

        self.outputs.push(BuilderOutput::Pixel {
            chroma,
            satoshis: self.change_satoshis,
            amount: residual_amount,
            recipient: self.private_key.public_key(&ctx).inner,
        });

        Ok(())
    }

    async fn inputs_sum(&self, chroma: Chroma) -> eyre::Result<u128> {
        let mut sum = 0u128;

        for input in &self.inputs {
            let (proof, _output) =
                get_output_from_storage(&self.yuv_txs_storage, input.outpoint()).await?;
            let pixel = proof.pixel();

            if pixel.chroma != chroma {
                continue;
            }

            sum = sum
                .checked_add(pixel.luma.amount)
                .ok_or_eyre("Inputs sum overflow")?;
        }

        Ok(sum)
    }

    /// Form [`WeightedUtxo`] for YUV coins from given [`OutPoint`]s from
    /// unspent transaction outputs.
    async fn form_weighted_utxos(
        &self,
        utxos: Vec<OutPoint>,
        chroma: Chroma,
    ) -> eyre::Result<Vec<WeightedUtxo>> {
        let mut weighted_utxos = Vec::new();

        for outpoint in utxos {
            let (proof, output) = get_output_from_storage(&self.yuv_txs_storage, outpoint).await?;
            let pixel = proof.pixel();

            #[cfg(feature = "bulletproof")]
            if proof.is_bulletproof() {
                continue;
            }

            if pixel.chroma != chroma {
                continue;
            }

            let weighted_utxo = WeightedUtxo {
                satisfaction_weight: 0, // FIXME: calculate weight
                utxo: Utxo::Yuv(YuvUtxo {
                    outpoint,
                    txout: YuvTxOut {
                        satoshis: output.value,
                        script_pubkey: output.script_pubkey,
                        pixel,
                    },
                    keychain: crate::types::KeychainKind::External,
                    is_spent: false,
                    derivation_index: 0,
                    confirmation_time: None,
                }),
            };

            weighted_utxos.push(weighted_utxo);
        }

        Ok(weighted_utxos)
    }

    /// Set flag that only selected inputs will be used for transaction
    fn manual_selected_only(&mut self) {
        self.is_inputs_selected = true;
    }

    /// Inserts empty pixel proofs to the outputs that don't hold any Pixel data,
    /// i.e. to the Satoshis only outputs.
    ///
    /// The output `script_pubkey` is also tweaked with an empty pixel, so the method
    /// creates wrapped satoshis that can be spent after sweeping them to a p2wpkh address.
    fn insert_empty_pixelproofs(
        &self,
        output_proofs: &mut Vec<PixelProof>,
        tx_outs: &mut [TxOut],
    ) -> eyre::Result<()> {
        let ctx = Secp256k1::new();

        // If the tx is an issuance, the first output is `OP_RETURN`, so the offset should be increased.
        let offset = if self.is_issuance {
            output_proofs.len() + 1
        } else {
            output_proofs.len()
        };

        tx_outs.iter_mut().skip(offset).for_each(|tx_out| {
            let (pixel_proof, script_pubkey) = get_empty_pixel_proof(
                self.private_key
                    .public_key(&ctx)
                    .even_public_key(&ctx)
                    .inner,
            )
            .expect("Failed to get empty pixelproof");

            output_proofs.push(pixel_proof);
            tx_out.script_pubkey = script_pubkey;
        });

        Ok(())
    }

    async fn build_sweep(
        mut self,
        blockchain: &impl Blockchain,
    ) -> eyre::Result<Option<Transaction>> {
        let fee_rate = self
            .fee_rate_strategy
            .get_fee_rate(blockchain)
            .wrap_err("failed to estimate fee")?;
        let ctx = Secp256k1::new();

        // Get the tweaked UTXOs.
        let mut tweaked_outputs = self
            .yuv_utxos
            .read()
            .unwrap()
            .iter()
            .filter(|utxo| utxo.1.is_empty_pixelproof())
            .map(|(outpoint, proof)| (*outpoint, proof.clone()))
            .collect::<HashMap<OutPoint, PixelProof>>();

        // If there are no tweaked UTXOs, then exit.
        if tweaked_outputs.is_empty() {
            return Ok(None);
        }

        for outpoint in tweaked_outputs.keys() {
            self.inputs.push(BuilderInput::TweakedSatoshis {
                outpoint: *outpoint,
            })
        }

        let mut inputs = Vec::new();
        self.process_inputs(&ctx, &mut tweaked_outputs, &mut inputs)
            .await?;

        let bitcoin_wallet = self.inner_wallet.read().unwrap();
        let mut tx_builder = bitcoin_wallet.build_tx();
        tx_builder.only_witness_utxo();
        tx_builder.fee_rate(fee_rate);

        for (outpoint, psbt_input, weight) in &inputs {
            tx_builder.add_foreign_utxo(*outpoint, psbt_input.clone(), *weight)?;
        }

        // Calculate the inputs sum and fee.
        let mut inputs_sum = 0;
        let mut total_weight = inputs[0].2;
        for (outpoint, _, weight) in inputs {
            let tx = blockchain
                .get_tx(&outpoint.txid)?
                .ok_or_else(|| eyre!("Transaction {} was not found", outpoint.txid))?;

            let output = &tx.output.get(outpoint.vout as usize).ok_or_else(|| {
                eyre!(
                    "Transaction {} doesn't contain vout {}",
                    outpoint.txid,
                    outpoint.vout
                )
            })?;

            inputs_sum += output.value;
            total_weight += weight;
        }

        let fee = fee_rate.as_sat_per_vb() as u64 * total_weight as u64;
        let output_sum = inputs_sum - fee;

        let pubkey = self.private_key.public_key(&ctx);
        let script_pubkey = ScriptBuf::new_v0_p2wpkh(&pubkey.wpubkey_hash().unwrap());

        tx_builder.add_recipient(script_pubkey, output_sum);

        let (mut psbt, _details) = tx_builder.finish()?;

        bitcoin_wallet.sign(
            &mut psbt,
            SignOptions {
                try_finalize: true,
                trust_witness_utxo: true,
                ..Default::default()
            },
        )?;

        let input_proofs = tweaked_outputs
            .iter()
            .enumerate()
            .map(|(i, (_, proof))| (i as u32, proof.clone()))
            .collect::<ProofMap>();

        self.tx_signer.sign(&mut psbt, &input_proofs)?;

        Ok(Some(psbt.extract_tx()))
    }

    async fn build_tx(mut self, fee_rate: BdkFeeRate) -> eyre::Result<YuvTransaction> {
        let ctx = Secp256k1::new();

        // Gather inputs as foreighn utxos with proofs for BDK wallet.
        let mut input_proofs = HashMap::new();
        let mut inputs = Vec::new();

        self.process_inputs(&ctx, &mut input_proofs, &mut inputs)
            .await?;

        #[cfg(feature = "bulletproof")]
        if !self.bulletproof_outputs.is_empty() {
            self.process_bulletproof_outputs(
                &input_proofs
                    .iter()
                    .filter_map(|(outpoint, proof)| {
                        proof
                            .get_bulletproof()
                            .map(|bulletproof| (*outpoint, bulletproof.clone()))
                    })
                    .collect(),
            )?;
        }

        // Gather output `script_pubkeys` with satoshis and profos for BDK wallet.
        let mut output_proofs = Vec::new();
        let mut outputs = Vec::new();

        for output in &self.outputs {
            self.process_output(output, &mut output_proofs, &mut outputs)?;
        }

        let bitcoin_wallet = self.inner_wallet.read().unwrap();
        let mut tx_builder = bitcoin_wallet.build_tx();

        // Do not sort inputs and outputs to make proofs valid
        tx_builder.ordering(TxOrdering::Untouched);
        tx_builder.only_witness_utxo();
        tx_builder.fee_rate(fee_rate);

        if self.is_issuance {
            let announcement = form_issue_announcement(output_proofs.clone())?;

            tx_builder.add_recipient(announcement.to_script(), 0);
        }
        // Fill tx_builder with formed inputs and outputs
        for (script_pubkey, amount) in outputs {
            tx_builder.add_recipient(script_pubkey, amount);
        }
        for (outpoint, psbt_input, weight) in inputs {
            tx_builder.add_foreign_utxo(outpoint, psbt_input, weight)?;
        }

        // Form transaction with satoshi inputs to satisfy consideration rules
        // of Bitcoin.
        let (mut psbt, _details) = tx_builder.finish()?;

        self.insert_empty_pixelproofs(&mut output_proofs, &mut psbt.unsigned_tx.output)?;

        let tx_type = form_tx_type(
            &psbt.unsigned_tx,
            &input_proofs,
            &output_proofs,
            self.is_issuance,
        )?;

        // Sign non YUV inputs with BDK wallet.
        bitcoin_wallet.sign(
            &mut psbt,
            SignOptions {
                try_finalize: true,
                trust_witness_utxo: true,
                ..Default::default()
            },
        )?;

        // We need to sign inputs in case of transfer transaction as there are always YUV inputs.
        // We also need to sign issue transaction inputs if it spends tweaked satoshis.
        if let YuvTxType::Transfer { input_proofs, .. } = &tx_type {
            self.tx_signer.sign(&mut psbt, input_proofs)?;
        } else if let YuvTxType::Issue { .. } = &tx_type {
            // Offset is basically the number of regular Bitcoin inputs that we need to skip
            // while constructing input proofs.
            let offset = psbt.inputs.len() - self.inputs.len();
            let input_proofs: ProofMap = input_proofs
                .into_values()
                .enumerate()
                .map(|(index, proof)| ((index + offset) as u32, proof))
                .collect();

            self.tx_signer.sign(&mut psbt, &input_proofs)?;
        }

        let tx = psbt.extract_tx();

        Ok(YuvTransaction {
            bitcoin_tx: tx,
            tx_type,
        })
    }

    /// Go through inputs, and form list of inputs for BDK wallet, and list of
    /// proofs for each input.
    ///
    /// Also, store keys that will be used for signing.
    async fn process_inputs(
        &mut self,
        ctx: &Secp256k1<All>,
        input_proofs: &mut HashMap<OutPoint, PixelProof>,
        inputs: &mut Vec<(OutPoint, psbt::Input, usize)>,
    ) -> eyre::Result<()> {
        #[cfg(feature = "bulletproof")]
        if !self.bulletproof_outputs.is_empty() {
            let outpoints = self
                .bulletproof_outputs
                .keys()
                .copied()
                .collect::<Vec<Option<OutPoint>>>();

            for outpoint in outpoints.into_iter().flatten() {
                self.add_bulletproof_input(outpoint.txid, outpoint.vout);
            }
        }

        for input in &self.inputs {
            let outpoint = input.outpoint();

            // Get proof for that input from synced transactions
            let (proof, output) = get_output_from_storage(&self.yuv_txs_storage, outpoint).await?;

            input_proofs.insert(outpoint, proof.clone());

            let mut psbt_input = psbt::Input {
                sighash_type: None,
                witness_utxo: Some(output.clone()),
                ..Default::default()
            };

            // Get descriptor and secret keys depending on the input type
            let (descriptor, secret_keys) =
                self.get_descriptor_and_keys_for_input(ctx, input, &proof)?;

            // Extend list of signers
            self.tx_signer.extend_signers(secret_keys);

            let derived = descriptor.at_derivation_index(0)?;

            psbt_input.update_with_descriptor_unchecked(&derived)?;

            // Some additional processing for psbt input
            if let BuilderInput::Multisig2x2 { .. } = input {
                let PixelProof::Multisig(multisig_proof) = proof else {
                    bail!("Invalid input proof type: proof is not multisig");
                };

                psbt_input.redeem_script = Some(multisig_proof.to_reedem_script()?);
            }

            let weight = derived.max_weight_to_satisfy()?;

            inputs.push((outpoint, psbt_input, weight));
        }

        Ok(())
    }

    /// Return descriptor for input and return map of keys that will be used for
    /// signing input after transaction is built.
    fn get_descriptor_and_keys_for_input(
        &self,
        ctx: &Secp256k1<All>,
        input: &BuilderInput,
        proof: &PixelProof,
    ) -> eyre::Result<(
        Descriptor<DescriptorPublicKey>,
        HashMap<XOnlyPublicKey, secp256k1::SecretKey>,
    )> {
        // Store private keys for future signing.
        let mut keys = HashMap::new();

        let pubkey1 = self.private_key.public_key(ctx);
        keys.insert(pubkey1.inner.into(), self.private_key.inner);

        // Keys keys depending of input type, and create descriptors on that.
        let (descriptor, _secret_keys, _) = match input {
            BuilderInput::Pixel { .. } => {
                let tweaked_pubkey = PixelKey::new_with_ctx(proof.pixel(), &pubkey1.inner, ctx)?;

                descriptor!(wpkh(tweaked_pubkey))?
            }
            BuilderInput::TweakedSatoshis { .. } => {
                let tweaked_pubkey = PixelKey::new_with_ctx(Pixel::empty(), &pubkey1.inner, ctx)?;

                descriptor!(wpkh(tweaked_pubkey))?
            }
            BuilderInput::Multisig2x2 {
                second_signer_key, ..
            } => {
                let pubkey2 = second_signer_key.public_key(ctx);
                keys.insert(pubkey2.inner.into(), second_signer_key.inner);

                let (tweaked_key1, key2) =
                    sort_and_tweak(ctx, self.private_key, *second_signer_key, proof)?;

                descriptor!(wsh(multi(2, tweaked_key1, key2)))?
            }
            #[cfg(feature = "bulletproof")]
            BuilderInput::BulletproofPixel { .. } => {
                let tweaked_pubkey = PixelKey::new_with_ctx(proof.pixel(), &pubkey1.inner, ctx)?;

                descriptor!(wpkh(tweaked_pubkey))?
            }
        };

        Ok((descriptor, keys))
    }

    /// Add output to the bitcoin transactions and list of output proofs.
    fn process_output(
        &self,
        output: &BuilderOutput,
        output_proofs: &mut Vec<PixelProof>,
        outputs: &mut Vec<(ScriptBuf, u64)>,
    ) -> eyre::Result<()> {
        let (script_pubkey, satoshis) = match output {
            // For satoshis output no addtion processing is required
            BuilderOutput::Satoshis {
                satoshis,
                recipient,
            } => {
                let (pixel_proof, script_pubkey) = get_empty_pixel_proof(*recipient)?;

                output_proofs.push(pixel_proof);
                (script_pubkey.clone(), *satoshis)
            }
            // For pixel, form script and push proof of it to the list
            BuilderOutput::Pixel {
                chroma,
                satoshis,
                amount,
                recipient,
            } => {
                let pixel = Pixel::new(*amount, *chroma);
                let pixel_key = PixelKey::new(pixel, recipient)?;

                let pubkey_hash = &pixel_key
                    .wpubkey_hash()
                    .ok_or_eyre("Pixel key is not compressed")?;

                let script_pubkey = ScriptBuf::new_v0_p2wpkh(pubkey_hash);

                let pixel_proof = SigPixelProof::new(pixel, *recipient);

                output_proofs.push(pixel_proof.into());

                (script_pubkey, *satoshis)
            }
            // For multisig pixel, form script and push proof of it to the list
            BuilderOutput::MultisigPixel {
                chroma,
                satoshis,
                amount,
                participants,
                required_signatures,
            } => {
                let pixel = Pixel::new(*amount, *chroma);

                let multisig_proof =
                    MultisigPixelProof::new(pixel, participants.clone(), *required_signatures);
                let script_pubkey = multisig_proof.to_script_pubkey();

                output_proofs.push(multisig_proof.into());

                (script_pubkey, *satoshis)
            }
            // For bulletproof pixel, form script and push proof of it to the list
            #[cfg(feature = "bulletproof")]
            BuilderOutput::BulletproofPixel {
                chroma,
                recipient,
                sender,
                luma,
                satoshis,
                commitment,
                proof,
                signature,
                chroma_signature,
            } => {
                let pixel = Pixel::new(*luma, *chroma);

                let pixel_key = PixelKey::new(pixel, &recipient.inner)?;

                let pixel_proof = PixelProof::bulletproof(
                    pixel,
                    recipient.inner,
                    sender.inner,
                    *commitment,
                    proof.clone(),
                    *signature,
                    *chroma_signature,
                );

                let script = ScriptBuf::new_v0_p2wpkh(
                    &pixel_key
                        .0
                        .wpubkey_hash()
                        .ok_or_else(|| eyre!("Pixel key is not compressed"))?,
                );

                output_proofs.push(pixel_proof);

                (script, *satoshis)
            }
        };

        outputs.push((script_pubkey, satoshis));

        Ok(())
    }
}

pub(crate) async fn get_output_from_storage<YTDB>(
    yuv_txs_storage: &YTDB,
    OutPoint { txid, vout }: OutPoint,
) -> eyre::Result<(PixelProof, TxOut)>
where
    YTDB: YuvTransactionsStorage + Clone + Send + Sync + 'static,
{
    let Some(tx) = yuv_txs_storage.get_yuv_tx(&txid).await? else {
        bail!("Transaction is not found in synced YUV txs: {}", txid);
    };

    let Some(output_proofs) = tx.tx_type.output_proofs() else {
        bail!("Transaction {} has no output proofs", txid);
    };

    let Some(proof) = output_proofs.get(&vout) else {
        bail!("Input is not found in synced YUV txs: {}:{}", txid, vout);
    };

    let Some(output) = tx.bitcoin_tx.output.get(vout as usize) else {
        bail!("Transaction output not found: {}:{}", txid, vout);
    };

    Ok((proof.clone(), output.clone()))
}

pub fn form_issue_announcement(output_proofs: Vec<PixelProof>) -> eyre::Result<IssueAnnouncement> {
    let filtered_proofs = output_proofs
        .iter()
        .filter(|proof| !proof.is_empty_pixelproof())
        .collect::<Vec<&PixelProof>>();

    let chroma = filtered_proofs
        .first()
        .map(|proof| proof.pixel().chroma)
        .ok_or_eyre("issuance with no outputs")?;

    #[cfg(feature = "bulletproof")]
    if is_bulletproof(filtered_proofs.clone()) {
        return Ok(IssueAnnouncement { chroma, amount: 0 });
    }

    let outputs_sum = filtered_proofs
        .iter()
        .map(|proof| proof.pixel().luma.amount)
        .sum::<u128>();

    Ok(IssueAnnouncement {
        chroma,
        amount: outputs_sum,
    })
}

/// Sort private keys by public keys and tweak first one.
fn sort_and_tweak(
    ctx: &Secp256k1<All>,
    key1: PrivateKey,
    key2: PrivateKey,
    proof: &PixelProof,
) -> eyre::Result<(PixelKey, PublicKey)> {
    let mut public_key1 = key1.public_key(ctx);
    let mut public_key2 = key2.public_key(ctx);

    if public_key1.inner.serialize()[..] > public_key2.inner.serialize()[..] {
        mem::swap(&mut public_key1, &mut public_key2);
    }

    let key1_tweaked = PixelKey::new_with_ctx(proof.pixel(), &public_key1.inner, ctx)?;

    Ok((key1_tweaked, public_key2))
}

/// Generate an empty pixel proof using the given `PublicKey` and an empty `Pixel`.
fn get_empty_pixel_proof(recipient: secp256k1::PublicKey) -> eyre::Result<(PixelProof, ScriptBuf)> {
    let pixel_key = PixelKey::new(Pixel::empty(), &recipient)?;

    let pubkey_hash = &pixel_key
        .wpubkey_hash()
        .ok_or_eyre("Pixel key is not compressed")?;

    let script_pubkey = ScriptBuf::new_v0_p2wpkh(pubkey_hash);

    Ok((
        PixelProof::EmptyPixel(EmptyPixelProof::new(recipient)),
        script_pubkey,
    ))
}

fn form_tx_type(
    unsigned_tx: &Transaction,
    input_proofs: &HashMap<OutPoint, PixelProof>,
    output_proofs: &[PixelProof],
    is_issuance: bool,
) -> eyre::Result<YuvTxType> {
    let mut mapped_input_proofs = BTreeMap::new();

    for (index, input) in unsigned_tx.input.iter().enumerate() {
        let Some(input_proof) = input_proofs.get(&input.previous_output) else {
            continue;
        };

        mapped_input_proofs.insert(index as u32, input_proof.clone());
    }

    let offset = if is_issuance { 1 } else { 0 };
    let output_proofs = output_proofs
        .iter()
        .enumerate()
        .map(|(index, proof)| ((index + offset) as u32, proof.clone()))
        .collect::<BTreeMap<u32, PixelProof>>();

    let tx_type = if is_issuance {
        let issue_announcement =
            form_issue_announcement(output_proofs.clone().into_values().collect())?;

        YuvTxType::Issue {
            output_proofs: Some(output_proofs),
            announcement: issue_announcement,
        }
    } else {
        YuvTxType::Transfer {
            input_proofs: mapped_input_proofs,
            output_proofs,
        }
    };

    Ok(tx_type)
}

#[cfg(test)]
mod tests {
    use bdk::database::MemoryDatabase;
    use yuv_storage::LevelDB;

    use super::*;

    fn check_is_sync<T: Sync>() {}
    fn check_is_send<T: Send>() {}

    #[test]
    fn test_send_sync() {
        check_is_sync::<TransactionBuilder<LevelDB, MemoryDatabase>>();
        check_is_send::<TransactionBuilder<LevelDB, MemoryDatabase>>();
    }
}
