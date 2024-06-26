#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("No compatible asset found")]
    NoCompatibleAsset,

    #[error("YUVd binary not found")]
    YuvdNotFound,

    #[error("{0}")]
    Other(#[from] eyre::Report),
}
