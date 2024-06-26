use clap::{Args, Subcommand};
use color_eyre::eyre;

use crate::context::Context;

use self::{fetch::CheckFetchArgs, hex::ValidateHexArgs};

use super::proof::ProofListArgs;

mod fetch;
mod hex;

#[derive(Args, Debug)]
pub struct ValidateArgs {
    #[command(subcommand)]
    pub command: ValidateCommand,

    #[clap(flatten)]
    pub proofs: ProofListArgs,
}

#[derive(Subcommand, Debug)]
pub enum ValidateCommand {
    /// Fetch the transaction from chain.
    Fetch(CheckFetchArgs),
    /// Parse transaction from it's hex representation (UNIMPLEMENTED)
    Tx(ValidateHexArgs),
}

pub(crate) async fn run(
    ValidateArgs { command, proofs }: ValidateArgs,
    context: Context,
) -> eyre::Result<()> {
    match command {
        ValidateCommand::Fetch(args) => fetch::run(proofs, args, context).await,
        _ => unimplemented!(),
    }
}
