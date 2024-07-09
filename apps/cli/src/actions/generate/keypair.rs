use bdk::miniscript::ToPublicKey;
use bitcoin::{
    address::{Payload, WitnessProgram, WitnessVersion},
    secp256k1::rand::thread_rng,
    Address, Network, PrivateKey,
};
use clap::Args;
use color_eyre::eyre;
use yuv_pixels::ToEvenPublicKey;

use crate::context::Context;

#[derive(Args, Debug)]
/// Generate P2WPKH YUV address from public key, amount and chroma.
pub struct GenerateKeypairArgs {
    /// Network to use.
    #[clap(long, short, default_value = "regtest")]
    pub network: Network,
}

/// Generate key, and print it to stdout.
pub fn run(
    GenerateKeypairArgs { network }: GenerateKeypairArgs,
    context: Context,
) -> eyre::Result<()> {
    let secp_ctx = context.secp_ctx();

    let (seckey, pubkey) = secp_ctx.generate_keypair(&mut thread_rng());

    let privkey = PrivateKey::new(seckey, network);
    let even_pubkey = privkey.even_public_key(secp_ctx);

    let (xonly, _parity) = even_pubkey.inner.x_only_public_key();

    // NOTE(Velnbur): The only way I found to add public key to taproot without
    // tweaking the key with hash of the merkle root:
    let p2tr = Address::new(
        network,
        Payload::WitnessProgram(
            WitnessProgram::new(WitnessVersion::V1, xonly.serialize().to_vec())
                .expect("Should be valid program"),
        ),
    );

    println!("Private key: {}", privkey);
    println!("P2TR address: {}", p2tr);

    let address = Address::p2wpkh(&pubkey.to_public_key(), network)?;
    println!("P2WPKH address: {}", address);

    Ok(())
}
