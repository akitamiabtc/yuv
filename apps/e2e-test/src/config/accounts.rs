use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct AccountsConfig {
    pub number: u32,
    pub funding_interval: u64,
    pub threshold: f32,
}
