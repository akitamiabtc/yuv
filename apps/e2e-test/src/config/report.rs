use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct ReportConfig {
    pub result_path: PathBuf,
    pub error_log_file: PathBuf,
}
