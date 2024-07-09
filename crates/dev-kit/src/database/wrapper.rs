//! This module provides [`Send]`, [`Sync]` wrapper [`bdk::database::BatchDatabase`].

use std::sync::{Arc, RwLock};

use bdk::database::{BatchDatabase, BatchOperations, Database};
use bdk::KeychainKind;
use bitcoin::{OutPoint, Script, ScriptBuf};

/// Clone, Send, Sync wrapper around [`MemoryDatabase`].
///
/// [`MemoryDatabase`]: bdk::database::MemoryDatabase
pub struct DatabaseWrapper<DB>(Arc<RwLock<DB>>);

impl<DB> Clone for DatabaseWrapper<DB>
where
    DB: bdk::database::BatchDatabase,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// NOTE(Velnbur): We assume here that database is created once by us, and then,
/// even if implementation is using [`RefCell`] inside it, [`Arc`] and
/// [`RwLock`] should prevent any unsafe operations.
///
/// [`RefCell`]: std::cell::RefCell
unsafe impl<DB> Sync for DatabaseWrapper<DB> where DB: bdk::database::BatchDatabase {}
unsafe impl<DB> Send for DatabaseWrapper<DB> where DB: bdk::database::BatchDatabase {}

impl<DB> DatabaseWrapper<DB>
where
    DB: bdk::database::BatchDatabase,
{
    pub fn new(db: DB) -> Self {
        Self(Arc::new(RwLock::new(db)))
    }
}

impl<DB> BatchOperations for DatabaseWrapper<DB>
where
    DB: bdk::database::BatchOperations,
{
    fn set_script_pubkey(
        &mut self,
        script: &Script,
        keychain: KeychainKind,
        child: u32,
    ) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.set_script_pubkey(script, keychain, child)
    }

    fn set_utxo(&mut self, utxo: &bdk::LocalUtxo) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.set_utxo(utxo)
    }

    fn set_raw_tx(&mut self, transaction: &bitcoin::Transaction) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.set_raw_tx(transaction)
    }

    fn set_tx(&mut self, transaction: &bdk::TransactionDetails) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.set_tx(transaction)
    }

    fn set_last_index(&mut self, keychain: KeychainKind, value: u32) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.set_last_index(keychain, value)
    }

    fn set_sync_time(&mut self, sync_time: bdk::database::SyncTime) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.set_sync_time(sync_time)
    }

    fn del_script_pubkey_from_path(
        &mut self,
        keychain: KeychainKind,
        child: u32,
    ) -> Result<Option<ScriptBuf>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_script_pubkey_from_path(keychain, child)
    }

    fn del_path_from_script_pubkey(
        &mut self,
        script: &Script,
    ) -> Result<Option<(KeychainKind, u32)>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_path_from_script_pubkey(script)
    }

    fn del_utxo(&mut self, outpoint: &OutPoint) -> Result<Option<bdk::LocalUtxo>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_utxo(outpoint)
    }

    fn del_raw_tx(
        &mut self,
        txid: &bitcoin::Txid,
    ) -> Result<Option<bitcoin::Transaction>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_raw_tx(txid)
    }

    fn del_tx(
        &mut self,
        txid: &bitcoin::Txid,
        include_raw: bool,
    ) -> Result<Option<bdk::TransactionDetails>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_tx(txid, include_raw)
    }

    fn del_last_index(&mut self, keychain: KeychainKind) -> Result<Option<u32>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_last_index(keychain)
    }

    fn del_sync_time(&mut self) -> Result<Option<bdk::database::SyncTime>, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.del_sync_time()
    }
}

impl<DB> Database for DatabaseWrapper<DB>
where
    DB: bdk::database::Database,
{
    fn check_descriptor_checksum<B: AsRef<[u8]>>(
        &mut self,
        keychain: KeychainKind,
        bytes: B,
    ) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.check_descriptor_checksum(keychain, bytes)
    }

    fn iter_script_pubkeys(
        &self,
        keychain: Option<KeychainKind>,
    ) -> Result<Vec<ScriptBuf>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.iter_script_pubkeys(keychain)
    }

    fn iter_utxos(&self) -> Result<Vec<bdk::LocalUtxo>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.iter_utxos()
    }

    fn iter_raw_txs(&self) -> Result<Vec<bitcoin::Transaction>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.iter_raw_txs()
    }

    fn iter_txs(&self, include_raw: bool) -> Result<Vec<bdk::TransactionDetails>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.iter_txs(include_raw)
    }

    fn get_script_pubkey_from_path(
        &self,
        keychain: KeychainKind,
        child: u32,
    ) -> Result<Option<ScriptBuf>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_script_pubkey_from_path(keychain, child)
    }

    fn get_path_from_script_pubkey(
        &self,
        script: &Script,
    ) -> Result<Option<(KeychainKind, u32)>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_path_from_script_pubkey(script)
    }

    fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<bdk::LocalUtxo>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_utxo(outpoint)
    }

    fn get_raw_tx(&self, txid: &bitcoin::Txid) -> Result<Option<bitcoin::Transaction>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_raw_tx(txid)
    }

    fn get_tx(
        &self,
        txid: &bitcoin::Txid,
        include_raw: bool,
    ) -> Result<Option<bdk::TransactionDetails>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_tx(txid, include_raw)
    }

    fn get_last_index(&self, keychain: KeychainKind) -> Result<Option<u32>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_last_index(keychain)
    }

    fn get_sync_time(&self) -> Result<Option<bdk::database::SyncTime>, bdk::Error> {
        let db = self.0.read().unwrap();

        db.get_sync_time()
    }

    fn increment_last_index(&mut self, keychain: KeychainKind) -> Result<u32, bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.increment_last_index(keychain)
    }
}

impl<DB> BatchDatabase for DatabaseWrapper<DB>
where
    DB: bdk::database::BatchDatabase,
{
    type Batch = DB::Batch;

    fn begin_batch(&self) -> Self::Batch {
        let db = self.0.read().unwrap();

        db.begin_batch()
    }

    fn commit_batch(&mut self, batch: Self::Batch) -> Result<(), bdk::Error> {
        let mut db = self.0.write().unwrap();

        db.commit_batch(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check that [`DatabaseWrapper`] is [`Send`], [`Sync`] and [`Clone`] for
    /// [`MemoryDatabase`] and [`SqliteDatabase`].
    #[test]
    fn test_send_sync_clone() {
        use bdk::database::{MemoryDatabase, SqliteDatabase};

        fn assert_send_sync_clone<T: Send + Sync + Clone>() {}

        assert_send_sync_clone::<DatabaseWrapper<MemoryDatabase>>();
        assert_send_sync_clone::<DatabaseWrapper<SqliteDatabase>>();
    }
}
