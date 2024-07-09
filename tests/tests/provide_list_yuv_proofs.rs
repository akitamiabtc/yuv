//! This integration test issues USD coins to Bob and Alice, performs a batch of transfers between them
//! and sends related proofs to the first YUV node. Then generated txs are provided to another YUV node,
//! that is not connected to the first one, with the `providelistyuvproofs` method and test checks that both
//! nodes accepted them.

use bdk::{bitcoincore_rpc::RpcApi, miniscript::ToPublicKey};
use bitcoin::{secp256k1::Secp256k1, PrivateKey};
use once_cell::sync::Lazy;

mod common;
use common::*;
use ydk::{types::FeeRateStrategy, wallet::SyncOptions};
use yuv_rpc_api::transactions::{ProvideYuvProofRequest, YuvTransactionsRpcClient};

const NUMBER_OF_TRANSFERS: i32 = 2;
const EXTRA_YUV_NODE_URL: &str = "http://127.0.0.1:18335";

static USD_ISSUER: Lazy<PrivateKey> = Lazy::new(|| {
    "cNMMXcLoM65N5GaULU7ct2vexmQnJ5i5j3Sjc6iNnEF18vY7gzn9"
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
async fn test_provide_list_yuv_proofs() -> eyre::Result<()> {
    let blockchain_rpc = setup_rpc_blockchain(&USD_ISSUER)?;

    let provider_cfg = bitcoin_provider_config(false);
    let blockchain = setup_blockchain(&provider_cfg);

    // Set up two YUV nodes
    let yuv_client_1 = setup_yuv_client(YUV_NODE_URL)?;
    let yuv_client_2 = setup_yuv_client(EXTRA_YUV_NODE_URL)?;

    let usd_issuer = setup_wallet_from_provider(*USD_ISSUER, provider_cfg.clone()).await?;

    let alice = setup_wallet_from_provider(*ALICE, provider_cfg.clone()).await?;

    let bob = setup_wallet_from_provider(*BOB, provider_cfg.clone()).await?;

    let secp = Secp256k1::new();

    blockchain_rpc.generate_to_address(101, &usd_issuer.address()?)?;

    usd_issuer.sync(SyncOptions::default()).await?;

    const ISSUANCE_AMOUNT: u128 = 10_000;

    let alice_pubkey = ALICE.public_key(&secp);

    let fee_rate_strategy = FeeRateStrategy::Manual { fee_rate: 2.0 };

    // =============================
    // 1. Issue USD tokens to ALICE
    // =============================
    let usd_issuance = {
        let mut builder = usd_issuer.build_issuance(None)?;

        builder
            .add_recipient(&alice_pubkey.inner, ISSUANCE_AMOUNT, 1000)
            // Fund alice with 50_000 sats
            .add_sats_recipient(&alice_pubkey.inner, 50_000)
            .set_fee_rate_strategy(fee_rate_strategy);

        builder.finish(&blockchain).await?
    };

    // let fee_rate = fee_rate_strategy.get_fee_rate(&provider)?;

    // TODO: Failed estimation on regtest
    // assert_fee_matches_difference(&usd_issuance, &provider, fee_rate, true)?;

    let usd_txid = usd_issuance.bitcoin_tx.txid();

    // This vector is later used to provide txs to another YUV node
    let mut raw_txs = Vec::new();
    raw_txs.push(ProvideYuvProofRequest::new(
        usd_txid,
        usd_issuance.tx_type.clone(),
        None,
    ));

    yuv_client_1.send_yuv_tx(usd_issuance.hex(), None).await?;

    // Add block with issuance to the chain
    blockchain_rpc.generate_to_address(1, &alice.address()?)?;

    let tx = wait_until_reject_or_attach(usd_txid, &yuv_client_1).await?;

    assert_attached!(tx, "USD issuance should be attached");
    println!("USD issuance attached");

    alice.sync(SyncOptions::default()).await?;

    assert_wallet_has_utxo!(alice, usd_txid, 0, "Alice should have USD issuance utxo");

    let bob_pubkey = BOB.public_key(&secp);

    const TRANSFER_AMOUNT: u128 = 100;

    // =============================
    // 2. Transfer USD tokens from ALICE to BOB
    // =============================
    println!(
        "waiting for {} transfers to be attached",
        NUMBER_OF_TRANSFERS
    );
    for i in 0..NUMBER_OF_TRANSFERS {
        let alice_bob_transfer = {
            let chroma = USD_ISSUER.public_key(&secp).to_x_only_pubkey().into();

            let mut builder = alice.build_transfer()?;

            builder
                .add_recipient(chroma, &bob_pubkey.inner, TRANSFER_AMOUNT, 1000)
                .set_fee_rate_strategy(fee_rate_strategy);

            builder.finish(&blockchain).await?
        };

        // TODO: Failed estimation on regtest
        // assert_fee_matches_difference(&alice_bob_transfer, &provider, fee_rate, false)?;

        let txid = alice_bob_transfer.bitcoin_tx.txid();

        raw_txs.push(ProvideYuvProofRequest::new(
            txid,
            alice_bob_transfer.tx_type.clone(),
            None,
        ));

        yuv_client_1
            .send_yuv_tx(alice_bob_transfer.hex(), None)
            .await?;

        // Add block with transfer to the chain
        blockchain_rpc.generate_to_address(7, &alice.address()?)?;

        let tx = wait_until_reject_or_attach(txid, &yuv_client_1).await?;

        assert_attached!(tx, "USD transfer should be attached");
        println!("{} transfer attached", i + 1);

        bob.sync(SyncOptions::default()).await?;

        assert_wallet_has_utxo!(bob, txid, 0, "Bob should have utxo from transfer");

        // Prevents the `bad-txns-inputs-missingorspent` error
        alice.sync(SyncOptions::default()).await?;
    }

    blockchain_rpc.generate_to_address(7, &alice.address()?)?;

    println!(
        "Verifying {} txs on the second node",
        NUMBER_OF_TRANSFERS + 1
    );

    // Provide txs to another YUV node
    yuv_client_2
        .provide_list_yuv_proofs(raw_txs.clone())
        .await?;

    // Wait for the txs to get attached
    for raw_tx in raw_txs {
        let tx = wait_until_reject_or_attach(raw_tx.txid, &yuv_client_2).await?;

        assert_attached!(tx, "USD transfer should be attached");
    }

    Ok(())
}
