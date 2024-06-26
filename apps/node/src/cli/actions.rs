use std::sync::Arc;
use tokio::select;
use tokio::signal::unix;
use tokio::signal::unix::SignalKind;

use crate::{
    cli::{arguments, node::Node},
    config::NodeConfig,
};
use tracing::{level_filters::LevelFilter, Event, Level, Subscriber};
use tracing_subscriber::{
    filter::Targets,
    fmt::format::{DefaultVisitor, Writer},
    layer::Layer,
    prelude::*,
    util::SubscriberInitExt,
    EnvFilter,
};

pub async fn run(args: arguments::Run) -> eyre::Result<()> {
    let config = NodeConfig::from_path(args.config)?;

    let level_filter = config.logger.level;

    let filter = Targets::new()
        .with_target("yuv_indexers", level_filter)
        .with_target("yuv_controller", level_filter)
        .with_target("yuv_rpc_server", level_filter)
        .with_target("yuv_network", level_filter)
        .with_target("yuv_tx_attach", level_filter)
        .with_target("yuv_tx_check", level_filter)
        .with_target("yuv_p2p", level_filter)
        .with_default(level_filter);

    // Disable `hyper_util` logs emitting from the `jsonrpc` crate.
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env()?
        .add_directive("hyper_util=info".parse()?);

    tracing_subscriber::registry()
        .with(YuvTracer.with_filter(filter))
        .with(env_filter)
        .try_init()?;

    // Start all main components, but do not start external services
    // like RPC, p2p until indexer will be initialized.

    let node = Arc::new(Node::new(config).await?);
    let node_clone = node.clone();

    tokio::spawn(async move {
        if let Err(err) = node_clone.run().await {
            tracing::error!("Node cancelled: {:?}", err);
        }
        node_clone.task_tracker.close();
    });

    let mut sigterm =
        unix::signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal handler");
    let mut sigint =
        unix::signal(SignalKind::interrupt()).expect("Failed to create SIGINT signal handler");

    select! {
        _ = node.cancelled() => {
            tracing::info!("Node run failed");
        }
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM signal");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT signal");
        }
    }

    node.shutdown().await;

    Ok(())
}

struct YuvTracer;

impl<S> Layer<S> for YuvTracer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let target = match event.metadata().level() {
            &Level::INFO | &Level::WARN | &Level::ERROR => event
                .metadata()
                .target()
                .split("::")
                .last()
                .unwrap_or_default(),
            _ => event.metadata().target(),
        };

        print!(
            "[{}] {} {}: ",
            chrono::offset::Local::now().format("%Y-%m-%d %H:%M:%S"),
            event.metadata().level(),
            target,
        );

        let mut message = String::new();

        event.record(&mut DefaultVisitor::new(Writer::new(&mut message), true));

        println!("{}", message);
    }
}
