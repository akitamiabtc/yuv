use std::collections::BTreeMap;

use bitcoin::PublicKey;
use clap::Args;
use color_eyre::eyre::{self, bail};
use yuv_pixels::{Chroma, Pixel, PixelProof};
use yuv_types::TransferProofs;

#[derive(Debug, Args)]
pub struct ProofListArgs {
    /// Chroma of the pixel.
    #[clap(long)]
    #[arg(value_parser = Chroma::from_address)]
    pub chroma: Chroma,

    /// Number of the input in transaction.
    #[clap(long, num_args = 0..)]
    pub vin: Vec<u32>,

    /// Number of the output in transaction.
    #[clap(long, num_args = 1..)]
    pub vout: Vec<u32>,

    /// Recipient public key
    #[clap(long, num_args = 1..)]
    pub inner_key: Vec<PublicKey>,

    /// Amount of the token
    #[clap(long, num_args = 1..)]
    pub amount: Vec<u128>,
}

impl ProofListArgs {
    pub(crate) fn into_proof_maps(self) -> eyre::Result<TransferProofs> {
        let inputs_number = self.vin.len();
        let outputs_number = self.vout.len();

        let sum = inputs_number + outputs_number;
        if sum != self.inner_key.len() || sum != self.amount.len() {
            bail!("Number of inputs and outputs should be equal to number of keys and amounts");
        }

        // Take first N recipients and amount for inputs, where N is number of inputs.
        let inputs = self
            .inner_key
            .iter()
            .zip(self.amount.iter())
            .take(inputs_number)
            .zip(self.vin);
        // Take next M recipients and amount for outputs, where M is number of outputs.
        let outputs = self
            .inner_key
            .iter()
            .zip(self.amount.iter())
            .skip(inputs_number)
            .take(outputs_number)
            .zip(self.vout);

        // Convert inputs and outputs into [`PixelProof`]s
        let inputs = inputs
            .map(|((recipient, amount), vin)| {
                let pixel = Pixel::new(*amount, self.chroma);

                (vin, PixelProof::sig(pixel, recipient.inner))
            })
            .collect::<BTreeMap<_, _>>();

        let outputs = outputs
            .map(|((recipient, amount), vout)| {
                let pixel = Pixel::new(*amount, self.chroma);

                (vout, PixelProof::sig(pixel, recipient.inner))
            })
            .collect::<BTreeMap<_, _>>();

        Ok(TransferProofs {
            input: inputs,
            output: outputs,
        })
    }
}
