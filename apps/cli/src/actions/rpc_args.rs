use clap::Args;

#[derive(Clone, Args, Debug)]
pub struct RpcArgs {
    /// RPC URL to the Bitcoin node. It's required only in case, when `bitcoin_provider`
    /// in the configuration file is specified to Esplora.
    #[clap(long)]
    pub rpc_url: Option<String>,
    /// RPC auth parameters in the following format: `[username]:[password]`.
    /// It is required only in cases when the Bitcoin node requires authentication
    /// with usage of --rpc-url flag.
    #[clap(long, requires = "rpc_url")]
    pub rpc_auth: Option<String>,
}
