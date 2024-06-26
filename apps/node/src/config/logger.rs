use std::str::FromStr;

use serde::Deserialize;
use tracing::metadata::Level;

#[derive(Deserialize)]
pub struct LoggerConfig {
    #[serde(default = "default_level", deserialize_with = "deserialize_level")]
    pub level: Level,
}

fn deserialize_level<'de, D>(deserializer: D) -> Result<Level, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    Level::from_str(&s).map_err(serde::de::Error::custom)
}

fn default_level() -> Level {
    Level::INFO
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
        }
    }
}
