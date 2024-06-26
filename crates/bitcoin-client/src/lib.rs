mod client;
pub use client::{Auth as BitcoinRpcAuth, Client as BitcoinRpcClient, Error, Result};

mod rpc_api;
pub use rpc_api::{RawTx, RpcApi as BitcoinRpcApi};

#[cfg(feature = "mocks")]
pub use rpc_api::MockRpcApi;

pub mod json;
mod queryable;

pub use jsonrpc::Error as JsonRpcError;

pub mod constants;
