use std::sync::Arc;

use bitcoin::Weight;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use once_cell::sync::Lazy;

use bdk::bitcoin::{secp256k1::Secp256k1, Network, PrivateKey, Txid};
use bdk::blockchain::rpc::RpcSyncParams;
use bdk::blockchain::{AnyBlockchain, EsploraBlockchain, GetTx};
use bdk::FeeRate;
use bdk::{
    blockchain::{rpc::Auth, ConfigurableBlockchain, RpcBlockchain, RpcConfig},
    descriptor,
    wallet::wallet_name_from_descriptor,
};
use bitcoin_client::BitcoinRpcAuth;
use ydk::bitcoin_provider::{BitcoinProviderConfig, BitcoinRpcConfig, EsploraConfig};
use ydk::wallet::{MemoryWallet, WalletConfig};
use yuv_rpc_api::transactions::{
    GetRawYuvTransactionResponseHex, YuvTransactionStatus, YuvTransactionsRpcClient,
};
use yuv_types::YuvTransaction;

/// Yuv node rpc url
pub const YUV_NODE_URL: &str = "http://127.0.0.1:18333";

// Nigiri credentials:
const BITCOIN_NODE_URL: &str = "http://127.0.0.1:18443";

/// Esplora api url
const ESPLORA_API_URL: &str = "http://127.0.0.1:3000";

static BITCOIN_NODE_AUTH: Lazy<BitcoinRpcAuth> = Lazy::new(|| BitcoinRpcAuth::UserPass {
    username: "admin1".to_string(),
    password: "123".to_string(),
});

static BITCOIN_RPC_AUTH: Lazy<Auth> = Lazy::new(|| Auth::UserPass {
    username: "admin1".to_string(),
    password: "123".to_string(),
});

static BITCOIN_RPC_CONFIG: Lazy<BitcoinProviderConfig> = Lazy::new(|| {
    BitcoinProviderConfig::BitcoinRpc(BitcoinRpcConfig {
        url: BITCOIN_NODE_URL.to_string(),
        auth: BITCOIN_RPC_AUTH.clone(),
        network: bitcoin::Network::Regtest,
        start_time: 0,
    })
});

static ESPLORA_CONFIG: Lazy<BitcoinProviderConfig> = Lazy::new(|| {
    let esplora_cfg = EsploraConfig {
        url: ESPLORA_API_URL.to_string(),
        network: Network::Regtest,
        stop_gap: 20,
    };

    BitcoinProviderConfig::Esplora(esplora_cfg)
});

pub fn bitcoin_provider_config(esplora_enabled: bool) -> BitcoinProviderConfig {
    match esplora_enabled {
        true => ESPLORA_CONFIG.clone(),
        false => BITCOIN_RPC_CONFIG.clone(),
    }
}

pub async fn setup_wallet_from_provider(
    privkey: PrivateKey,
    provider: BitcoinProviderConfig,
) -> eyre::Result<MemoryWallet> {
    let wallet_config = WalletConfig {
        privkey,
        network: Network::Regtest,
        bitcoin_provider: provider,
        yuv_url: YUV_NODE_URL.to_string(),
    };

    let wallet = MemoryWallet::from_config(wallet_config).await?;

    Ok(wallet)
}

pub fn setup_blockchain(cfg: &BitcoinProviderConfig) -> Arc<AnyBlockchain> {
    match (*cfg).clone() {
        BitcoinProviderConfig::Esplora(cfg) => {
            Arc::new(EsploraBlockchain::new(cfg.url.as_str(), cfg.stop_gap).into())
        }
        BitcoinProviderConfig::BitcoinRpc(cfg) => Arc::new(
            RpcBlockchain::from_config(&RpcConfig {
                url: cfg.url,
                auth: cfg.auth,
                network: cfg.network,
                wallet_name: "some_wallet".to_string(),
                sync_params: Some(RpcSyncParams {
                    start_time: cfg.start_time,
                    ..Default::default()
                }),
            })
            .expect("rpc blockchain should be inited")
            .into(),
        ),
    }
}

pub fn setup_rpc_blockchain(user: &PrivateKey) -> eyre::Result<RpcBlockchain> {
    let secp_ctx = Secp256k1::new();

    let wallet_name =
        wallet_name_from_descriptor(descriptor!(wpkh(user))?, None, Network::Regtest, &secp_ctx)?;

    let config = RpcConfig {
        url: BITCOIN_NODE_URL.to_string(),
        auth: from_custom_auth(BITCOIN_NODE_AUTH.clone()),
        network: Network::Regtest,
        wallet_name,
        sync_params: None,
    };

    Ok(RpcBlockchain::from_config(&config)?)
}

pub fn setup_yuv_client(node_url: &str) -> eyre::Result<HttpClient> {
    let client = HttpClientBuilder::new().build(node_url)?;

    Ok(client)
}

#[macro_export]
macro_rules! assert_attached {
    ($tx:expr, $msg:literal) => {
        assert!(
            matches!($tx.status, yuv_rpc_api::transactions::YuvTransactionStatus::Attached),
            $msg,
        );
    };
    ($tx:expr, $msg:literal, $($options:expr),*) => {
        assert!(
            matches!($tx.status, yuv_rpc_api::transactions::YuvTransactionStatus::Attached),
            $msg,
            $($options),*
        );
    };
}

#[macro_export]
macro_rules! assert_wallet_has_utxo {
    ($wallet:expr, $txid:expr, $vout:expr, $msg:literal) => {{
        let __wallet = &$wallet;
        let __outpoint = bitcoin::OutPoint::new($txid, $vout);

        let __utxos = __wallet.yuv_utxos();

        let __utxo = __utxos.get(&__outpoint);

        assert!(__utxo.is_some(), $msg);
    }};
}

#[allow(dead_code)]
pub fn assert_fee_matches_difference(
    tx: &YuvTransaction,
    provider: &AnyBlockchain,
    fee_rate: FeeRate,
    is_issuance: bool,
) -> eyre::Result<()> {
    let outputs = tx.clone().bitcoin_tx.output;
    let outputs_sum: u64 = outputs.iter().map(|output| output.value).sum();

    let weight = tx.bitcoin_tx.weight();

    let fee_wu = if is_issuance {
        fee_rate.fee_wu(weight)
    } else {
        // Segwit transactions' header is 2WU larger than legacy txs' header.
        fee_rate.fee_wu(weight + Weight::from_wu(2))
    };

    let inputs = tx.clone().bitcoin_tx.input;
    let mut inputs_sum = 0;
    for input in inputs {
        let tx = provider.get_tx(&input.previous_output.txid)?.unwrap();
        inputs_sum += tx.output[input.previous_output.vout as usize].value
    }

    assert_eq!(
        inputs_sum - outputs_sum,
        fee_wu,
        "fee doesn't match the txin/txout difference"
    );

    Ok(())
}

pub fn from_custom_auth(custom_rpc_auth: BitcoinRpcAuth) -> Auth {
    match custom_rpc_auth {
        BitcoinRpcAuth::None => Auth::None,
        BitcoinRpcAuth::UserPass { username, password } => Auth::UserPass { username, password },
        BitcoinRpcAuth::Cookie { file } => Auth::Cookie { file },
    }
}

pub async fn wait_until_reject_or_attach(
    txid: Txid,
    yuv_client: &HttpClient,
) -> eyre::Result<GetRawYuvTransactionResponseHex> {
    let mut tx = yuv_client.get_yuv_transaction(txid).await?;

    while !matches!(tx.status, YuvTransactionStatus::Attached) {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        tx = yuv_client.get_yuv_transaction(txid).await?;
    }

    Ok(tx)
}
