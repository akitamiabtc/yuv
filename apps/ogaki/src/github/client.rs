use bytes::Bytes;
use eyre::bail;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};

use crate::github::Release;

/*
 * Headers for the Github API.
 */

/// User agent in Github API must be set by the spec.
const OGAKI_USER_AGENT: &str = concat!("/ogaki:", env!("CARGO_PKG_VERSION"), "/");
/// Accept header for the Github API JSON values from the examples.
const GITHUB_JSON_ACCEPT_HEADER: &str = "application/vnd.github+json";

/// Accept header for the Github API octet streams.
const GITHUB_STREAM_ACCEPT_HEADER: &str = "application/octet-stream";

/// All static headers with values for the Github API client.
const CLIENT_HEADERS: &[(HeaderName, &str)] = &[
    (ACCEPT, GITHUB_JSON_ACCEPT_HEADER),
    (USER_AGENT, OGAKI_USER_AGENT),
];

const ENV_CARGO_PKG_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Custom client to Github API used specifially for ogaki.
pub(crate) struct GithubClient {
    inner: reqwest::Client,

    url: String,

    header_map: HeaderMap,
}

impl GithubClient {
    pub fn new(url_opt: Option<String>, token: String) -> Self {
        let inner = reqwest::Client::new();

        let url = url_opt.unwrap_or_else(|| ENV_CARGO_PKG_REPOSITORY.to_string());

        // Repalce the repository URL with the API URL.
        let api_url = url
            .replace("github.com", "api.github.com/repos")
            .replace("//", "/")
            .replace(":/", "://");

        let mut header_map = HeaderMap::new();

        for (name, value) in CLIENT_HEADERS {
            header_map.append(name, HeaderValue::from_static(value));
        }

        if !token.is_empty() {
            header_map.append(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
            );
        }

        Self {
            inner,
            url: api_url,
            header_map,
        }
    }

    /// Download the binary asset from GitHub url into memory and return it as a
    /// byte array.
    ///
    /// # Errors
    ///
    /// Returns in case of bad connection or if the asset is not found or not
    /// accessible by the user.
    ///
    /// # Note
    ///
    /// In future could be changed into async reader for large files.
    #[tracing::instrument(skip(self))]
    pub async fn download_asset(&self, asset_id: u64) -> eyre::Result<Option<Bytes>> {
        let mut header_map = self.header_map.clone();
        // Replace the accept header to octet stream as the client downloads a
        // binary.
        header_map.insert(
            ACCEPT,
            HeaderValue::from_static(GITHUB_STREAM_ACCEPT_HEADER),
        );

        let response = self
            .inner
            .get(format!("{}/releases/assets/{}", self.url, asset_id))
            .headers(header_map)
            .send()
            .await?;

        // On 404 or 401 return None as client can't access the asset.
        if response.status().is_client_error() {
            return Ok(None);
        }
        // On other statuses return the error.
        if !response.status().is_success() {
            bail!("Failed to download asset");
        }

        let response_bytes = response.bytes().await?;

        Ok(Some(response_bytes))
    }

    /// Get the latest release from a Github repository. Uses the repository URL
    /// from the config if present, otherwise uses the official yuvd repository.
    /// Returns the latest release or an error if the request fails.
    pub async fn get_latest_release(&self) -> eyre::Result<Option<Release>> {
        self.get("releases/latest").await
    }

    async fn get<T>(&self, path: impl AsRef<str>) -> eyre::Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}/{}", self.url, path.as_ref());

        let response = self
            .inner
            .get(&url)
            .headers(self.header_map.clone())
            .send()
            .await?;

        // On 404 or 401 return None as we can't access the asset.
        if response.status().is_client_error() {
            return Ok(None);
        }
        // On other statuses return the error.
        if !response.status().is_success() {
            bail!("Server returned an error while fetching the {url}");
        }

        let response_text = &response.text().await?;

        tracing::trace!(text = response_text, "Received response");

        Ok(Some(serde_json::from_str(response_text)?))
    }
}
