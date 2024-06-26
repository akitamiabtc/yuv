use alloc::collections::BTreeMap;
use yuv_pixels::PixelProof;

/// Contains proofs for inputs or outputs of the YUV Transaction.
///
/// Maps inputs or outputs ids to [`PixelProof`]s.
pub type ProofMap = BTreeMap<u32, PixelProof>;

/// Contains proofs for inputs and outputs of the YUV Transaction.
#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TransferProofs {
    #[cfg_attr(feature = "serde", serde(default))]
    pub input: ProofMap,
    pub output: ProofMap,
}

/// Checks if any of the proofs is bulletproof.
#[cfg(feature = "bulletproof")]
pub fn is_bulletproof<'a>(proofs: impl IntoIterator<Item = &'a PixelProof>) -> bool {
    proofs
        .into_iter()
        .any(|pixel_proof| pixel_proof.is_bulletproof())
}
