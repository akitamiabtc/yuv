use color_eyre::eyre;
use yuv_pixels::Chroma;

use crate::context::Context;

pub(crate) fn run(mut context: Context) -> eyre::Result<()> {
    let config = context.config()?;
    let ctx = context.secp_ctx();

    let pubkey = config.private_key.public_key(ctx);

    let address = Chroma::from(pubkey).to_address(config.network());

    println!("{}", address);

    Ok(())
}
