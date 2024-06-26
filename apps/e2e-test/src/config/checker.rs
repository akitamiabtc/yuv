use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct CheckerConfig {
    pub threshold: u64,
    pub check_balances_matching: bool,
}
