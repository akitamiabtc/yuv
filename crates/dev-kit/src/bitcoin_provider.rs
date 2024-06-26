use std::result::Result::Ok;
use std::{ops::Deref, sync::Arc};

use serde::{Deserialize, Serialize};

use bdk::{
    bitcoincore_rpc::RpcApi,
    blockchain::{
        rpc::Auth, AnyBlockchain, AnyBlockchainConfig, ConfigurableBlockchain, EsploraBlockchain,
        RpcBlockchain,
    },
};
use bitcoin::{Network, OutPoint, Txid};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BitcoinProviderConfig {
    Esplora(EsploraConfig),
    BitcoinRpc(BitcoinRpcConfig),
}

impl From<Arc<BitcoinProviderConfig>> for BitcoinProviderConfig {
    fn from(value: Arc<BitcoinProviderConfig>) -> Self {
        value.deref().clone()
    }
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct BitcoinRpcConfig {
    /// Bitcoin node url
    pub url: String,
    /// Bitcoin's network
    pub network: Network,
    /// Auth params for rpc
    pub auth: Auth,
    /// Time in unix seconds in which initial sync will start scanning from (0 to start from genesis).
    pub start_time: u64,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct EsploraConfig {
    /// Esplora api url
    pub url: String,
    /// Bitcoin's network
    pub network: Network,
    /// It is a setting that determines when to stop fetching transactions
    /// for a set of addresses by indicating a gap of unused addresses.
    /// For example, if set to 20, the syncing mechanism
    /// would stop if it encounters 20 consecutive unused addresses.
    pub stop_gap: usize,
}

/// This describes the on-chain status of the output: whether it exists at all, and if it does, whether it has been spent
pub enum TxOutputStatus {
    /// Output was spent
    Spent,
    /// Output wasn't spent
    Unspent,
    /// This status can be applied in a case when we ask RPC but it doesn't find the output
    NotFound,
}

pub trait BitcoinProvider: Clone {
    /// Initialize provider from [`AnyBlockchainConfig`]
    fn from_config(cfg: AnyBlockchainConfig) -> eyre::Result<Self>;
    /// Takes outpoint then returns the output status. See [`TxOutputStatus`] about avaliable statuses.
    fn get_tx_out_status(&self, outpoint: OutPoint) -> eyre::Result<TxOutputStatus>;
    /// Returns the [`AnyBlockchain`] instance.
    fn blockchain(&self) -> Arc<AnyBlockchain>;
    /// Returns number of transaction confirmations on the bitcoin network.
    fn get_tx_confirmations(&self, txid: &Txid) -> eyre::Result<u32>;
    /// Returns the transaction is confirmed or not.
    fn is_tx_confirmed(&self, txid: &Txid) -> eyre::Result<bool> {
        Ok(self.get_tx_confirmations(txid)? >= REQUIRED_CONFIRMATIONS)
    }
}

/// Transaction confirmations amount when tx can be considered as confirmed.
const REQUIRED_CONFIRMATIONS: u32 = 6;

#[derive(Clone)]
pub struct AnyBitcoinProvider(Arc<AnyBlockchain>);

impl AnyBitcoinProvider {
    pub fn from_blockchain(blockchain: Arc<AnyBlockchain>) -> eyre::Result<Self> {
        if let AnyBlockchain::Electrum(_) = blockchain.deref() {
            eyre::bail!("Unsupported bitcoin provider");
        }

        Ok(Self(blockchain))
    }
}

impl TryFrom<EsploraBlockchain> for AnyBitcoinProvider {
    type Error = eyre::Error;

    fn try_from(value: EsploraBlockchain) -> Result<Self, Self::Error> {
        Self::from_blockchain(Arc::new(value.into()))
    }
}

impl TryFrom<RpcBlockchain> for AnyBitcoinProvider {
    type Error = eyre::Error;

    fn try_from(value: RpcBlockchain) -> Result<Self, Self::Error> {
        Self::from_blockchain(Arc::new(value.into()))
    }
}

impl BitcoinProvider for AnyBitcoinProvider {
    fn get_tx_out_status(&self, OutPoint { txid, vout }: OutPoint) -> eyre::Result<TxOutputStatus> {
        match self.0.deref() {
            AnyBlockchain::Esplora(esplora) => {
                let Some(output_status) = esplora.get_output_status(&txid, vout.into())? else {
                    return Ok(TxOutputStatus::NotFound);
                };

                match output_status.spent {
                    true => Ok(TxOutputStatus::Spent),
                    false => Ok(TxOutputStatus::Unspent),
                }
            }
            AnyBlockchain::Rpc(rpc) => {
                let output_status = match rpc.get_tx_out(&txid, vout, None)? {
                    Some(_) => TxOutputStatus::Unspent,
                    None => TxOutputStatus::Spent,
                };

                Ok(output_status)
            }
            _ => eyre::bail!("Unsupported bitcoin provider"),
        }
    }

    fn blockchain(&self) -> Arc<AnyBlockchain> {
        self.0.clone()
    }

    fn from_config(cfg: AnyBlockchainConfig) -> eyre::Result<Self> {
        match cfg {
            AnyBlockchainConfig::Esplora(cfg) => EsploraBlockchain::from_config(&cfg)?.try_into(),
            AnyBlockchainConfig::Rpc(cfg) => RpcBlockchain::from_config(&cfg)?.try_into(),
            _ => eyre::bail!("Unsupported bitcoin provider"),
        }
    }

    fn get_tx_confirmations(&self, txid: &Txid) -> eyre::Result<u32> {
        match self.0.deref() {
            AnyBlockchain::Esplora(esplora) => {
                let tx_status = esplora.get_tx_status(txid)?;

                let tx_mined_block = tx_status.block_height.unwrap_or_default();

                let cur_height = esplora.get_height()?;

                Ok(cur_height - tx_mined_block)
            }
            AnyBlockchain::Rpc(rpc) => {
                let tx_info = rpc.get_raw_transaction_info(txid, None)?;

                Ok(tx_info.confirmations.unwrap_or_default())
            }
            _ => eyre::bail!("Unsupported bitcoin provider"),
        }
    }
}
