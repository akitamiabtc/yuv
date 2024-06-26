# `dev-kit`

A utility crate that provides building blocks for constructing and signing YUV transactions.

The main components include:

- [Transaction builder](src/txbuilder.rs): used to build **issuance** and **transfer** YUV transactions.
- [Transaction signer](src/txsigner.rs): used to sign *singlesig*, *multisig*, *bulletproof* and *lightning* YUV transactions.
- [YUV Wallet](src/wallet.rs): an abstraction over `BDK` wallet that provides an interface for simple transaction creation. It synchronizes with the YUV node to fetch all the unspent outpoints and fetches UTXOs with satoshis through different providers (like Bitcoin node or Esplora server). It is also used to create **freeze/unfreeze** transactions.
- [Coin selection](src/yuv_coin_selection.rs): provides a trait for generalized YUV coin selection algorithms. Currently, there are two implementations:
  - `YuvLargestFirstCoinSelection`: this coin selection algorithm sorts the available UTXOs by value and then picks them starting from the largest ones until the required amount is reached. Simple and dumb coin selection.
  - `YUVOldestFirstCoinSelection`: this coin selection algorithm sorts the available UTXOs by `blockheight` and then picks them starting from the oldest ones until the required amount is reached.
- [Types](src/types.rs): provides some types that are used by the components listed above.

A simple example of how to build a transfer transaction using `dev-kit's` YUV `MemoryWallet`:

```rust
use bdk::bitcoin::PrivateKey;
use bdk::blockchain::{rpc::Auth, EsploraBlockchain, AnyBlockchain};
use bitcoin::secp256k1::PublicKey;
use std::{str::FromStr, sync::Arc};
use ydk::{
    types::FeeRateStrategy,
    bitcoin_provider::{BitcoinProviderConfig, BitcoinRpcConfig},
    wallet::{MemoryWallet, WalletConfig},
};
use yuv_pixels::Chroma;

async fn build_tx() {
    // Provide valid Bitcoin node credentials.
    let bitcoin_auth = Auth::UserPass {
        username: "admin1".to_string(),
        password: "123".to_string(),
    };

    // Set up the Bitcoin provider. In this case, Rpc is used.
    let provider = BitcoinProviderConfig::BitcoinRpc(BitcoinRpcConfig {
        url: "http://127.0.0.1:18443".to_string(), // Provide a valid, accessible Bitcoin node URL.
        auth: bitcoin_auth,
        network: bitcoin::Network::Regtest, // Specify the desired network.
        start_time: 0,
    });

    let private_key: PrivateKey = "cNMMXcLoM65N5GaULU7ct2vexmQnJ5i5j3Sjc6iNnEF18vY7gzn9"
        .parse()
        .expect("Should be valid key");

    // Set up the wallet config.
    let wallet_config = WalletConfig {
        privkey: private_key, // Replace `private_key` with the actual private key.
        network: bitcoin::Network::Regtest, // Specify the desired network.
        bitcoin_provider: provider, // Provide a valid Bitcoin provider. Could be either `BitcoinRpcConfig` or `EsploraConfig`.
        yuv_url: "http://127.0.0.1:18333".to_string(), // Provide a valid, accessible YUV node URL.
    };

    // Build a wallet from the config.
    let mut wallet = MemoryWallet::from_config(wallet_config)
        .await
        .expect("Couldn't init the wallet");

    // Don't forget to sync the wallet to fetch the UTXOs.
    wallet.sync(ydk::wallet::SyncOptions::default()).await.expect("Wallet should sync");

    // Init the blockchain. In this case, Esplora is used.
    let blockchain: Arc<AnyBlockchain> = Arc::new(
        EsploraBlockchain::new("http://127.0.0.1:30000", 20)
            .try_into()
            .expect("Esplora blockchain should be inited"),
    );

    // Build a YUV transaction.
    let tx = {
        let mut builder = wallet.build_transfer().expect("Tx should build");

        // Recipient `PublicKey`.
        let pubkey = PublicKey::from_str(
            "03ab5575d69e46968a528cd6fa2a35dd7808fea24a12b41dc65c7502108c75f9a9",
        )
        .unwrap();

        // `Chroma` that is to be transferred
        let chroma =
            Chroma::from_str("bcrt1p6gvky9eh0q6d3r0k4gs2l4m9qptm7yac09l37adhazqd7y3gcmtsmgpe0u")
                .unwrap();

        // Add a recipient and specify valid `Chroma`, receiver's `PublicKey`, YUV token amount and Satoshis amount.
        builder
            .add_recipient(chroma, &pubkey, 5000, 1000)
            .set_fee_rate_strategy(FeeRateStrategy::Manual { fee_rate: 2.0 });

        // Finish the transaction.
        builder
            .finish(&blockchain)
            .await
            .expect("Transaction should finish")
    };
}
```
