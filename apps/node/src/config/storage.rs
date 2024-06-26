use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use yuv_storage::DEFAULT_FLUSH_PERIOD_SECS;

pub const DEFAULT_TX_PER_PAGE: u64 = 100;

#[derive(Serialize, Deserialize, Clone)]
pub struct StorageConfig {
    /// Path to directory in which node will store all its
    /// data.
    pub path: PathBuf,

    #[serde(default = "default_flush_period")]
    pub flush_period: u64,

    /// Create if missing database file
    #[serde(default = "default_create_if_missing")]
    pub create_if_missing: bool,

    /// Transactions per one page
    #[serde(default = "default_tx_per_page")]
    pub tx_per_page: u64,
}

fn default_flush_period() -> u64 {
    DEFAULT_FLUSH_PERIOD_SECS
}

fn default_tx_per_page() -> u64 {
    DEFAULT_TX_PER_PAGE
}

fn default_create_if_missing() -> bool {
    true
}
