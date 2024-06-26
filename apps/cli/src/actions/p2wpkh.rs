use bitcoin::Address;
use color_eyre::eyre;

use crate::context::Context;

/// Get the P2WPKH address for the given config.
pub(crate) fn run(mut context: Context) -> eyre::Result<()> {
    let config = context.config()?;
    let ctx = context.secp_ctx();

    let pubkey = config.private_key.public_key(ctx);

    let address = Address::p2wpkh(&pubkey, config.network())?;

    println!("{}", address);

    Ok(())
}
