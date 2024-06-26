use bitcoin::{Address, Network, PublicKey};
use clap::Args;
use color_eyre::eyre;
use yuv_pixels::{Pixel, PixelKey};

#[derive(Args, Debug)]
/// Generate P2WPKH YUV address from public key, amount and chroma.
pub struct GenerateAddressArgs {
    /// Public key in hex format.
    #[clap(long)]
    pub pubkey: PublicKey,
    /// Amount of tokens to send.
    #[clap(long)]
    pub amount: u128,
    /// Chroma of the pixel.
    #[clap(long)]
    pub chroma: PublicKey,
    /// Network to use.
    #[clap(long, short, default_value = "regtest")]
    pub network: Network,
}

pub(crate) fn run(
    GenerateAddressArgs {
        pubkey,
        amount,
        chroma,
        network,
    }: GenerateAddressArgs,
) -> eyre::Result<()> {
    let pixel = Pixel::new(amount, chroma);

    let pixel_key = PixelKey::new(pixel, &pubkey.inner)?;

    let address = Address::p2wpkh(&pixel_key, network)?;

    println!("Address: {}", address);

    Ok(())
}
