//! This module provides a [`BitcoinBlockIndexer`] which indexes blocks from Bitcoin.
#![doc = include_str!("../README.md")]

mod params;
pub use params::{IndexingParams, RunParams};

mod indexer;
pub use indexer::BitcoinBlockIndexer;

mod subindexer;
pub use subindexer::{AnnouncementsIndexer, ConfirmationIndexer, Subindexer};

mod blockloader;
pub use blockloader::{BlockLoader, BlockLoaderConfig};
