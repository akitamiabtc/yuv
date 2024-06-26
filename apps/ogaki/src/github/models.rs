use serde::Deserialize;

/// Represents a release from a Github repository. Learn more at:
/// <https://developer.github.com/v3/repos/releases/#get-the-latest-release>.
#[derive(Deserialize, Debug)]
pub struct Release {
    /// The tag name of the release, expected to be semver (e.g. v0.1.0).
    pub tag_name: String,
    /// The assets of the release.
    pub assets: Vec<Asset>,
}

/// Represents an asset from a Github release.
#[derive(Deserialize, Debug)]
pub struct Asset {
    /// Unique identifier of the asset.
    pub id: u64,
    /// Name of the asset file (e.g. yuv-v0.1.0-x86_64-unknown-linux-gnu.tar.gz).
    pub name: String,
}

/// The result of checking for updates.
pub struct UpdateAvailable {
    /// Indicates if an update is available.
    pub update_available: bool,
    /// The latest release.
    pub latest_release: Release,
}
