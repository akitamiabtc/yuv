use crate::{KeyValueResult, KeyValueStorage};
use async_trait::async_trait;
use bitcoin::Txid;

const INVENTORY_KEY_SIZE: usize = 9;
/// Key for the [`KeyValueStorage`] where the YUV Node's inventory is stored.
const INVENTORY_KEY: &[u8; INVENTORY_KEY_SIZE] = b"inventory";

#[async_trait]
pub trait InventoryStorage: KeyValueStorage<[u8; INVENTORY_KEY_SIZE], Vec<Txid>> {
    async fn get_inventory(&self) -> KeyValueResult<Vec<Txid>> {
        self.get(*INVENTORY_KEY)
            .await
            .map(|res| res.unwrap_or_default())
    }

    async fn put_inventory(&self, tx: Vec<Txid>) -> KeyValueResult<()> {
        self.put(*INVENTORY_KEY, tx).await
    }
}
