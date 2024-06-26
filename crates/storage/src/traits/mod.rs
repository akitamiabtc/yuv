use std::io;

use async_trait::async_trait;

mod transactions;
use serde::{de::DeserializeOwned, Serialize};
pub use transactions::TransactionsStorage;

mod invalid;
pub use invalid::InvalidTxsStorage;

mod inventory;
pub use inventory::InventoryStorage;

pub(crate) mod pages;
pub use pages::PagesNumberStorage;
pub use pages::PagesStorage;

mod indexed_block;
pub use indexed_block::BlockIndexerStorage;
pub use indexed_block::IsIndexedStorage;

mod frozen;
pub use frozen::FrozenTxsStorage;

mod chroma_info;
pub use chroma_info::ChromaInfoStorage;

pub type KeyValueResult<T> = Result<T, KeyValueError>;

#[async_trait]
pub trait KeyValueStorage<K, V>
where
    K: Serialize + Send + Sync + 'static,
    V: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    type Error: std::error::Error + 'static + Send + Sync;

    async fn raw_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), Self::Error>;
    async fn raw_get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error>;
    async fn raw_delete(&self, key: Vec<u8>) -> Result<(), Self::Error>;

    async fn flush(&self) -> Result<(), Self::Error>;

    async fn put(&self, key: K, value: V) -> KeyValueResult<()> {
        let key = cbor_to_vec(key)?;
        let value = cbor_to_vec(value)?;

        self.raw_put(key, value)
            .await
            .map_err(|err| KeyValueError::Storage(Box::new(err)))
    }

    async fn get(&self, key: K) -> KeyValueResult<Option<V>> {
        let key: Vec<u8> = cbor_to_vec(key)?;

        let result = self
            .raw_get(key)
            .await
            .map_err(|err| KeyValueError::Storage(Box::new(err)))?;

        let Some(value) = result else {
            return Ok(None);
        };

        let value: V = cbor_from_vec(value)?;

        Ok(Some(value))
    }

    async fn delete(&self, key: K) -> KeyValueResult<()> {
        let key: Vec<u8> = cbor_to_vec(key)?;

        self.raw_delete(key)
            .await
            .map_err(|err| KeyValueError::Storage(Box::new(err)))
    }
}

fn cbor_to_vec<K: Serialize>(key: K) -> Result<Vec<u8>, ciborium::ser::Error<io::Error>> {
    let mut buf = Vec::new();
    ciborium::into_writer(&key, &mut buf)?;
    Ok(buf)
}

fn cbor_from_vec<T: DeserializeOwned>(data: Vec<u8>) -> Result<T, ciborium::de::Error<io::Error>> {
    ciborium::from_reader(data.as_slice())
}

#[derive(Debug, thiserror::Error)]
pub enum KeyValueError {
    #[error("Decoding error: {0}")]
    Decoding(ciborium::de::Error<io::Error>),
    #[error("Encoding error: {0}")]
    Encoding(ciborium::ser::Error<io::Error>),
    #[error("Storage error: {0}")]
    Storage(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<ciborium::de::Error<io::Error>> for KeyValueError {
    fn from(err: ciborium::de::Error<io::Error>) -> Self {
        KeyValueError::Decoding(err)
    }
}

impl From<ciborium::ser::Error<io::Error>> for KeyValueError {
    fn from(err: ciborium::ser::Error<io::Error>) -> Self {
        KeyValueError::Encoding(err)
    }
}
