#[macro_use]
extern crate criterion;

use std::sync::Arc;

use bitcoin_client::{BitcoinRpcApi, MockRpcApi};
use criterion::async_executor::FuturesExecutor;
use criterion::{black_box, BatchSize, Criterion};
use event_bus::{BusEvent, EventBus};
use eyre::WrapErr;
use tokio_util::sync::CancellationToken;

use yuv_storage::LevelDB;
use yuv_tx_check::{Config, TxCheckerWorkerPool};
use yuv_types::{ControllerMessage, GraphBuilderMessage, TxCheckerMessage};

use crate::tx_generator::TxGenerator;

mod common;
mod tx_generator;

/// Amount of messages generated per one benchmark iteration
const MSG_AMOUNT: u32 = 10;
/// Amount of transactions generated per one message
const TXS_PER_MSG: u32 = 1;

fn new_messages(
    msg_amount: u32,
    txs_per_message: u32,
    generator: &mut TxGenerator,
    mut rpc_api: Arc<MockRpcApi>,
) -> Vec<TxCheckerMessage> {
    let mut messages = Vec::new();

    let rpc_api = common::mut_mock(&mut rpc_api);

    for _ in 0..msg_amount {
        let mut yuv_txs = Vec::new();
        for _ in 0..txs_per_message {
            let yuv_tx = generator.get_next_yuv_tx();

            yuv_txs.push(yuv_tx.clone());

            rpc_api
                .expect_get_raw_transaction()
                .returning(move |_, _| Ok(yuv_tx.clone().bitcoin_tx));
        }
        messages.push(TxCheckerMessage::NewTxs {
            txs: yuv_txs,
            sender: None,
        })
    }

    messages
}

async fn spawn_tx_checker_worker_pool<BC: BitcoinRpcApi + Send + Sync + 'static>(
    size: usize,
    event_bus: &EventBus,
    txs_storage: LevelDB,
    state_storage: LevelDB,
    bitcoin_client: Arc<BC>,
    cancellation: CancellationToken,
) -> eyre::Result<()> {
    let worker_pool = TxCheckerWorkerPool::from_config(
        size,
        Config {
            full_event_bus: event_bus.clone(),
            txs_storage: txs_storage.clone(),
            state_storage: state_storage.clone(),
            bitcoin_client,
        },
    )
    .wrap_err("TxCheckers worker pool must run successfully")?;

    tokio::spawn(worker_pool.run(cancellation));
    Ok(())
}

pub fn init_event_bus() -> EventBus {
    let mut event_bus = EventBus::default();

    event_bus.register::<GraphBuilderMessage>(None);
    event_bus.register::<ControllerMessage>(None);
    event_bus.register::<TxCheckerMessage>(None);

    event_bus
}

pub async fn send_messages<E: BusEvent + Clone + 'static>(event_bus: &EventBus, messages: Vec<E>) {
    for msg in messages {
        event_bus.send(msg.clone()).await;
    }
}

#[tokio::main]
async fn tx_check_benchmark(c: &mut Criterion) {
    let event_bus = init_event_bus();
    let txs_storage = LevelDB::in_memory()
        .wrap_err("failed to initialize storage")
        .unwrap();
    let state_storage = LevelDB::in_memory()
        .wrap_err("failed to initialize storage")
        .unwrap();

    let mut tx_generator = TxGenerator::default();
    let rpc_api = Arc::new(MockRpcApi::default());

    let cancellation = CancellationToken::new();

    spawn_tx_checker_worker_pool(
        1000,
        &event_bus,
        txs_storage.clone(),
        state_storage.clone(),
        Arc::clone(&rpc_api),
        cancellation,
    )
    .await
    .expect("failed to start tx checker pool");

    c.bench_function("tx check benchmark", |b| {
        b.to_async(FuturesExecutor).iter_batched(
            || new_messages(MSG_AMOUNT, TXS_PER_MSG, &mut tx_generator, rpc_api.clone()),
            |messages| send_messages(&event_bus, black_box(messages)),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, tx_check_benchmark);
criterion_main!(benches);
