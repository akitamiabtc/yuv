use crate::types::{Utxo, WeightedUtxo};
use bdk::Error;
use bitcoin::Script;
use yuv_pixels::Chroma;

/// Default coin selection algorithm used by transaction buileder if not
/// overridden
pub type DefaultCoinSelectionAlgorithm = YuvLargestFirstCoinSelection;

/// Result of a successful coin selection
#[derive(Debug)]
pub struct YUVCoinSelectionResult {
    /// List of outputs selected for use as inputs
    pub selected: Vec<Utxo>,
    /// Remaining amount after deducting fees and outgoing outputs
    pub amount: u128,
}

impl YUVCoinSelectionResult {
    /// The total value of the inputs selected.
    pub fn selected_amount(&self) -> u128 {
        self.selected
            .iter()
            .map(|u| u.yuv_txout().pixel.luma.amount)
            .sum()
    }

    /// The total value of the inputs selected from the local wallet.
    pub fn local_selected_amount(&self) -> u128 {
        self.selected
            .iter()
            .map(|u| u.yuv_txout().pixel.luma.amount)
            .sum()
    }
}

/// Trait for generalized coin selection algorithms
///
/// This trait can be implemented to make the [`Wallet`](crate::wallet::Wallet) use a customized coin
/// selection algorithm when it creates transactions.
pub trait YUVCoinSelectionAlgorithm: core::fmt::Debug {
    /// Perform the coin selection
    ///
    /// - `database`: a reference to the wallet's database that can be used to lookup additional
    ///               details for a specific UTXO
    /// - `required_utxos`: the utxos that must be spent regardless of `target_amount` with their
    ///                     weight cost
    /// - `optional_utxos`: the remaining available utxos to satisfy `target_amount` with their
    ///                     weight cost
    /// - `fee_rate`: fee rate to use
    /// - `target_amount`: the outgoing amount in satoshis and the fees already
    ///                    accumulated from added outputs and transaction’s header.
    /// - `drain_script`: the script to use in case of change
    fn coin_select(
        &self,
        required_utxos: Vec<WeightedUtxo>,
        optional_utxos: Vec<WeightedUtxo>,
        target_amount: u128,
        drain_script: &Script,
        target_token: Chroma,
    ) -> Result<YUVCoinSelectionResult, Error>;
}

/// Simple and dumb coin selection
///
/// This coin selection algorithm sorts the available UTXOs by value and then picks them starting
/// from the largest ones until the required amount is reached.
/// Simple and dumb coin selection
///
/// This coin selection algorithm sorts the available UTXOs by value and then picks them starting
/// from the largest ones until the required amount is reached.
#[derive(Debug, Default, Clone, Copy)]
pub struct YuvLargestFirstCoinSelection;

impl YUVCoinSelectionAlgorithm for YuvLargestFirstCoinSelection {
    fn coin_select(
        &self,
        required_utxos: Vec<WeightedUtxo>,
        mut optional_utxos: Vec<WeightedUtxo>,
        target_amount: u128,
        drain_script: &Script,
        target_chroma: Chroma,
    ) -> Result<YUVCoinSelectionResult, Error> {
        tracing::debug!("target_amount = `{}`", target_amount);

        // Filter UTXOs based on the target token.
        optional_utxos.retain(|wu| {
            wu.utxo.yuv_txout().pixel.chroma == target_chroma
                && !wu.utxo.yuv_txout().script_pubkey.is_op_return()
        });

        // We put the "required UTXOs" first and make sure the optional UTXOs are sorted,
        // initially smallest to largest, before being reversed with `.rev()`.
        let utxos = {
            optional_utxos.sort_unstable_by_key(|wu| wu.utxo.yuv_txout().pixel.luma.amount); // Sorting by amount now
            required_utxos
                .into_iter()
                .map(|utxo| (true, utxo))
                .chain(optional_utxos.into_iter().rev().map(|utxo| (false, utxo)))
        };

        select_sorted_utxos(utxos, target_amount, drain_script)
    }
}

/// OldestFirstCoinSelection always picks the utxo with the smallest blockheight to add to the selected coins next
///
/// This coin selection algorithm sorts the available UTXOs by blockheight and then picks them starting
/// from the oldest ones until the required amount is reached.
#[derive(Debug, Default, Clone, Copy)]
pub struct YUVOldestFirstCoinSelection;

impl YUVCoinSelectionAlgorithm for YUVOldestFirstCoinSelection {
    fn coin_select(
        &self,
        required_utxos: Vec<WeightedUtxo>,
        mut optional_utxos: Vec<WeightedUtxo>,
        target_amount: u128,
        drain_script: &Script,
        target_chroma: Chroma,
    ) -> Result<YUVCoinSelectionResult, Error> {
        // We put the "required UTXOs" first and make sure the optional UTXOs are sorted from
        // oldest to newest according to blocktime
        // For utxo that doesn't exist in DB, they will have lowest priority to be selected
        let utxos = {
            optional_utxos.retain(|wu| wu.utxo.yuv_txout().pixel.chroma == target_chroma);

            required_utxos
                .into_iter()
                .map(|utxo| (true, utxo))
                .chain(optional_utxos.into_iter().map(|utxo| (false, utxo)))
        };

        select_sorted_utxos(utxos, target_amount, drain_script)
    }
}

fn select_sorted_utxos(
    utxos: impl Iterator<Item = (bool, WeightedUtxo)>,
    target_amount: u128,
    _drain_script: &Script,
) -> Result<YUVCoinSelectionResult, Error> {
    let mut yuv_amount = 0;
    let selected = utxos
        .scan(&mut yuv_amount, |yuv_amount, (must_use, weighted_utxo)| {
            if must_use || **yuv_amount < target_amount {
                **yuv_amount += weighted_utxo.utxo.yuv_txout().pixel.luma.amount;
                Some(weighted_utxo.utxo)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(YUVCoinSelectionResult {
        selected,
        amount: yuv_amount,
    })
}

#[cfg(test)]
mod test {
    use bitcoin::{OutPoint, ScriptBuf};
    use core::str::FromStr;
    use yuv_pixels::{Luma, Pixel};

    use super::*;
    use crate::types::*;

    // n. of items on witness (1WU) + signature len (1WU) + signature and sighash (72WU)
    // + pubkey len (1WU) + pubkey (33WU) + script sig len (1 byte, 4WU)
    const P2WPKH_SATISFACTION_SIZE: usize = 1 + 1 + 72 + 1 + 33 + 4;

    const FEE_AMOUNT: u64 = 50;

    fn utxo(
        satoshis: u64,
        yuv_amount: u128,
        token: bitcoin::PublicKey,
        index: u32,
    ) -> WeightedUtxo {
        assert!(index < 10);
        let outpoint = OutPoint::from_str(&format!(
            "000000000000000000000000000000000000000000000000000000000000000{}:0",
            index
        ))
        .unwrap();
        WeightedUtxo {
            satisfaction_weight: P2WPKH_SATISFACTION_SIZE,
            utxo: Utxo::Yuv(YuvUtxo {
                outpoint,
                txout: YuvTxOut {
                    satoshis,
                    script_pubkey: ScriptBuf::new(),
                    pixel: Pixel {
                        luma: Luma::from(yuv_amount),
                        chroma: token.into(),
                    },
                },
                keychain: KeychainKind::External,
                is_spent: false,
                derivation_index: 42,
                confirmation_time: None,
            }),
        }
    }

    fn get_test_utxos() -> Vec<WeightedUtxo> {
        vec![
            utxo(
                100_000,
                500_000,
                bitcoin::PublicKey::from_str(
                    "02ba604e6ad9d3864eda8dc41c62668514ef7d5417d3b6db46e45cc4533bff001c",
                )
                .expect("pubkey"),
                0,
            ),
            utxo(
                FEE_AMOUNT - 40,
                40_000,
                bitcoin::PublicKey::from_str(
                    "02ba604e6ad9d3864eda8dc41c62668514ef7d5417d3b6db46e45cc4533bff001c",
                )
                .expect("pubkey"),
                1,
            ),
            utxo(
                200_000,
                250_000,
                bitcoin::PublicKey::from_str(
                    "02ba604e6ad9d3864eda8dc41c62668514ef7d5417d3b6db46e45cc4533bff001c",
                )
                .expect("pubkey"),
                2,
            ),
        ]
    }

    #[test]
    fn test_largest_first_coin_selection_success() {
        let utxos = get_test_utxos();
        let drain_script = ScriptBuf::default();
        let target_amount = 600_000;

        let result = YuvLargestFirstCoinSelection
            .coin_select(
                utxos,
                vec![],
                target_amount,
                &drain_script,
                Chroma::from_str(
                    "ba604e6ad9d3864eda8dc41c62668514ef7d5417d3b6db46e45cc4533bff001c",
                )
                .expect("pubkey"),
            )
            .unwrap();

        assert_eq!(result.selected.len(), 3);
        assert_eq!(result.selected_amount(), 790_000);
    }
}
