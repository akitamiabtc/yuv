use std::sync::Arc;

use bitcoin_client::BitcoinRpcClient;
use event_bus::EventBus;
use jsonrpsee::server::Server;
use tokio_util::sync::CancellationToken;

use yuv_rpc_api::transactions::YuvTransactionsRpcServer;
use yuv_storage::{
    ChromaInfoStorage, FrozenTxsStorage, MempoolEntryStorage, PagesStorage, TransactionsStorage,
};

use crate::transactions::TransactionsController;

pub mod transactions;

pub struct ServerConfig {
    /// Address at which the server will listen for incoming connections.
    pub address: String,
    /// Max number of items to request/process per incoming request.
    pub max_items_per_request: usize,
    /// Max size of incoming request in kilobytes.
    pub max_request_size_kb: u32,
}

/// Runs YUV Node's RPC server.
pub async fn run_server<TS, SS>(
    ServerConfig {
        address,
        max_items_per_request,
        max_request_size_kb,
    }: ServerConfig,
    txs_storage: TS,
    state_storage: SS,
    full_event_bus: EventBus,
    bitcoin_client: Arc<BitcoinRpcClient>,
    cancellation: CancellationToken,
) -> eyre::Result<()>
where
    TS: TransactionsStorage + PagesStorage + Clone + Send + Sync + 'static,
    SS: FrozenTxsStorage + ChromaInfoStorage + MempoolEntryStorage + Clone + Send + Sync + 'static,
{
    // The multiplication of average transaction size and max number of items
    // per request approximately gives the maximum JSON RPC request size.
    //
    // See `providelistyuvproofs`

    let server = Server::builder()
        .max_request_body_size(max_request_size_kb * 1024)
        .build(address)
        .await?;

    let handle = server.start(
        TransactionsController::new(
            txs_storage,
            full_event_bus,
            state_storage,
            bitcoin_client,
            max_items_per_request,
        )
        .into_rpc(),
    );

    // Await until stop message received
    cancellation.cancelled().await;

    // Send stop message to server
    if let Err(err) = handle.stop() {
        tracing::trace!("Failed to stop server: {}", err);
    }

    // Wait until server stopped
    handle.stopped().await;

    Ok(())
}
