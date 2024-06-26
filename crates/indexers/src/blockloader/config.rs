use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct BlockLoaderConfig {
    /// Number of workers which will load blocks
    #[serde(default = "default_workers_number")]
    pub workers_number: usize,
    /// The size of chunk which contains heights of blocks to load
    #[serde(default = "default_buffer_size")]
    pub chunk_size: usize,
    /// Sleep the worker for seconds when the worker exceeds the rate limit
    #[serde(default = "default_worker_time_sleep")]
    pub worker_time_sleep: usize,
}

fn default_workers_number() -> usize {
    10
}

fn default_buffer_size() -> usize {
    1000
}

fn default_worker_time_sleep() -> usize {
    3
}

impl Default for BlockLoaderConfig {
    fn default() -> Self {
        Self {
            workers_number: default_workers_number(),
            chunk_size: default_buffer_size(),
            worker_time_sleep: default_worker_time_sleep(),
        }
    }
}
