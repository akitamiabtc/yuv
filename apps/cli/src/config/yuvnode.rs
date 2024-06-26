use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct YuvNodeConfig {
    pub url: String,
}
