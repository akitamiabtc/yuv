use std::mem::size_of;

use crate::{KeyValueResult, KeyValueStorage};
use async_trait::async_trait;
use bitcoin::Txid;

const PAGES_NUMBER_KEY_SIZE: usize = 12;
/// The key for the [`KeyValueStorage`] storage where the YUV Node's pages number are stored.
const PAGES_NUMBER_KEY: &[u8; PAGES_NUMBER_KEY_SIZE] = b"pages-number";

/// The prefix that is used with the page number to store the page in the
/// [`KeyValueStorage`]. "page-1", "page-2", etc.
const PAGES_PREFIX: &str = "page-";
const PAGES_PREFIX_SIZE: usize = PAGES_PREFIX.len();

/// Page key size is 5(`PAGES_PREFIX:[u8; 5]`) + 8(`page number:u64`) = 13 bytes long
const PAGE_KEY_SIZE: usize = PAGES_PREFIX_SIZE + size_of::<u64>();

#[async_trait]
pub trait PagesNumberStorage: KeyValueStorage<[u8; PAGES_NUMBER_KEY_SIZE], u64> {
    async fn put_pages_number(&self, pages_number: u64) -> KeyValueResult<()> {
        self.put(*PAGES_NUMBER_KEY, pages_number).await
    }

    async fn get_pages_number(&self) -> KeyValueResult<Option<u64>> {
        Ok(self.get(*PAGES_NUMBER_KEY).await?)
    }
}

fn page_key(page_num: u64) -> [u8; PAGE_KEY_SIZE] {
    let mut bytes = [0u8; PAGE_KEY_SIZE];

    bytes[..PAGES_PREFIX_SIZE].copy_from_slice(PAGES_PREFIX.as_bytes());
    bytes[PAGES_PREFIX_SIZE..].copy_from_slice(&page_num.to_be_bytes());

    bytes
}

#[async_trait]
pub trait PagesStorage:
    KeyValueStorage<[u8; PAGE_KEY_SIZE], Vec<Txid>> + PagesNumberStorage
{
    async fn put_page(&self, page_num: u64, page: Vec<Txid>) -> KeyValueResult<()> {
        self.put(page_key(page_num), page).await
    }

    async fn get_page_by_num(&self, num: u64) -> KeyValueResult<Option<Vec<Txid>>> {
        Ok(self.get(page_key(num)).await?)
    }
}
