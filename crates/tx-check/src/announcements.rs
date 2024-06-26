use bitcoin::Txid;
use bitcoin_client::BitcoinRpcApi;
use yuv_storage::{ChromaInfoStorage, FrozenTxsStorage, InvalidTxsStorage, TransactionsStorage};
use yuv_types::announcements::{
    ChromaAnnouncement, FreezeAnnouncement, IssueAnnouncement, TransferOwnershipAnnouncement,
};

use crate::TxCheckerWorker;

impl<TS, SS, BC> TxCheckerWorker<TS, SS, BC>
where
    TS: TransactionsStorage + Clone + Send + Sync + 'static,
    SS: InvalidTxsStorage + FrozenTxsStorage + ChromaInfoStorage + Clone + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    /// Update chroma announcements in storage.
    pub(crate) async fn add_chroma_announcements(
        &self,
        announcement: &ChromaAnnouncement,
    ) -> eyre::Result<()> {
        let chroma_info = self
            .state_storage
            .get_chroma_info(&announcement.chroma)
            .await?;

        let (total_supply, owner) = if let Some(chroma_info) = chroma_info {
            if chroma_info.announcement.is_some() {
                tracing::debug!(
                    "Chroma announcement for Chroma {} already exist",
                    announcement.chroma
                );

                return Ok(());
            }

            (chroma_info.total_supply, chroma_info.owner)
        } else {
            (0, None)
        };

        self.state_storage
            .put_chroma_info(
                &announcement.chroma,
                Some(announcement.clone()),
                total_supply,
                owner,
            )
            .await?;

        tracing::debug!(
            "Chroma announcement for Chroma {} is added",
            announcement.chroma
        );

        Ok(())
    }

    /// For each freeze toggle, update entry in freeze state storage.
    pub(crate) async fn update_freezes(
        &self,
        txid: Txid,
        freeze: &FreezeAnnouncement,
    ) -> eyre::Result<()> {
        let freeze_outpoint = &freeze.freeze_outpoint();

        let mut freeze_entry = self
            .state_storage
            .get_frozen_tx(freeze_outpoint)
            .await?
            .unwrap_or_default();

        freeze_entry.tx_ids.push(txid);

        tracing::debug!(
            "Freeze toggle for txid={} vout={} is set to {:?}",
            freeze.freeze_txid(),
            freeze_outpoint,
            freeze_entry.tx_ids,
        );

        self.state_storage
            .put_frozen_tx(freeze_outpoint, freeze_entry.tx_ids)
            .await?;

        Ok(())
    }

    pub(crate) async fn update_supply(&self, issue: &IssueAnnouncement) -> eyre::Result<()> {
        if let Some(chroma_info) = self.state_storage.get_chroma_info(&issue.chroma).await? {
            self.state_storage
                .put_chroma_info(
                    &issue.chroma,
                    chroma_info.announcement,
                    chroma_info.total_supply + issue.amount,
                    chroma_info.owner,
                )
                .await?;

            return Ok(());
        }

        self.state_storage
            .put_chroma_info(&issue.chroma, None, issue.amount, None)
            .await?;

        tracing::debug!("Updated supply for chroma {}", issue.chroma);

        Ok(())
    }

    pub(crate) async fn update_owner(
        &self,
        transfer_ownership: &TransferOwnershipAnnouncement,
    ) -> eyre::Result<()> {
        let chroma_info_opt = self
            .state_storage
            .get_chroma_info(&transfer_ownership.chroma)
            .await?;

        let (announcement, total_supply) = chroma_info_opt.map_or((None, 0), |chroma_info| {
            (chroma_info.announcement, chroma_info.total_supply)
        });

        self.state_storage
            .put_chroma_info(
                &transfer_ownership.chroma,
                announcement,
                total_supply,
                Some(transfer_ownership.new_owner.clone()),
            )
            .await?;
        Ok(())
    }
}
