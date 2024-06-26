use bitcoin::Txid;
use clap::Args;
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;

use crate::context::Context;

#[derive(Args, Debug, Clone)]
pub struct GetArgs {
    #[clap(long, short)]
    pub txid: Txid,
}

pub(crate) async fn run(
    GetArgs { txid }: GetArgs,
    mut context: Context,
) -> Result<(), color_eyre::Report> {
    let client = context.yuv_client()?;

    let tx = client.get_raw_yuv_transaction(txid).await?;

    println!("{}", serde_json::to_string_pretty(&tx)?);

    Ok(())
}
