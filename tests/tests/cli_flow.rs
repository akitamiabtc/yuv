//! This intergration test checks that flow from `yuv-cli` documentation works
//! as expected.

use bdk::bitcoin::{secp256k1::Secp256k1, OutPoint, PrivateKey};
use bdk::bitcoincore_rpc::RpcApi;
use bdk::miniscript::ToPublicKey;
use once_cell::sync::Lazy;

mod common;
use common::*;

use ydk::types::FeeRateStrategy;
use ydk::wallet::{MemoryWallet, SyncOptions};
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;

static USD_ISSUER: Lazy<PrivateKey> = Lazy::new(|| {
    "cNMMXcLoM65N5GaULU7ct2vexmQnJ5i5j3Sjc6iNnEF18vY7gzn9"
        .parse()
        .expect("Should be valid key")
});

static EUR_ISSUER: Lazy<PrivateKey> = Lazy::new(|| {
    "cUK2ZdLQWWpKeFcrrD7BBjiUsEns9M3MFBTkmLTXyzs66TQN72eX"
        .parse()
        .expect("Should be valid key")
});

static ALICE: Lazy<PrivateKey> = Lazy::new(|| {
    "cQb7JarJTBoeu6eLvyDnHYNr6Hz4AuAnELutxcY478ySZy2i29FA"
        .parse()
        .expect("Should be valid key")
});

static BOB: Lazy<PrivateKey> = Lazy::new(|| {
    "cUrMc62nnFeQuzXb26KPizCJQPp7449fsPsqn5NCHTwahSvqqRkV"
        .parse()
        .expect("Should be valid key")
});

#[tokio::test]
async fn test_cli_flow() -> eyre::Result<()> {
    let rpc_blockchain = setup_rpc_blockchain(&USD_ISSUER)?;
    let yuv_client = setup_yuv_client(YUV_NODE_URL)?;

    let provider_cfg = bitcoin_provider_config(false);
    let blockchain = setup_blockchain(&provider_cfg);

    let usd_issuer = setup_wallet_from_provider(*USD_ISSUER, provider_cfg.clone()).await?;

    let eur_issuer = setup_wallet_from_provider(*EUR_ISSUER, provider_cfg.clone()).await?;

    let alice = setup_wallet_from_provider(*ALICE, provider_cfg.clone()).await?;

    let bob = setup_wallet_from_provider(*BOB, provider_cfg.clone()).await?;

    let secp = Secp256k1::new();

    rpc_blockchain.generate_to_address(101, &usd_issuer.address()?)?;
    usd_issuer.sync(SyncOptions::default()).await?;

    const ISSUANCE_AMOUNT: u128 = 10_000;

    let alice_pubkey = ALICE.public_key(&secp);
    let eur_pubkey = EUR_ISSUER.public_key(&secp);

    let fee_rate_strategy = FeeRateStrategy::Manual { fee_rate: 2.0 };

    // =============================
    // 1. Issue USD tokens to ALICE
    // =============================
    let usd_issuance = {
        let mut builder = usd_issuer.build_issuance(None)?;

        builder
            .add_recipient(&alice_pubkey.inner, ISSUANCE_AMOUNT, 1000)
            // Fund alice with 10_000 sats
            .add_sats_recipient(&alice_pubkey.inner, 10_000)
            // Fund eur issuer with 10_000 sats for further issuance
            .add_sats_recipient(&eur_pubkey.inner, 10_000)
            .set_fee_rate_strategy(fee_rate_strategy);

        builder.finish(&blockchain).await?
    };

    // let fee_rate = fee_rate_strategy.get_fee_rate(&provider)?;

    // TODO: Failed estimation on regtest
    // assert_fee_matches_difference(&usd_issuance, &provider, fee_rate, true)?;

    let usd_txid = usd_issuance.bitcoin_tx.txid();

    yuv_client.send_yuv_tx(usd_issuance.hex(), None).await?;

    // Add block with issuance to the chain
    rpc_blockchain.generate_to_address(7, &alice.address()?)?;

    let tx = wait_until_reject_or_attach(usd_txid, &yuv_client).await?;

    assert_attached!(tx, "USD issuance should be attached");
    println!("USD issuance attached");

    // To sync output with satoshis for next transaction.
    eur_issuer.sync(SyncOptions::default()).await?;

    // =============================
    // 2. Issue EUR tokens to ALICE
    // =============================
    let eur_issuance = {
        let mut builder = eur_issuer.build_issuance(None)?;

        builder
            .add_recipient(&alice_pubkey.inner, ISSUANCE_AMOUNT, 1000)
            .set_fee_rate_strategy(fee_rate_strategy);

        builder.finish(&blockchain).await?
    };

    // TODO: Failed estimation on regtest
    // assert_fee_matches_difference(&eur_issuance, &provider, fee_rate, true)?;

    let eur_txid = eur_issuance.bitcoin_tx.txid();

    yuv_client.send_yuv_tx(eur_issuance.hex(), None).await?;

    // Add block with issuance to the chain
    rpc_blockchain.generate_to_address(7, &alice.address()?)?;

    let tx = wait_until_reject_or_attach(eur_txid, &yuv_client).await?;

    assert_attached!(tx, "EUR issuance should be attached");
    println!("EUR issuance attached");

    alice.sync(SyncOptions::default()).await?;

    assert_wallet_has_utxo!(alice, usd_txid, 0, "Alice should have USD issuance utxo");
    assert_wallet_has_utxo!(alice, eur_txid, 0, "Alice should have EUR issuance utxo");

    let bob_pubkey = BOB.public_key(&secp);

    const TRANSFER_AMOUNT: u128 = 100;

    // =============================
    // 3. Transfer USD tokens from ALICE to BOB
    // =============================
    let alice_bob_transfer = {
        let usd_chroma = USD_ISSUER.public_key(&secp).to_x_only_pubkey().into();
        let eur_chroma = EUR_ISSUER.public_key(&secp).to_x_only_pubkey().into();

        let mut builder = alice.build_transfer()?;

        builder
            .add_recipient(usd_chroma, &bob_pubkey.inner, TRANSFER_AMOUNT, 1000)
            .add_recipient(eur_chroma, &bob_pubkey.inner, TRANSFER_AMOUNT, 1000)
            .set_fee_rate_strategy(fee_rate_strategy);

        builder.finish(&blockchain).await?
    };

    // TODO: Failed estimation on regtest
    // assert_fee_matches_difference(&alice_bob_transfer, &provider, fee_rate, false)?;

    let txid = alice_bob_transfer.bitcoin_tx.txid();

    yuv_client
        .send_yuv_tx(alice_bob_transfer.hex(), None)
        .await?;

    // Add block with transfer to the chain
    rpc_blockchain.generate_to_address(7, &alice.address()?)?;

    let tx = wait_until_reject_or_attach(txid, &yuv_client).await?;

    assert_attached!(tx, "USD transfer should be attached");

    bob.sync(SyncOptions::default()).await?;

    assert_wallet_has_utxo!(bob, txid, 0, "Bob should have utxo from transfer");

    Ok(())
}

pub fn find_in_utxos(wallet: &MemoryWallet, outpoint: OutPoint) -> eyre::Result<()> {
    let utxos = wallet.yuv_utxos();

    let _utxo = utxos
        .iter()
        .find(|(outpoint_, _)| *outpoint_ == &outpoint)
        .ok_or_else(|| eyre::eyre!("UTXO not found"))?;

    Ok(())
}
