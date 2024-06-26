use std::collections::HashMap;

use bitcoin::OutPoint;
use jsonrpsee::core::async_trait;
use yuv_pixels::PixelProof;
use yuv_storage::KeyValueStorage;

const UNSPENT_YUV_OUTPOINTS_KEY: &[u8; 15] = b"unspent_yuv_txs";
const UNSPENT_YUV_OUTPOINTS_KEY_LEN: usize = UNSPENT_YUV_OUTPOINTS_KEY.len();

#[async_trait]
pub trait UnspentYuvOutPointsStorage:
    KeyValueStorage<&'static [u8; UNSPENT_YUV_OUTPOINTS_KEY_LEN], HashMap<OutPoint, PixelProof>>
{
    async fn get_unspent_yuv_outpoints(&self) -> eyre::Result<HashMap<OutPoint, PixelProof>> {
        let entry = self
            .get(UNSPENT_YUV_OUTPOINTS_KEY)
            .await?
            .unwrap_or_default();

        Ok(entry)
    }

    async fn put_unspent_yuv_outpoints(
        &self,
        unspent_yuv_outpoints: HashMap<OutPoint, PixelProof>,
    ) -> eyre::Result<()> {
        self.put(UNSPENT_YUV_OUTPOINTS_KEY, unspent_yuv_outpoints)
            .await?;

        Ok(())
    }
}

impl<T> UnspentYuvOutPointsStorage for T where
    T: KeyValueStorage<&'static [u8; UNSPENT_YUV_OUTPOINTS_KEY_LEN], HashMap<OutPoint, PixelProof>>
{
}
