use std::sync::Arc;
use std::time::Duration;

use crate::config::{NodeConfig, StorageConfig};
use bitcoin_client::BitcoinRpcClient;
use event_bus::EventBus;
use eyre::{Context, Ok};
use tokio::select;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{error, info};
use yuv_controller::Controller;
use yuv_indexers::{AnnouncementsIndexer, BitcoinBlockIndexer, ConfirmationIndexer, RunParams};

use yuv_p2p::{
    client::{Handle, P2PClient},
    net::{ReactorTcp, Waker},
};
use yuv_rpc_server::ServerConfig;
use yuv_storage::{FlushStrategy, LevelDB, LevelDbOptions};
use yuv_tx_attach::GraphBuilder;
use yuv_tx_check::TxChecker;
use yuv_tx_confirm::TxConfirmator;
use yuv_types::{
    ControllerMessage, GraphBuilderMessage, IndexerMessage, TxCheckerMessage, TxConfirmMessage,
};

/// Default size of the channel for the event bus.
const DEFAULT_CHANNEL_SIZE: usize = 1000;
/// The limit of time to wait for the node to shutdown.
const DEFAULT_SHUTDOWN_TIMEOUT_SECS: u64 = 30;

/// Node encapsulate node service's start
pub struct Node {
    config: NodeConfig,
    event_bus: EventBus,
    txs_storage: LevelDB,
    state_storage: LevelDB,
    btc_client: Arc<BitcoinRpcClient>,

    cancelation: CancellationToken,
    pub(crate) task_tracker: TaskTracker,
}

impl Node {
    pub async fn new(config: NodeConfig) -> eyre::Result<Self> {
        let event_bus = Self::init_event_bus();
        let (txs_storage, state_storage) = Self::init_storage(config.storage.clone())?;

        let btc_client = Arc::new(
            BitcoinRpcClient::new(
                config.bnode.auth().clone(),
                config.bnode.url.clone(),
                config.bnode.timeout,
            )
            .await?,
        );

        Ok(Self {
            config,
            event_bus,
            txs_storage,
            state_storage,
            btc_client,
            cancelation: CancellationToken::new(),
            task_tracker: TaskTracker::new(),
        })
    }

    /// Wait for the signal from any node's service about the cancellation.
    pub async fn cancelled(&self) {
        self.cancelation.cancelled().await
    }

    /// The order of service starting is important if you want to index blocks first and then start
    /// listen to inbound messages.
    pub async fn run(&self) -> eyre::Result<()> {
        self.spawn_graph_builder();
        self.spawn_tx_checker()?;
        self.spawn_tx_confirmator();
        self.spawn_indexer().await?;

        let p2p_handle = self.spawn_p2p()?;
        self.spawn_controller(p2p_handle).await?;

        self.spawn_rpc();

        self.task_tracker.close();

        Ok(())
    }

    fn spawn_p2p(&self) -> eyre::Result<Handle<Waker>> {
        let p2p_client_runner = P2PClient::<ReactorTcp>::new(
            self.config.p2p.to_client_config(self.config.network)?,
            &self.event_bus,
        )
        .expect("P2P client must be successfully created");

        let handle = p2p_client_runner.handle();

        self.task_tracker
            .spawn(p2p_client_runner.run(self.cancelation.clone()));

        Ok(handle)
    }

    async fn spawn_controller(&self, handle: Handle<Waker>) -> eyre::Result<()> {
        let mut controller = Controller::new(
            &self.event_bus,
            self.txs_storage.clone(),
            self.state_storage.clone(),
            handle,
            self.config.storage.tx_per_page,
        )
        .set_inv_sharing_interval(Duration::from_secs(
            self.config.controller.inv_sharing_interval,
        ))
        .set_max_inv_size(self.config.controller.max_inv_size);

        controller.handle_mempool_txs().await?;

        self.task_tracker
            .spawn(controller.run(self.cancelation.clone()));

        Ok(())
    }

    fn spawn_graph_builder(&self) {
        let graph_builder = GraphBuilder::new(self.txs_storage.clone(), &self.event_bus);

        self.task_tracker
            .spawn(graph_builder.run(self.cancelation.clone()));
    }

    fn spawn_tx_checker(&self) -> eyre::Result<()> {
        let tx_checker = TxChecker::new(
            self.event_bus.clone(),
            self.txs_storage.clone(),
            self.state_storage.clone(),
        );

        self.task_tracker
            .spawn(tx_checker.run(self.cancelation.clone()));

        Ok(())
    }

    fn spawn_tx_confirmator(&self) {
        let tx_confirmator = TxConfirmator::new(
            &self.event_bus,
            self.btc_client.clone(),
            self.config.indexer.max_confirmation_time,
            self.config.indexer.clean_up_interval,
            self.config.indexer.confirmations_number,
        );

        self.task_tracker
            .spawn(tx_confirmator.run(self.cancelation.clone()));
    }

    fn spawn_rpc(&self) {
        let address = self.config.rpc.address.to_string();
        let max_items_per_request = self.config.rpc.max_items_per_request;
        let max_request_size_kb = self.config.rpc.max_request_size_kb;

        self.task_tracker.spawn(yuv_rpc_server::run_server(
            ServerConfig {
                address,
                max_items_per_request,
                max_request_size_kb,
            },
            self.txs_storage.clone(),
            self.state_storage.clone(),
            self.event_bus.clone(),
            self.btc_client.clone(),
            self.cancelation.clone(),
        ));
    }

    async fn spawn_indexer(&self) -> eyre::Result<()> {
        let mut indexer = BitcoinBlockIndexer::new(
            self.btc_client.clone(),
            self.state_storage.clone(),
            &self.event_bus,
        );

        indexer.add_subindexer(AnnouncementsIndexer::new(
            &self.event_bus,
            self.config.network,
        ));
        indexer.add_subindexer(ConfirmationIndexer::new(&self.event_bus));

        let restart_interval = self.config.indexer.restart_interval;
        let mut current_attempt = 1;
        while let Err(err) = indexer
            .init(
                self.config.indexer.clone().into(),
                self.config.indexer.blockloader.clone(),
                self.btc_client.clone(),
                self.config.indexer.confirmations_number as usize,
                self.cancelation.clone(),
            )
            .await
        {
            if current_attempt >= self.config.indexer.max_restart_attempts {
                return Err(err);
            }

            current_attempt += 1;
            error!(
                %err,
                "Failed to init the indexer. Trying again in {} secs",
                restart_interval.as_secs()
            );
            tokio::time::sleep(restart_interval).await;
        }

        self.task_tracker.spawn(indexer.run(
            RunParams {
                polling_period: self.config.indexer.polling_period,
            },
            self.cancelation.clone(),
        ));

        Ok(())
    }

    fn init_storage(config: StorageConfig) -> eyre::Result<(LevelDB, LevelDB)> {
        // Create directory if it does not exist
        if !config.path.exists() {
            std::fs::create_dir_all(&config.path)
                .wrap_err_with(|| format!("failed to create directory {:?}", config.path))?;
        }

        // Initialize storage for transactions
        let opt = LevelDbOptions {
            create_if_missing: config.create_if_missing,
            path: config.path.join("transactions"),
            flush_strategy: FlushStrategy::Ticker {
                period: config.flush_period,
            },
        };
        let txs_storage = LevelDB::from_opts(opt).wrap_err("failed to initialize storage")?;

        // Initialize storage for states
        let opt = LevelDbOptions {
            path: config.path.join("state"),
            create_if_missing: config.create_if_missing,
            flush_strategy: FlushStrategy::Ticker {
                period: config.flush_period,
            },
        };
        let state_storage = LevelDB::from_opts(opt).wrap_err("failed to initialize storage")?;

        Ok((txs_storage, state_storage))
    }

    fn init_event_bus() -> EventBus {
        let mut event_bus = EventBus::default();
        event_bus.register::<TxCheckerMessage>(Some(DEFAULT_CHANNEL_SIZE));
        event_bus.register::<GraphBuilderMessage>(Some(DEFAULT_CHANNEL_SIZE));
        event_bus.register::<ControllerMessage>(Some(DEFAULT_CHANNEL_SIZE));
        event_bus.register::<TxConfirmMessage>(Some(DEFAULT_CHANNEL_SIZE));
        event_bus.register::<IndexerMessage>(Some(DEFAULT_CHANNEL_SIZE));

        event_bus
    }

    pub async fn shutdown(&self) {
        info!("Shutting down node, finishing received requests...");

        self.cancelation.cancel();

        let timeout = self
            .config
            .shutdown_timeout
            .unwrap_or(DEFAULT_SHUTDOWN_TIMEOUT_SECS);

        select! {
            // Wait until all tasks are finished
            _ = self.task_tracker.wait() => {},
            // Or wait for and exit by timeout
            _ = sleep(Duration::from_secs(timeout)) => {
                info!("Shutdown timeout reached, exiting...");
            },
        }
    }
}
