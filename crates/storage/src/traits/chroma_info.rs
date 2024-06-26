use async_trait::async_trait;

use bitcoin::ScriptBuf;
use serde_bytes::ByteArray;
use yuv_pixels::{Chroma, CHROMA_SIZE};
use yuv_types::announcements::{ChromaAnnouncement, ChromaInfo};

use crate::{KeyValueResult, KeyValueStorage};

const KEY_PREFIX: &str = "chrm-";
const KEY_PREFIX_SIZE: usize = KEY_PREFIX.len();

const KEY_SIZE: usize = KEY_PREFIX.len() + CHROMA_SIZE;

fn get_storage_key(chroma: &Chroma) -> ByteArray<KEY_SIZE> {
    let mut bytes = [0u8; KEY_SIZE];

    bytes[..KEY_PREFIX_SIZE].copy_from_slice(KEY_PREFIX.as_bytes());
    bytes[KEY_PREFIX_SIZE..].copy_from_slice(&chroma.to_bytes());

    ByteArray::new(bytes)
}

/// It is a key-value storage for the [`ChromaAnnouncement`] and total supply.
///
/// - key: `b"chrm-"` + [`Chroma`]
/// - value: [`ChromaInfo`][`ChromaAnnouncement`]
#[async_trait]
pub trait ChromaInfoStorage: KeyValueStorage<ByteArray<KEY_SIZE>, ChromaInfo> {
    /// Get the [`ChromaAnnouncement`] for the given [`Chroma`].
    async fn get_chroma_info(&self, chroma: &Chroma) -> KeyValueResult<Option<ChromaInfo>> {
        self.get(get_storage_key(chroma)).await
    }

    /// Put the [`ChromaAnnouncement`] for the given [`Chroma`].
    async fn put_chroma_info(
        &self,
        chroma: &Chroma,
        announcement: Option<ChromaAnnouncement>,
        total_supply: u128,
        owner: Option<ScriptBuf>,
    ) -> KeyValueResult<()> {
        self.put(
            get_storage_key(chroma),
            ChromaInfo {
                announcement,
                total_supply,
                owner,
            },
        )
        .await
    }
}
