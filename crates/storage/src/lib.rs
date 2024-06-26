#![doc = include_str!("../README.md")]
mod traits;
pub use traits::KeyValueError;
pub use traits::{
    BlockIndexerStorage, ChromaInfoStorage, FrozenTxsStorage, InvalidTxsStorage, InventoryStorage,
    IsIndexedStorage, KeyValueResult, KeyValueStorage, PagesNumberStorage, PagesStorage,
    TransactionsStorage,
};

mod txstates;
pub use txstates::{TxState, TxStatesStorage};

mod impls;
#[cfg(feature = "leveldb")]
pub use impls::leveldb::{
    FlushStrategy, LevelDB, Options as LevelDbOptions, DEFAULT_FLUSH_PERIOD_SECS,
};
