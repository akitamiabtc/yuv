use async_trait::async_trait;
use bitcoin::hashes::hex;
use bitcoin::secp256k1;
use log::Level::{Debug, Trace, Warn};
use log::{debug, log_enabled, trace};
use serde::*;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use crate::{BitcoinRpcApi, JsonRpcError};

/// The different authentication methods for the client.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(untagged)]
pub enum Auth {
    /// None authentication
    None,
    /// Authentication with username and password, usually [Auth::Cookie] should be preferred
    UserPass {
        /// Username
        username: String,
        /// Password
        password: String,
    },
    /// Authentication with a cookie file
    Cookie {
        /// Cookie file
        file: PathBuf,
    },
}

impl Auth {
    /// Convert into the arguments that jsonrpc::Client needs.
    fn get_user_pass(self) -> Result<Option<(String, String)>> {
        use std::io::Read;
        match self {
            Auth::None => Ok(None),
            Auth::UserPass { username, password } => Ok(Some((username, password))),
            Auth::Cookie { file } => {
                let mut file = File::open(file)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                let mut split = contents.splitn(2, ':');
                let u = split.next().ok_or(Error::InvalidCookieFile)?.into();
                let p = split.next().ok_or(Error::InvalidCookieFile)?.into();
                Ok(Some((u, p)))
            }
        }
    }
}

/// Client implements a JSON-RPC client for the Bitcoin Core daemon or compatible APIs.
pub struct Client {
    client: jsonrpc::client::Client,
}

impl Client {
    /// Creates a client to a bitcoind JSON-RPC server.
    ///
    /// Can only return [Err] when using cookie authentication.
    pub async fn new(auth: Auth, url: String, timeout: Option<Duration>) -> Result<Self> {
        let mut client = jsonrpc::http::reqwest_http::Builder::new()
            .url(&url)
            .map_err(|e| Error::JsonRpc(e.into()))?;

        if let Some((user, pass)) = auth.get_user_pass()? {
            client = client.auth(user, Some(pass));
        }

        if let Some(timeout) = timeout {
            client = client.timeout(timeout);
        }

        Ok(Self {
            client: jsonrpc::client::Client::with_transport(client.build()),
        })
    }

    pub fn from_jsonrpc(client: jsonrpc::client::Client) -> Self {
        Self { client }
    }

    /// Get the underlying JSONRPC client.
    pub fn get_jsonrpc_client(&self) -> &jsonrpc::client::Client {
        &self.client
    }
}

#[async_trait]
impl BitcoinRpcApi for Client {
    /// Call an `cmd` rpc with given `args` list
    async fn call<T: for<'a> de::Deserialize<'a>>(
        &self,
        cmd: &str,
        args: &[serde_json::Value],
    ) -> Result<T> {
        let v_args: Vec<_> = args
            .iter()
            .map(serde_json::value::to_raw_value)
            .collect::<std::result::Result<_, serde_json::Error>>()?;
        let req = self.client.build_request(cmd, &v_args[..]);
        if log_enabled!(Debug) {
            debug!(target: "bitcoincore_rpc", "JSON-RPC request: {} {}", cmd, serde_json::Value::from(args));
        }

        let resp = self.client.send_request(req).await.map_err(Error::from);
        log_response(cmd, &resp);
        Ok(resp?.result()?)
    }
}

fn log_response(cmd: &str, resp: &Result<jsonrpc::Response>) {
    if log_enabled!(Warn) || log_enabled!(Debug) || log_enabled!(Trace) {
        match resp {
            Err(ref e) => {
                if log_enabled!(Debug) {
                    debug!(target: "bitcoincore_rpc", "JSON-RPC failed parsing reply of {}: {:?}", cmd, e);
                }
            }
            Ok(ref resp) => {
                if let Some(ref e) = resp.error {
                    if log_enabled!(Debug) {
                        debug!(target: "bitcoincore_rpc", "JSON-RPC error for {}: {:?}", cmd, e);
                    }
                } else if log_enabled!(Trace) {
                    let rawnull =
                        serde_json::value::to_raw_value(&serde_json::Value::Null).unwrap();
                    let result = resp.result.as_ref().unwrap_or(&rawnull);
                    trace!(target: "bitcoincore_rpc", "JSON-RPC response for {}: {}", cmd, result);
                }
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// The error type for errors produced in this library.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] JsonRpcError),

    #[error("hex decode error: {0}")]
    Hex(#[from] hex::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::error::Error),

    #[error("Bitcoin serialization error: {0}")]
    BitcoinSerialization(#[from] bitcoin::consensus::encode::Error),

    #[error("secp256k1 error: {0}")]
    Secp256k1(#[from] secp256k1::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid amount: {0}")]
    InvalidAmount(#[from] bitcoin::amount::ParseAmountError),

    #[error("invalid cookie file")]
    InvalidCookieFile,

    /// The JSON result had an unexpected structure.
    #[error("the JSON result had an unexpected structure")]
    UnexpectedStructure,

    #[error("Unsupported version Bitcoin Core RPC")]
    UnsupportedVersion,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc_api::{handle_defaults, into_json, null};
    use bitcoin::hashes::hex::FromHex;
    use bitcoin::Transaction;

    #[tokio::test]
    async fn test_raw_tx() {
        use bitcoin::consensus::encode;
        let client = Client::new(Auth::None, "http://localhost/".into(), None)
            .await
            .unwrap();
        let tx: Transaction = encode::deserialize(&Vec::<u8>::from_hex("0200000001586bd02815cf5faabfec986a4e50d25dbee089bd2758621e61c5fab06c334af0000000006b483045022100e85425f6d7c589972ee061413bcf08dc8c8e589ce37b217535a42af924f0e4d602205c9ba9cb14ef15513c9d946fa1c4b797883e748e8c32171bdf6166583946e35c012103dae30a4d7870cd87b45dd53e6012f71318fdd059c1c2623b8cc73f8af287bb2dfeffffff021dc4260c010000001976a914f602e88b2b5901d8aab15ebe4a97cf92ec6e03b388ac00e1f505000000001976a914687ffeffe8cf4e4c038da46a9b1d37db385a472d88acfd211500").unwrap()).unwrap();

        assert!(client.send_raw_transaction(&tx).await.is_err());
        assert!(client
            .send_raw_transaction(&encode::serialize(&tx))
            .await
            .is_err());
        assert!(client.send_raw_transaction("deadbeef").await.is_err());
        assert!(client
            .send_raw_transaction("deadbeef".to_owned())
            .await
            .is_err());
    }

    fn test_handle_defaults_inner() -> Result<()> {
        {
            let mut args = [into_json(0)?, null(), null()];
            let defaults = [into_json(1)?, into_json(2)?];
            let res = [into_json(0)?];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [into_json(0)?, into_json(1)?, null()];
            let defaults = [into_json(2)?];
            let res = [into_json(0)?, into_json(1)?];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [into_json(0)?, null(), into_json(5)?];
            let defaults = [into_json(2)?, into_json(3)?];
            let res = [into_json(0)?, into_json(2)?, into_json(5)?];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [into_json(0)?, null(), into_json(5)?, null()];
            let defaults = [into_json(2)?, into_json(3)?, into_json(4)?];
            let res = [into_json(0)?, into_json(2)?, into_json(5)?];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [null(), null()];
            let defaults = [into_json(2)?, into_json(3)?];
            let res: [serde_json::Value; 0] = [];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [null(), into_json(1)?];
            let defaults = [];
            let res = [null(), into_json(1)?];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [];
            let defaults = [];
            let res: [serde_json::Value; 0] = [];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        {
            let mut args = [into_json(0)?];
            let defaults = [into_json(2)?];
            let res = [into_json(0)?];
            assert_eq!(handle_defaults(&mut args, &defaults), &res);
        }
        Ok(())
    }

    #[test]
    fn test_handle_defaults() {
        test_handle_defaults_inner().unwrap();
    }
}
