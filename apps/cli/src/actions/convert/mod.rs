use clap::Subcommand;
use color_eyre::eyre;

use crate::actions::convert::p2tr_to_pubkey::P2trToPubkeyArgs;

mod p2tr_to_pubkey;

/// Commands for conversion of something into something.
#[derive(Subcommand, Debug)]
pub enum ConvertCommands {
    /// Convert a Taproot address to a public key.
    P2trToPubkey(P2trToPubkeyArgs),
}

pub fn run(cmd: ConvertCommands) -> eyre::Result<()> {
    match cmd {
        ConvertCommands::P2trToPubkey(args) => p2tr_to_pubkey::run(args),
    }
}
