use std::process::exit;

use clap::{ArgGroup, Args};
use yuv_types::{YuvTransaction, YuvTxType};

#[derive(Args, Debug, Clone)]
#[clap(group(
    ArgGroup::new("decode")
        .required(true)
        .args(&["tx", "proofs"])
        .multiple(false),
))]
pub struct DecodeArgs {
    pub hex: String,

    #[clap(long, group = "decode")]
    pub tx: bool,

    #[clap(long, group = "decode")]
    pub proofs: bool,
}

pub(crate) async fn run(
    DecodeArgs { hex, tx, proofs }: DecodeArgs,
) -> Result<(), color_eyre::Report> {
    if tx {
        let Ok(yuv_tx) = YuvTransaction::from_hex(hex) else {
            eprintln!("The hex value could not be parsed as a YUV transaction");
            exit(1);
        };
        println!("{}", serde_json::to_string_pretty(&yuv_tx)?);
        return Ok(());
    }

    if proofs {
        let Ok(tx_type) = YuvTxType::from_hex(hex) else {
            eprintln!("The hex value could not be parsed as a YUV proof");
            exit(1);
        };
        println!("{}", serde_json::to_string_pretty(&tx_type)?);
        return Ok(());
    }

    Ok(())
}
