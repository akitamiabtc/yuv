use clap::Args;

#[derive(Args, Debug)]
pub struct ValidateHexArgs {
    /// Transaction hex representation.
    pub tx_hex: String,
}
