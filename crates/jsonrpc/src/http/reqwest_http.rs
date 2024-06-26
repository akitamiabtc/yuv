//! This module implements the [`crate::client::Transport`] trait using [`reqwest`]
//! as the underlying HTTP transport.
//!
//! [reqwest]: <https://github.com/seanmonstar/reqwest>

use std::str::FromStr;
use std::time::Duration;
use std::{error, fmt};

use async_trait::async_trait;
use reqwest::header::HeaderValue;
use reqwest::{Body, Method, Url};

use crate::client::Transport;
use crate::{Request, Response};

use super::{DEFAULT_PORT, DEFAULT_TIMEOUT_SECONDS, DEFAULT_URL};

/// An HTTP transport that uses [`reqwest`] and is useful for running a bitcoind RPC client.
#[derive(Clone, Debug)]
pub struct ReqwestHttpTransport {
    /// URL of the RPC server.
    url: String,
    /// timeout only supports second granularity.
    timeout: Duration,
    /// The value of the `Authorization` HTTP header, i.e., a base64 encoding of 'user:password'.
    auth: Option<String>,
}

impl Default for ReqwestHttpTransport {
    fn default() -> Self {
        ReqwestHttpTransport {
            url: format!("{}:{}", DEFAULT_URL, DEFAULT_PORT),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
            auth: None,
        }
    }
}

impl ReqwestHttpTransport {
    /// Constructs a new [`ReqwestHttpTransport`] with default parameters.
    pub fn new() -> Self {
        ReqwestHttpTransport::default()
    }

    async fn request<R>(&self, body: impl serde::Serialize) -> Result<R, Error>
    where
        R: serde::de::DeserializeOwned,
    {
        let mut request = self.form_request(body)?;
        if let Some(auth) = &self.auth {
            request.headers_mut().insert(
                "Authorization",
                HeaderValue::from_str(auth).expect("Auth header should be valid"),
            );
        }

        let response = reqwest::Client::new().execute(request).await?;
        Ok(serde_json::from_str(&response.text().await?)?)
    }

    fn form_request(&self, body: impl serde::Serialize) -> Result<reqwest::Request, Error> {
        let mut request = reqwest::Request::new(
            Method::POST,
            Url::from_str(&self.url).expect("URL should be valid"),
        );

        *request.timeout_mut() = Some(self.timeout);
        *request.body_mut() = Some(Body::from(serde_json::to_string(&body)?));

        Ok(request)
    }
}

#[async_trait]
impl Transport for ReqwestHttpTransport {
    async fn send_request(&self, req: Request<'_>) -> Result<Response, crate::Error> {
        Ok(self.request(req).await?)
    }

    async fn send_batch(&self, reqs: &[Request<'_>]) -> Result<Vec<Response>, crate::Error> {
        Ok(self.request(reqs).await?)
    }

    fn fmt_target(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.url)
    }
}

/// Builder for simple bitcoind [`ReqwestHttpTransport`].
#[derive(Clone, Debug)]
pub struct Builder {
    tp: ReqwestHttpTransport,
}

impl Builder {
    /// Constructs a new [`Builder`] with default configuration and the URL to use.
    pub fn new() -> Builder {
        Builder {
            tp: ReqwestHttpTransport::new(),
        }
    }

    /// Sets the timeout after which requests will abort if they aren't finished.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.tp.timeout = timeout;
        self
    }

    /// Sets the URL of the server to the transport.
    pub fn url(mut self, url: &str) -> Result<Self, Error> {
        self.tp.url = url.to_owned();
        Ok(self)
    }

    /// Adds authentication information to the transport.
    pub fn auth(mut self, user: String, pass: Option<String>) -> Self {
        let mut s = user;
        s.push(':');
        if let Some(ref pass) = pass {
            s.push_str(pass.as_ref());
        }
        self.tp.auth = Some(format!("Basic {}", &base64::encode(s.as_bytes())));
        self
    }

    /// Builds the final [`ReqwestHttpTransport`].
    pub fn build(self) -> ReqwestHttpTransport {
        self.tp
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder::new()
    }
}

#[derive(Debug)]
pub enum Error {
    /// JSON parsing error.
    Json(serde_json::Error),
    /// Reqwest error.
    Reqwest(reqwest::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Error::Json(ref e) => write!(f, "parsing JSON failed: {}", e),
            Error::Reqwest(ref e) => write!(f, "reqwest: {}", e),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::Json(ref e) => Some(e),
            Error::Reqwest(ref e) => Some(e),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Reqwest(e)
    }
}

impl From<Error> for crate::Error {
    fn from(e: Error) -> crate::Error {
        match e {
            Error::Json(e) => crate::Error::Json(e),
            e => crate::Error::Transport(Box::new(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Client;

    #[test]
    fn construct() {
        let tp = Builder::new()
            .timeout(Duration::from_millis(100))
            .url("http://localhost:22")
            .unwrap()
            .auth("user".to_string(), None)
            .build();
        let _ = Client::with_transport(tp);
    }
}
