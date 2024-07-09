#![doc = include_str!("../README.md")]

mod errors;
pub use errors::CheckError;

mod isolated_checks;
pub use isolated_checks::check_transaction;

mod service;
pub use service::TxChecker;

mod announcements;
mod script_parser;

#[cfg(test)]
mod tests;
