use bitcoin_client::BitcoinRpcAuth;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize, Clone)]
pub struct BitcoinConfig {
    /// Url to the bitcoin node
    pub url: String,
    /// Authentication for the bitcoin node
    #[serde(default)]
    pub auth: Option<BitcoinAuth>,
    /// The timeout after which requests will abort if they aren't finished.
    #[serde(default)]
    pub timeout: Option<Duration>,
}

#[derive(Deserialize, Clone)]
pub struct BitcoinAuth {
    /// User name for the bitcoin node
    pub username: String,
    /// Password for the bitcoin node
    pub password: String,
}

impl BitcoinConfig {
    pub fn auth(&self) -> BitcoinRpcAuth {
        match &self.auth {
            Some(auth) => BitcoinRpcAuth::UserPass {
                username: auth.username.clone(),
                password: auth.password.clone(),
            },
            None => BitcoinRpcAuth::None,
        }
    }
}
