use std::path::PathBuf;

use crate::actions::chroma::ChromaCommands;
use burn::BurnArgs;
use clap::{Parser, Subcommand};
use clap_verbosity::Verbosity;
use color_eyre::eyre;
use decode::DecodeArgs;
use get::GetArgs;
use tracing_log::AsTrace;

use self::{
    convert::ConvertCommands, freeze::FreezeArgs, generate::GenerateCommands, issue::IssueArgs,
    provide::ProvideArgs, transfer::TransferArgs, utxos::UtxosArgs, validate::ValidateArgs,
    wallet::WalletCommands,
};
use crate::context::Context;

mod announcement_args;
mod balances;
#[cfg(feature = "bulletproof")]
mod bulletproof;
mod burn;
mod chroma;
mod convert;
mod decode;
mod freeze;
mod generate;
mod get;
mod issue;
mod p2tr;
mod p2wpkh;
mod proof;
mod provide;
mod rpc_args;
mod sweep;
mod transfer;
mod utxos;
mod validate;
mod wallet;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub verbosity: Verbosity,

    #[command(subcommand)]
    pub command: Commands,

    #[clap(short, long, default_value = "config.toml")]
    pub config: PathBuf,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Get balances of a user
    Balances,

    /// Generate secret key, public key and address.
    #[command(subcommand)]
    Generate(GenerateCommands),

    /// Convert instances between each other.
    #[command(subcommand)]
    Convert(ConvertCommands),

    /// Issue new tokens.
    Issue(IssueArgs),

    /// Transfer tokens
    Transfer(TransferArgs),

    /// Burn tokens
    Burn(BurnArgs),

    /// Decode raw YUV transaction
    Decode(DecodeArgs),

    /// Sweep tweaked Bitcoin UTXOs created with the YUV protocol.
    /// Outputs will be sweeped to a p2wpkh address.
    Sweep,

    /// Validate pixel proof of provided transaction.
    Validate(ValidateArgs),

    /// Send freeze transaction
    Freeze(FreezeArgs),

    /// Provide proof to node
    Provide(ProvideArgs),

    /// Get transaction data from node
    Get(GetArgs),

    /// Get a list of unspent transaction outputs with amounts
    Utxos(UtxosArgs),

    /// Provide abortion and syncing of the wallet
    #[command(subcommand)]
    Wallet(WalletCommands),

    /// Get the p2wpkh address of the current user.
    P2WPKH,

    /// Get the p2tr address of the current user as chroma created from
    /// XOnlyPubKey of the `private_key` specified in the config file.
    P2TR,

    // Bulletproof commands
    #[cfg(feature = "bulletproof")]
    #[command(subcommand)]
    Bulletproof(bulletproof::BulletproofCommands),

    /// Provides command to create Chroma announcement, and retrieve info about the token.
    #[command(subcommand)]
    Chroma(ChromaCommands),
}

impl Cli {
    pub async fn run(self) -> eyre::Result<()> {
        tracing_subscriber::fmt()
            .with_max_level(self.verbosity.log_level_filter().as_trace())
            .init();

        let context = Context::new(self.config);
        execute_command(self.command, context).await
    }
}

async fn execute_command(command: Commands, context: Context) -> eyre::Result<()> {
    use Commands as Cmd;
    match command {
        Cmd::Generate(cmd) => generate::run(cmd, context),
        Cmd::Issue(args) => issue::run(args, context).await,
        Cmd::Transfer(args) => transfer::run(args, context).await,
        Cmd::Burn(args) => burn::run(args, context).await,
        Cmd::Validate(args) => validate::run(args, context).await,
        Cmd::Freeze(args) => freeze::run(args, context).await,
        Cmd::Provide(args) => provide::run(args, context).await,
        Cmd::Get(args) => get::run(args, context).await,
        Cmd::Balances => balances::run(context).await,
        Cmd::Utxos(args) => utxos::run(args, context).await,
        Cmd::Wallet(cmd) => wallet::run(cmd, context).await,
        #[cfg(feature = "bulletproof")]
        Cmd::Bulletproof(cmd) => bulletproof::run(cmd, context).await,
        Cmd::Convert(args) => convert::run(args),
        Cmd::P2WPKH => p2wpkh::run(context),
        Cmd::P2TR => p2tr::run(context),
        Cmd::Sweep => sweep::run(context).await,
        Cmd::Chroma(cmd) => chroma::run(cmd, context).await,
        Cmd::Decode(args) => decode::run(args).await,
    }
}

/// Checks if all the arguments to the command are specified the same number of times.
#[macro_export]
macro_rules! check_equal_lengths {
    ($($args:expr),+ $(,)?) => {
        {
            let lengths = [$($args.len()),+];
            eyre::ensure!(lengths.iter().all(|&len| len == lengths[0]), "The number of the repeated arguments must match")
        }
    };
}
