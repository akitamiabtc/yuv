use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use bdk::{
    blockchain::{
        esplora::EsploraBlockchainConfig, rpc::RpcSyncParams, AnyBlockchainConfig, Blockchain,
        RpcConfig,
    },
    database::{MemoryDatabase, SqliteDatabase},
    descriptor,
    wallet::wallet_name_from_descriptor,
    Balance, LocalUtxo, SignOptions,
};
use bitcoin::{
    secp256k1::{self, All, Secp256k1},
    Address, Network, OutPoint, PrivateKey, PublicKey,
};
use eyre::{eyre, Context};
use futures::future::join_all;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use yuv_pixels::{Chroma, LightningCommitmentProof, Pixel, PixelProof, ToEvenPublicKey};
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;
use yuv_storage::{
    FlushStrategy, LevelDB, LevelDbOptions, PagesNumberStorage,
    TransactionsStorage as YuvTransactionsStorage,
};
use yuv_types::{Announcement, YuvTransaction};

use crate::{
    bitcoin_provider::{BitcoinProvider, BitcoinProviderConfig, TxOutputStatus},
    database::wrapper::DatabaseWrapper,
    sync::{indexer::YuvTransactionsIndexer, storage::UnspentYuvOutPointsStorage},
    txbuilder::{
        get_output_from_storage, IssuanceTransactionBuilder, SweepTransactionBuilder,
        TransferTransactionBuilder,
    },
    types::{FeeRateStrategy, YuvBalances},
    AnyBitcoinProvider,
};

pub const DEFAULT_FEE_RATE_STRATEGY: FeeRateStrategy = FeeRateStrategy::TryEstimate {
    fee_rate: 1.0,
    target: 2,
};

pub type MemoryWallet =
    Wallet<HttpClient, LevelDB, AnyBitcoinProvider, DatabaseWrapper<MemoryDatabase>>;

/// Configuration parameters requried to construct [`Wallet`].
#[derive(Clone, Debug)]
pub struct WalletConfig {
    /// Private key of the user.
    pub privkey: PrivateKey,

    /// Network of the wallet.
    pub network: Network,

    /// Bitcoin provider config
    pub bitcoin_provider: BitcoinProviderConfig,

    // == YUV node RPC ==
    /// URL of YUV node RPC API.
    pub yuv_url: String,
}

impl TryFrom<WalletConfig> for AnyBlockchainConfig {
    type Error = eyre::Error;

    fn try_from(config: WalletConfig) -> Result<Self, Self::Error> {
        let secp_ctx = Secp256k1::new();

        let wallet_name = wallet_name_from_descriptor(
            descriptor!(wpkh(config.privkey))?,
            None,
            config.network,
            &secp_ctx,
        )?;

        let res = match config.bitcoin_provider {
            BitcoinProviderConfig::Esplora(cfg) => {
                AnyBlockchainConfig::Esplora(EsploraBlockchainConfig::new(cfg.url, cfg.stop_gap))
            }
            BitcoinProviderConfig::BitcoinRpc(cfg) => AnyBlockchainConfig::Rpc(RpcConfig {
                url: cfg.url,
                auth: cfg.auth,
                network: cfg.network,
                wallet_name,
                sync_params: Some(RpcSyncParams {
                    start_time: cfg.start_time,
                    ..Default::default()
                }),
            }),
        };

        Ok(res)
    }
}

impl MemoryWallet {
    pub async fn from_config(config: WalletConfig) -> eyre::Result<Self> {
        let signer_key = config.privkey;
        let network = config.network;

        let bitcoin_provider = BitcoinProvider::from_config(config.clone().try_into()?)?;
        let yuv_client = HttpClientBuilder::new().build(config.yuv_url)?;

        let yuv_txs_storage = LevelDB::in_memory()?;
        let bitcoin_txs_storage = DatabaseWrapper::new(MemoryDatabase::default());

        Self::new(
            signer_key,
            network,
            yuv_client,
            yuv_txs_storage,
            bitcoin_provider,
            bitcoin_txs_storage,
        )
    }
}

/// Wallet that stores transaction and data in local file storage.
///
/// LevelDB for YUV transactions and SQLite for Bitcoin transactions.
pub type StorageWallet =
    Wallet<HttpClient, LevelDB, AnyBitcoinProvider, DatabaseWrapper<SqliteDatabase>>;

pub struct StorageWalletConfig {
    /// Configuration parameters for wallet.
    pub inner: WalletConfig,

    /// Path to directory where transactions will be stored.
    pub storage_path: PathBuf,
}

const YUV_TXS_DIR_NAME: &str = "yuv";
const BITCOIN_TXS_DIR_NAME: &str = "bitcoin";

impl StorageWallet {
    pub async fn from_storage_config(config: StorageWalletConfig) -> eyre::Result<Self> {
        let signer_key = config.inner.privkey;
        let network = config.inner.network;

        let bitcoin_provider = BitcoinProvider::from_config(config.inner.clone().try_into()?)?;
        let yuv_client = HttpClientBuilder::new().build(config.inner.yuv_url)?;

        let yuv_txs_storage = LevelDB::from_opts(LevelDbOptions {
            path: config.storage_path.join(YUV_TXS_DIR_NAME),
            create_if_missing: true,
            flush_strategy: FlushStrategy::Disabled,
        })?;

        let bitcoin_txs_storage = DatabaseWrapper::new(SqliteDatabase::new(
            config.storage_path.join(BITCOIN_TXS_DIR_NAME),
        ));

        Self::new(
            signer_key,
            network,
            yuv_client,
            yuv_txs_storage,
            bitcoin_provider,
            bitcoin_txs_storage,
        )
    }
}

unsafe impl<YPC, YTD, BP, BTDB> Sync for Wallet<YPC, YTD, BP, BTDB>
where
    YPC: Sync,
    YTD: Sync,
    BP: Sync,
    BTDB: Sync,
{
}

unsafe impl<YPC, YTD, BP, BTDB> Send for Wallet<YPC, YTD, BP, BTDB>
where
    YPC: Send,
    YTD: Send,
    BP: Send,
    BTDB: Send,
{
}

pub struct SyncOptions {
    pub inner: bdk::SyncOptions,
    /// Sync YUV wallet, defaults to true
    pub sync_yuv_wallet: bool,
    /// Sync Bitcoin wallet, defaults to true
    pub sync_bitcoin_wallet: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            sync_yuv_wallet: true,
            sync_bitcoin_wallet: true,
        }
    }
}

impl SyncOptions {
    /// Sync only inner Bitcoin wallet without YUV wallet.
    pub fn bitcoin_only() -> Self {
        Self {
            sync_yuv_wallet: false,
            ..Default::default()
        }
    }

    /// Sync only YUV wallet without inner Bitcoin wallet.
    pub fn yuv_only() -> Self {
        Self {
            sync_bitcoin_wallet: false,
            ..Default::default()
        }
    }
}

/// A wallet that can manage YUV UTXOs and create transactions for YUV protocol.
#[derive(Clone)]
pub struct Wallet<YuvRpcClient, YuvTxsDB, BitcoinProvider, BitcoinTxsDB> {
    /// Global wallet context used for internal operations on curve.
    pub(crate) secp_ctx: Secp256k1<All>,

    /// Private key of the user.
    pub(crate) signer_key: PrivateKey,
    pub(crate) network: Network,

    /// Internal storage for YUV UTXOs.
    pub(crate) utxos: Arc<RwLock<HashMap<OutPoint, PixelProof>>>,

    /// Client to access YUV node RPC API.
    pub(crate) yuv_client: YuvRpcClient,

    /// Storage of transactions.
    pub(crate) yuv_txs_storage: YuvTxsDB,

    /// Client to Bitcoin RPC.
    pub(crate) bitcoin_provider: BitcoinProvider,

    /// Bitcoin wallet
    pub(crate) bitcoin_wallet: Arc<RwLock<bdk::Wallet<BitcoinTxsDB>>>,
}

impl<YC, YTDB, BP, BTDB> Wallet<YC, YTDB, BP, BTDB>
where
    YC: YuvTransactionsRpcClient + Clone + Send + Sync + 'static,
    YTDB: YuvTransactionsStorage
        + PagesNumberStorage
        + UnspentYuvOutPointsStorage
        + Clone
        + Send
        + Sync
        + 'static,
    BP: BitcoinProvider + Clone + Send + Sync + 'static,
    BTDB: bdk::database::BatchDatabase + Clone + Send + Sync,
{
    pub fn new(
        privkey: PrivateKey,
        network: Network,
        yuv_client: YC,
        yuv_txs_storage: YTDB,
        bitcoin_provider: BP,
        bitcoin_txs_storage: BTDB,
    ) -> eyre::Result<Self> {
        let bitcoin_wallet = bdk::Wallet::<BTDB>::new(
            descriptor!(wpkh(privkey))?,
            None,
            network,
            bitcoin_txs_storage,
        )
        .wrap_err("Failed to initialize wallet")?;

        Ok(Self {
            secp_ctx: Secp256k1::new(),
            signer_key: privkey,
            network,
            utxos: Arc::new(RwLock::new(HashMap::new())),
            yuv_client,
            yuv_txs_storage,
            bitcoin_provider,
            bitcoin_wallet: Arc::new(RwLock::new(bitcoin_wallet)),
        })
    }

    /// Synchronize from YUV node all unspent outpoints and sync the internal bitcoin wallet
    /// database with the blockchain
    pub async fn sync(&self, opts: SyncOptions) -> eyre::Result<()> {
        if opts.sync_bitcoin_wallet {
            self.bitcoin_wallet
                .write()
                .unwrap()
                .sync(&self.bitcoin_provider.blockchain(), opts.inner)?;
        }

        // Skip syncing of YUV wallet if we don't need that.
        if !opts.sync_yuv_wallet {
            return Ok(());
        }

        let pubkey = self.signer_key.even_public_key(&self.secp_ctx);

        let utxos = YuvTransactionsIndexer::new(
            self.yuv_client.clone(),
            self.yuv_txs_storage.clone(),
            pubkey,
        )
        .sync()
        .await
        .wrap_err("Failed to sync YUV transactions from node")?;

        let utxos = self.filter_spent_utxos(utxos).await?;

        let mut guard = self.utxos.write().map_err(|_| eyre!("Poisoned lock"))?;
        *guard = utxos.into_iter().collect();

        Ok(())
    }

    pub fn address(&self) -> eyre::Result<Address> {
        let addr = Address::p2wpkh(&self.signer_key.public_key(&self.secp_ctx), self.network)?;

        Ok(addr)
    }

    pub fn public_key(&self) -> PublicKey {
        self.signer_key.public_key(&self.secp_ctx)
    }

    pub fn bitcoin_provider(&self) -> BP {
        self.bitcoin_provider.clone()
    }

    /// Return the reference to inner [`bdk::Wallet`] which handles the indexing of
    /// Bitcoins for us.
    ///
    /// # Safety
    ///
    /// This methods is marked as unsafe as it's related to unsafe
    /// implementations for [`Wallet`] of `Send` and `Sync`. The problem is,
    /// that [`bdk::Wallet`] has `RefCell` inside of it, because of which we
    /// cannot borrow and use it in async context. But still [`bdk::Wallet`]
    /// wallet has getter method for database [`bdk::Wallet::database`] which is
    /// not recommended to use.
    pub unsafe fn bitcoin_wallet(&self) -> Arc<RwLock<bdk::Wallet<BTDB>>> {
        Arc::clone(&self.bitcoin_wallet)
    }

    /// By calling [`gettxout`] for each unspent YUV transaction got from node,
    /// check if transaction is already spent on Bitcoin.
    ///
    /// This could happen when proofs were not brought to node in time, so this
    /// function prevents this.
    ///
    /// [`gettxout`]: https://developer.bitcoin.org/reference/rpc/gettxout.html
    async fn filter_spent_utxos(
        &self,
        utxos: Vec<(OutPoint, PixelProof)>,
    ) -> eyre::Result<Vec<(OutPoint, PixelProof)>> {
        let futures = utxos
            .into_iter()
            .map(|(OutPoint { txid, vout }, proof)| async move {
                let output_status = match self
                    .bitcoin_provider
                    .get_tx_out_status(OutPoint::new(txid, vout))
                {
                    Ok(status) => status,
                    Err(e) => {
                        tracing::debug!("Failed to get tx output: {}", e);
                        return None;
                    }
                };

                match output_status {
                    TxOutputStatus::Spent => {
                        tracing::debug!("UTXO {}:{} is spent", txid, vout);
                        return None;
                    }
                    TxOutputStatus::NotFound => {
                        tracing::debug!("UTXO {}:{} not found", txid, vout);
                        return None;
                    }
                    _ => (),
                }

                match &proof {
                    PixelProof::Sig(..) => Some((OutPoint::new(txid, vout), proof)),
                    PixelProof::Lightning(LightningCommitmentProof { to_self_delay, .. }) => {
                        match self.bitcoin_provider.get_tx_confirmations(&txid) {
                            Ok(confirmations) => {
                                if confirmations >= *to_self_delay as u32 {
                                    Some((OutPoint::new(txid, vout), proof))
                                } else {
                                    None
                                }
                            }
                            Err(_) => None, // Handle the error by returning None
                        }
                    }
                    #[cfg(feature = "bulletproof")]
                    PixelProof::Bulletproof(..) => Some((OutPoint::new(txid, vout), proof)),
                    PixelProof::EmptyPixel(..) => Some((OutPoint::new(txid, vout), proof)),
                    PixelProof::LightningHtlc(..) | PixelProof::Multisig(..) => None,
                }
            });

        let results = join_all(futures).await;
        let filtered: Vec<_> = results.into_iter().flatten().collect();

        Ok(filtered)
    }

    /// Calculate current balances by iterating through transactions from
    /// intenal storage.
    pub async fn balances(&self) -> eyre::Result<YuvBalances> {
        let mut yuv_balances = HashMap::new();
        #[cfg(feature = "bulletproof")]
        let mut bulletproof_balances = HashMap::new();
        let mut tweaked_satoshis_balances = 0;

        // Collect the data while holding the read lock.
        let utxos: Vec<(OutPoint, PixelProof)> = {
            let utxos = self.utxos.read().unwrap();
            utxos
                .iter()
                .map(|(outpoint, proof)| (*outpoint, proof.clone()))
                .collect()
        };

        for (outpoint, proof) in utxos {
            if proof.is_empty_pixelproof() {
                let (_pixel_proof, txout) =
                    get_output_from_storage(&self.yuv_txs_storage, outpoint).await?;
                tweaked_satoshis_balances += txout.value;
                continue;
            }

            let pixel = proof.pixel();

            #[cfg(feature = "bulletproof")]
            if proof.is_bulletproof() {
                *bulletproof_balances.entry(pixel.chroma).or_insert(0) += pixel.luma.amount;
                continue;
            }

            *yuv_balances.entry(pixel.chroma).or_insert(0) += pixel.luma.amount;
        }

        Ok(YuvBalances {
            yuv: yuv_balances,
            tweaked_satoshis: tweaked_satoshis_balances,
            #[cfg(feature = "bulletproof")]
            bulletproof: bulletproof_balances,
        })
    }

    /// Get Bitcoin balances.
    pub fn bitcoin_balances(&self) -> eyre::Result<Balance> {
        Ok(self.bitcoin_wallet.read().unwrap().get_balance()?)
    }

    /// Get all unspent Bitcoin outputs.
    pub fn bitcoin_utxos(&self) -> eyre::Result<Vec<LocalUtxo>> {
        Ok(self.bitcoin_wallet.read().unwrap().list_unspent()?)
    }

    /// Get all unspent YUV transactions outputs with given [`Chroma`].
    pub fn utxos_by_chroma(&self, chroma: Chroma) -> Vec<(OutPoint, u128)> {
        let utxos = self.yuv_utxos();
        utxos
            .iter()
            .filter(|(_, proof)| proof.pixel().chroma == chroma)
            .map(|(outpoint, proof)| (*outpoint, proof.pixel().luma.amount))
            .collect()
    }

    /// Get unspent YUV transactions with the given filter.
    fn utxos<P>(&self, filter: P) -> HashMap<OutPoint, PixelProof>
    where
        P: Fn(&(&OutPoint, &PixelProof)) -> bool,
    {
        let utxos = &self
            .utxos
            .read()
            .unwrap()
            .iter()
            .filter(|utxo| filter(utxo))
            .map(|(outpoint, proof)| (*outpoint, proof.clone()))
            .collect::<HashMap<OutPoint, PixelProof>>();

        (*utxos).clone()
    }

    /// Get all unspent YUV transactions outputs.
    pub fn yuv_utxos(&self) -> HashMap<OutPoint, PixelProof> {
        self.utxos(|utxo| !utxo.1.is_empty_pixelproof())
    }

    /// Get unspent tweaked Bitcoin outputs.
    ///
    /// Note: all the tweaked unspent outputs are tweaked by the same zero chroma.
    pub fn tweaked_satoshi_utxos(&self) -> HashMap<OutPoint, PixelProof> {
        self.utxos(|utxo| utxo.1.is_empty_pixelproof())
    }

    /// Return [`YuvTxType::Transfer`] transaction builder for creating
    /// transaction by YUV protocol.
    ///
    /// [`YuvTxType::Transfer`]: yuv_types::YuvTxType::Transfer
    pub fn build_transfer(&self) -> eyre::Result<TransferTransactionBuilder<YTDB, BTDB>> {
        TransferTransactionBuilder::try_from(self)
    }

    /// Return [`YuvTxType::Issue`] transaction builder for creating
    /// issuance transaction by YUV protocol
    ///
    /// [`YuvTxType::Issue`]: yuv_types::YuvTxType::Issue
    pub fn build_issuance(
        &self,
        chroma: Option<Chroma>,
    ) -> eyre::Result<IssuanceTransactionBuilder<YTDB, BTDB>> {
        IssuanceTransactionBuilder::new(self, chroma)
    }

    /// Return a sweep transaction builder for creating
    /// a sweep transaction by YUV protocol.
    pub fn build_sweep(&self) -> eyre::Result<SweepTransactionBuilder<YTDB, BTDB>> {
        SweepTransactionBuilder::try_from(self)
    }

    /// Create funding lightning transaction from:
    ///
    /// * `funding_pixel` - chroma and amount that will be in Lightning Network
    /// funding transaction.
    /// * `holder_pubkey` - funding pubkey of the holder.
    /// * `counterparty_pubkey` - funding pubkey of the counterparty.
    /// * `satoshis` - value of satoshis that will be included in funding transaction
    /// with `funding_pixel`.
    /// * optional `fee_rate_strategy` which defaults to `TryEstimate` with `fee_rate = 1.0`.
    ///
    /// The keys will be sorted in lexical order, and the first one after that will be tweaked
    /// by `funding_pixel`.
    ///
    /// # Returns
    ///
    /// Returns [`YuvTransaction`] with 2 of 2 multisig which has specified
    /// `satoshis` value and pixel from `funding_pixel`, `holder_pubkey` and
    /// `counterparty_pubkey` as spenders and filled Bitcoin and YUV inputs with
    /// some additional outputs for change.
    pub async fn lightning_funding_tx(
        &self,
        funding_pixel: Pixel,
        holder_pubkey: secp256k1::PublicKey,
        counterparty_pubkey: secp256k1::PublicKey,
        satoshis: u64,
        fee_rate_strategy: Option<FeeRateStrategy>,
    ) -> eyre::Result<YuvTransaction> {
        let fee_rate_strategy = fee_rate_strategy.unwrap_or(DEFAULT_FEE_RATE_STRATEGY);

        let mut tx_builder = self
            .build_transfer()
            .wrap_err("failed to init transaction builder")?;

        tx_builder
            .add_multisig_recipient(
                vec![holder_pubkey, counterparty_pubkey],
                2,
                funding_pixel.luma.amount,
                funding_pixel.chroma,
                satoshis,
            )
            .set_fee_rate_strategy(fee_rate_strategy);

        let yuv_transaction = tx_builder
            .finish(&self.bitcoin_provider.blockchain())
            .await
            .wrap_err("failed to build yuv transaction")?;

        Ok(yuv_transaction)
    }

    /// Create a simple transfer to given recipient with given pixel.
    pub async fn create_transfer(
        &self,
        pixel: Pixel,
        recipient: secp256k1::PublicKey,
        fee_rate_strategy: Option<FeeRateStrategy>,
    ) -> eyre::Result<YuvTransaction> {
        let fee_rate_strategy = fee_rate_strategy.unwrap_or(DEFAULT_FEE_RATE_STRATEGY);

        let mut tx_builder = self
            .build_transfer()
            .wrap_err("failed to init transaction builder")?;

        tx_builder
            .add_recipient(pixel.chroma, &recipient, pixel.luma.amount, 1000)
            .set_fee_rate_strategy(fee_rate_strategy);

        let yuv_tx = tx_builder
            .finish(&self.bitcoin_provider.blockchain())
            .await
            .wrap_err("failed to build yuv transaction")?;

        Ok(yuv_tx)
    }

    /// Create YUV [`Announcement`] transaction for given [`Announcement`].
    pub fn create_announcement_tx(
        &self,
        announcement: Announcement,
        fee_rate_strategy: FeeRateStrategy,
        blockchain: &impl Blockchain,
    ) -> eyre::Result<YuvTransaction> {
        let tx = {
            let wallet = self.bitcoin_wallet.read().unwrap();
            let mut builder = wallet.build_tx();

            let fee_rate = fee_rate_strategy
                .get_fee_rate(blockchain)
                .wrap_err("failed to estimate fee")?;

            builder
                .add_recipient(announcement.to_script(), 0)
                .fee_rate(fee_rate)
                .allow_dust(true);

            let (mut psbt, _) = builder.finish()?;

            wallet.sign(&mut psbt, SignOptions::default())?;

            psbt.extract_tx()
        };

        Ok(YuvTransaction::new(tx, announcement.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that [`Wallet`] implements `Sync` and `Send`.
    #[test]
    fn wallet_is_sync_and_send() {
        fn assert_sync<T: Sync>() {}
        fn assert_send<T: Send>() {}

        assert_sync::<MemoryWallet>();
        assert_send::<MemoryWallet>();

        assert_sync::<StorageWallet>();
        assert_send::<StorageWallet>();
    }
}