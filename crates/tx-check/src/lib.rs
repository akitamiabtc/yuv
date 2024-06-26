#![doc = include_str!("../README.md")]

mod errors;
pub use errors::CheckError;

mod isolated_checks;
pub use isolated_checks::check_transaction;

mod worker;
pub use worker::{Config, TxCheckerWorker};

mod worker_pool;
pub use worker_pool::TxCheckerWorkerPool;

mod announcements;

#[cfg(test)]
mod tests;
