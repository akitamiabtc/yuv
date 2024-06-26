use bitcoin::{
    address::{Payload, WitnessProgram, WitnessVersion},
    Address,
};
use bulletproof::util::ecdh;
use clap::Args;
use color_eyre::eyre;
use yuv_pixels::Chroma;

use crate::context::Context;

#[derive(Args, Debug)]
pub struct DhArgs {
    #[clap(long, value_parser = Chroma::from_address)]
    pub recipient: Chroma,
}

pub fn run(DhArgs { recipient }: DhArgs, mut context: Context) -> eyre::Result<()> {
    let config = context.config()?;

    let recipient = recipient.public_key();

    let dh_key = ecdh(config.private_key, recipient, config.network())?;

    let pubkey = dh_key.public_key(context.secp_ctx());

    let (xonly, _) = pubkey.inner.x_only_public_key();

    let p2tr = Address::new(
        config.network(),
        Payload::WitnessProgram(
            WitnessProgram::new(WitnessVersion::V1, xonly.serialize().to_vec())
                .expect("Should be valid program"),
        ),
    );

    println!("DH key: {}", dh_key);
    println!("DH P2TR address: {}", p2tr);

    let address = Address::p2wpkh(&pubkey, config.network())?;
    println!("DH P2WPKH address: {}", address);

    Ok(())
}
