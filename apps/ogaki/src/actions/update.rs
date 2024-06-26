use std::path::PathBuf;
use std::{env, fs};

use clap::Args;
use eyre::{Context, ContextCompat, OptionExt};
use flate2::bufread::GzDecoder;
use semver::Version;
use tar::Archive;

use crate::constants;
use crate::errors::Error;
use crate::github::Asset;
use crate::github::GithubClient;
use crate::github::UpdateAvailable;
use crate::utils::{get_current_version, try_get_yuvd_path};

/// Configuration for the ogaki commands.
#[derive(Args, Debug, Clone)]
pub struct UpdateArgs {
    /// Github repository URL to pool for updates, defaults to the official yuvd repository.
    #[clap(short, long)]
    pub url: Option<String>,

    /// Optional Bearer token to GitHub API.
    #[clap(short, long, default_value_t = String::new())]
    pub token: String,
}

/// Check for updates and install them.
///
/// # Returns
///
/// `Ok(true)` if an update was installed, `Ok(false)` if no update was available and an error if an
/// error occurred.
pub async fn update(args: &UpdateArgs) -> Result<bool, Error> {
    let client = GithubClient::new(args.url.clone(), args.token.clone());

    let new_version_available = is_new_version_available(&client)
        .await
        .map_err(Error::Other)?;

    if !new_version_available.update_available {
        tracing::debug!(
            "No new version available, current version: {}",
            new_version_available.latest_release.tag_name
        );
        return Ok(false);
    }

    tracing::info!(
        "A new version is available: {}",
        new_version_available.latest_release.tag_name
    );

    let asset = find_compatible_asset(&new_version_available.latest_release.assets)
        .ok_or(Error::NoCompatibleAsset)?;

    let span = tracing::debug_span!("update", asset.name, asset.id);
    let _guard = span.enter();

    tracing::info!("Downloading asset");

    let bytes = client
        .download_asset(asset.id)
        .await
        .map_err(Error::Other)?
        .ok_or_eyre("No such asset or asset is no accesible")?;

    tracing::info!("Downloaded asset");

    unpack_tar_gz_from_bytes(&bytes)?;
    tracing::info!("Unpacked asset");

    Ok(true)
}

/// Find a compatible update asset for the current platform. Returns the first compatible asset
/// found or None if there is no compatible asset.
fn find_compatible_asset(assets: &[Asset]) -> Option<&Asset> {
    // TODO: replace searching with a more robust method
    assets.iter().find(|a| {
        a.name.contains(constants::OS)
            && a.name.contains(env::consts::ARCH)
            && a.name.ends_with(".tar.gz")
    })
}

/// Unpack update tar.gz archive from a byte slice to the yuvd binary directory.
fn unpack_tar_gz_from_bytes(bytes: &[u8]) -> Result<(), Error> {
    let destination = try_get_yuvd_path()?
        .parent()
        .wrap_err("YUVd binary path has no parent directory")?
        .to_path_buf();

    unpack_tar_gz_into_dest(bytes, destination).map_err(Error::Other)
}

fn unpack_tar_gz_into_dest(bytes: &[u8], destination: PathBuf) -> eyre::Result<()> {
    let decompressed = GzDecoder::new(bytes);
    let mut archive = Archive::new(decompressed);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        let dest_path = destination.join(
            path.file_name()
                .ok_or_eyre("Invalid path inside of the compressed asset")?,
        );

        if dest_path.exists() {
            fs::remove_file(&dest_path)?;
        }

        entry.unpack(dest_path)?;
    }

    Ok(())
}

/// Check for updates. Prints a message to the user if a new version is available or not. Returns
/// an error if an error occurred while checking for updates.
pub async fn check_updates(args: &UpdateArgs) -> Result<(), Error> {
    let client = GithubClient::new(args.url.clone(), args.token.clone());

    let is_new_version_available = is_new_version_available(&client)
        .await
        .map_err(Error::Other)?;

    if is_new_version_available.update_available {
        println!(
            "A new version is available {}",
            is_new_version_available.latest_release.tag_name
        );

        return Ok(());
    }

    println!("Ogaki has nothing new for you.",);

    Ok(())
}

/// Check if a new version is available. Returns [`UpdateAvailable`] with the result.
async fn is_new_version_available(client: &GithubClient) -> eyre::Result<UpdateAvailable> {
    let current_version = get_current_version()?;
    let latest_release = client
        .get_latest_release()
        .await?
        .ok_or_eyre("No latest release found in repo")?;

    // Skip first byte as semver tag usually starts with `v` char.
    let latest_version = Version::parse(&latest_release.tag_name[1..])
        .wrap_err("Invalid version got from release tag")?;

    Ok(UpdateAvailable {
        update_available: latest_version > current_version,
        latest_release,
    })
}
