use bitcoin::Txid;
use yuv_storage::{ChromaInfoStorage, FrozenTxsStorage, InvalidTxsStorage, TransactionsStorage};
use yuv_types::announcements::{
    ChromaAnnouncement, FreezeAnnouncement, IssueAnnouncement, TransferOwnershipAnnouncement,
};

use crate::TxChecker;

impl<TS, SS> TxChecker<TS, SS>
where
    TS: TransactionsStorage + Clone + Send + Sync + 'static,
    SS: InvalidTxsStorage + FrozenTxsStorage + ChromaInfoStorage + Clone + Send + Sync + 'static,
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

    /// Set freeze entry for the given outpoint in the freeze storage.
    pub(crate) async fn update_freezes(
        &self,
        txid: Txid,
        freeze: &FreezeAnnouncement,
    ) -> eyre::Result<()> {
        let freeze_outpoint = &freeze.freeze_outpoint();
        let freeze_entry = self.state_storage.get_frozen_tx(freeze_outpoint).await?;
        if let Some(freeze_entry) = freeze_entry {
            tracing::debug!(
                txid = freeze.freeze_txid().to_string(),
                vout = freeze.freeze_vout(),
                "Outpoint was previously frozen in tx {:?}",
                freeze_entry.txid
            );

            return Ok(());
        }

        self.state_storage
            .put_frozen_tx(freeze_outpoint, txid, freeze.chroma)
            .await?;

        tracing::debug!(
            txid = freeze.freeze_txid().to_string(),
            vout = freeze.freeze_vout(),
            "The outpoint is frozen",
        );

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
