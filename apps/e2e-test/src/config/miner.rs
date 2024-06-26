use std::time::Duration;

use serde::Deserialize;

const DEFAULT_MINING_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct MinerConfig {
    #[serde(default = "default_mining_interval")]
    pub interval: Duration,
}

fn default_mining_interval() -> Duration {
    DEFAULT_MINING_INTERVAL
}

impl Default for MinerConfig {
    fn default() -> Self {
        Self {
            interval: default_mining_interval(),
        }
    }
}
