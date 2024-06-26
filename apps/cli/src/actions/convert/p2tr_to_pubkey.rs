use clap::Args;
use color_eyre::eyre;
use yuv_pixels::Chroma;

#[derive(Args, Debug)]
pub struct P2trToPubkeyArgs {
    /// A Taproot address to be converted to a public key.
    #[clap(long, short, value_parser = Chroma::from_address)]
    pub address: Chroma,
}

pub(crate) fn run(P2trToPubkeyArgs { address }: P2trToPubkeyArgs) -> eyre::Result<()> {
    println!("{}", address.public_key());

    Ok(())
}
