use clap::Subcommand;
use color_eyre::eyre;

use crate::context::Context;

use self::{check::CheckArgs, issue::IssueArgs, transfer::TransferArgs};

mod check;
mod dh;
mod issue;
mod transfer;

#[derive(Subcommand, Debug)]
pub enum BulletproofCommands {
    // bulletproof issue
    Issue(IssueArgs),
    // bulletproof transfer
    Transfer(TransferArgs),
    // bulletproof check
    Check(CheckArgs),
    // bulletproof dh
    Dh(dh::DhArgs),
}

pub async fn run(cmd: BulletproofCommands, context: Context) -> eyre::Result<()> {
    match cmd {
        BulletproofCommands::Issue(args) => issue::run(args, context).await,
        BulletproofCommands::Transfer(args) => transfer::run(args, context).await,
        BulletproofCommands::Check(args) => check::run(args, context).await,
        BulletproofCommands::Dh(args) => dh::run(args, context),
    }
}
