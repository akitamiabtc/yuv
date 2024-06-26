use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct NodesConfig {
    pub yuv: Vec<String>,
    pub bitcoin: Vec<BitcoinNode>,
    pub esplora: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BitcoinNode {
    pub url: String,
    #[serde(default)]
    pub auth: Option<BitcoinAuth>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BitcoinAuth {
    /// User name for the bitcoin node
    pub username: String,
    /// Password for the bitcoin node
    pub password: String,
}
