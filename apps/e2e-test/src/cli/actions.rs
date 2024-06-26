use tracing::level_filters::LevelFilter;
use tracing::{info, Level, Metadata};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::{fmt, Layer, Registry};

use crate::cli::arguments;
use crate::config::TestConfig;

use super::e2e::E2e;

struct ErrorLogFilter;
impl<S> Filter<S> for ErrorLogFilter {
    fn enabled(&self, meta: &Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        *meta.level() == Level::ERROR
    }
}

pub async fn run(args: arguments::Run) -> eyre::Result<()> {
    let config = TestConfig::from_path(args.config)?;

    let file_appender =
        RollingFileAppender::new(Rotation::MINUTELY, "", &config.report.error_log_file);
    let subscriber = Registry::default()
        .with(
            fmt::layer()
                .compact()
                .with_target(false)
                .with_filter(LevelFilter::INFO),
        )
        .with(
            fmt::layer()
                .json()
                .with_writer(file_appender)
                .with_target(false)
                .with_filter(ErrorLogFilter),
        );

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set up tracing subscriber");

    let e2e = E2e::new(config.clone()).await?;
    e2e.run().await?;

    if let Some(duration) = config.duration {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Cancellation received");
            }
            _ = tokio::time::sleep(duration) => {
                info!("Test duration is reached, exiting");
            }
        }
    } else {
        tokio::signal::ctrl_c().await?;
    }

    e2e.shutdown().await;

    info!(
        r#"
    The End-to-End test has successfully ended
    Check {:?} for the result
    Errors (if any) were written to files with the {:?} prefix"#,
        config.report.result_path, config.report.error_log_file
    );

    Ok(())
}
