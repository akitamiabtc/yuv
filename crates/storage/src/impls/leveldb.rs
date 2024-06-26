use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use rusty_leveldb::AsyncDB;
use serde::{Deserialize, Serialize};

use crate::traits::pages::PagesNumberStorage;
use crate::traits::{ChromaInfoStorage, IsIndexedStorage, PagesStorage};

use crate::{
    traits::{FrozenTxsStorage, InvalidTxsStorage, InventoryStorage, TransactionsStorage},
    BlockIndexerStorage, KeyValueStorage,
};

pub const DEFAULT_FLUSH_PERIOD_SECS: u64 = 600;

pub struct Options {
    pub path: PathBuf,
    pub create_if_missing: bool,
    pub flush_strategy: FlushStrategy,
}

pub enum FlushStrategy {
    Ticker { period: u64 },
    Disabled,
}

#[derive(Clone)]
pub struct LevelDB(rusty_leveldb::AsyncDB);

impl LevelDB {
    fn new(db: rusty_leveldb::AsyncDB, flush_strategy: FlushStrategy) -> Self {
        let db = Self(db);

        if let FlushStrategy::Ticker {
            period: flush_period,
        } = flush_strategy
        {
            db.clone().flush_ticker(flush_period);
        }

        db
    }

    fn flush_ticker(self, flush_period: u64) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(flush_period)).await;
                if (self.0.flush().await).is_ok() {}
            }
        });
    }

    pub fn from_opts(config: Options) -> eyre::Result<Self> {
        let opt = rusty_leveldb::Options {
            create_if_missing: config.create_if_missing,
            ..Default::default()
        };

        let db = AsyncDB::new(config.path, opt)?;
        Ok(Self::new(db, config.flush_strategy))
    }

    pub fn in_memory() -> eyre::Result<Self> {
        let opt = rusty_leveldb::in_memory();

        let db = AsyncDB::new("yuv-db", opt)?;

        Ok(Self::new(db, FlushStrategy::Disabled))
    }
}

#[async_trait]
impl<K, V> KeyValueStorage<K, V> for LevelDB
where
    K: Serialize + Send + Sync + 'static,
    V: Serialize + for<'a> Deserialize<'a> + Send + Sync + 'static,
{
    type Error = rusty_leveldb::Status;

    async fn raw_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), Self::Error> {
        self.0.put(key, value).await
    }

    async fn raw_get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error> {
        self.0.get(key).await
    }

    async fn raw_delete(&self, key: Vec<u8>) -> Result<(), Self::Error> {
        self.0.delete(key).await
    }

    async fn flush(&self) -> Result<(), Self::Error> {
        self.0.flush().await
    }
}

impl TransactionsStorage for LevelDB {}

impl InvalidTxsStorage for LevelDB {}

impl InventoryStorage for LevelDB {}

impl PagesNumberStorage for LevelDB {}

impl PagesStorage for LevelDB {}

impl BlockIndexerStorage for LevelDB {}

impl FrozenTxsStorage for LevelDB {}

impl ChromaInfoStorage for LevelDB {}

impl IsIndexedStorage for LevelDB {}
