use crate::errors::Error;

use eyre::{Context, OptionExt};
use semver::Version;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};
use which::which;

pub const DEFAULT_SEARCH_PATHS: [&str; 2] = ["./yuvd", "yuvd"];

/// Get the absolute path to the yuvd binary. Checks CWD, PATH for the binary. Returns an error if
/// the binary is not found.
pub fn get_yuvd_path() -> Option<PathBuf> {
    let [local, global] = DEFAULT_SEARCH_PATHS;

    which(local).or_else(|_| which(global)).ok()
}

pub fn try_get_yuvd_path() -> Result<PathBuf, Error> {
    get_yuvd_path().ok_or(Error::YuvdNotFound)
}

/// Get the current semver version of the yuvd binary. Returns an error if the version cannot be
/// parsed or if the yuvd binary is not found.
pub fn get_current_version() -> Result<Version, Error> {
    let yuvd = try_get_yuvd_path()?;

    let output = Command::new(yuvd)
        .arg("--version")
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err("Failed to spawn YUVd process")?
        .wait_with_output()
        .wrap_err("Failed to get version from YUVd process")?;

    let output_str =
        String::from_utf8(output.stdout).wrap_err("Failed to parse YUVd process output")?;

    let version_str = output_str
        .trim()
        .split(' ')
        .last()
        .ok_or_eyre("Invalid version")?;

    let version =
        Version::parse(version_str).wrap_err("Invalid semver version from YUVd binary")?;

    Ok(version)
}
