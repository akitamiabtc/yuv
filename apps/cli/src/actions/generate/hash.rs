use bitcoin::PublicKey;
use clap::Args;
use color_eyre::eyre;
use yuv_pixels::{Pixel, PixelHash};

#[derive(Args, Debug)]
/// Generate YUV pixel hash from amount and chroma.
pub struct GeneratePixelHashArgs {
    /// Amount of tokens to send.
    #[clap(long)]
    pub amount: u128,
    /// Chroma of the pixel.
    #[clap(long)]
    pub chroma: PublicKey,
}

pub(crate) fn run(
    GeneratePixelHashArgs { amount, chroma }: GeneratePixelHashArgs,
) -> eyre::Result<()> {
    let pixel = Pixel::new(amount, chroma);

    let pixel_hash = PixelHash::from(pixel);

    println!("Pixel hash: {}", *pixel_hash);

    Ok(())
}
