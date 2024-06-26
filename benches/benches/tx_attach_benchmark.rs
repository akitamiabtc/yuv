#[macro_use]
extern crate criterion;

use criterion::async_executor::FuturesExecutor;
use criterion::{black_box, BatchSize, Criterion};
use event_bus::{BusEvent, EventBus};
use eyre::WrapErr;
use tokio_util::sync::CancellationToken;

use yuv_storage::LevelDB;
use yuv_tx_attach::GraphBuilder;
use yuv_types::{ControllerMessage, GraphBuilderMessage};

use crate::tx_generator::TxGenerator;

mod tx_generator;

/// Amount of messages generated per one benchmark iteration
const MSG_AMOUNT: u32 = 10;
/// Amount of transactions generated per one message
const TXS_PER_MSG: u32 = 1;

pub fn init_event_bus() -> EventBus {
    let mut event_bus = EventBus::default();

    event_bus.register::<ControllerMessage>(None);
    event_bus.register::<GraphBuilderMessage>(None);

    event_bus
}

fn new_messages(
    msg_amount: u32,
    txs_per_message: u32,
    generator: &mut TxGenerator,
) -> Vec<GraphBuilderMessage> {
    let mut messages = Vec::new();

    for _ in 0..msg_amount {
        let mut yuv_txs = Vec::new();
        for _ in 0..txs_per_message {
            let yuv_tx = generator.get_next_yuv_tx();

            yuv_txs.push(yuv_tx);
        }
        messages.push(GraphBuilderMessage::CheckedTxs(yuv_txs));
    }

    messages
}

async fn spawn_graph_builder(
    event_bus: &EventBus,
    txs_storage: LevelDB,
    cancellation: CancellationToken,
) {
    let graph_builder = GraphBuilder::new(txs_storage, event_bus, 100);

    tokio::spawn(graph_builder.run(cancellation));
}

pub async fn send_messages<E: BusEvent + Clone + 'static>(event_bus: &EventBus, messages: Vec<E>) {
    for msg in messages {
        event_bus.send(msg.clone()).await;
    }
}

#[tokio::main]
async fn tx_attach_benchmark(c: &mut Criterion) {
    let event_bus = init_event_bus();
    let txs_storage = LevelDB::in_memory()
        .wrap_err("failed to initialize storage")
        .unwrap();

    let cancellation = CancellationToken::new();

    spawn_graph_builder(&event_bus, txs_storage.clone(), cancellation).await;

    let mut tx_generator = TxGenerator::default();

    c.bench_function("tx attach benchmark", |b| {
        b.to_async(FuturesExecutor).iter_batched(
            || new_messages(MSG_AMOUNT, TXS_PER_MSG, &mut tx_generator),
            |messages| send_messages(&event_bus, black_box(messages)),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, tx_attach_benchmark);
criterion_main!(benches);
