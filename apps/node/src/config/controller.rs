use serde::Deserialize;

pub const DEFAULT_MAX_INV_SIZE: usize = 100;
pub const DEFAULT_INV_SHARING_INTERVAL: u64 = 10;

#[derive(Deserialize)]
pub struct ControllerConfig {
    /// Max inventory size
    #[serde(default = "default_max_inv_size")]
    pub max_inv_size: usize,
    /// Interval between inventory sharing in seconds
    #[serde(default = "default_inv_sharing_interval")]
    pub inv_sharing_interval: u64,
}

fn default_max_inv_size() -> usize {
    DEFAULT_MAX_INV_SIZE
}

fn default_inv_sharing_interval() -> u64 {
    DEFAULT_INV_SHARING_INTERVAL
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            max_inv_size: default_max_inv_size(),
            inv_sharing_interval: default_inv_sharing_interval(),
        }
    }
}
