use crate::context::Context;
use bitcoin::Address;
use clap::Args;
use color_eyre::eyre;
use yuv_pixels::Chroma;
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;

/// Arguments to request the information about the token from YUV node by its [`Chroma`].
#[derive(Clone, Args, Debug)]
pub struct InfoArgs {
    /// The [`Chroma`] of the token to get the information about.
    #[clap(long, short, value_parser = Chroma::from_address)]
    pub chroma: Chroma,
}

pub async fn run(args: InfoArgs, mut context: Context) -> eyre::Result<()> {
    let client = context.yuv_client()?;
    let config = context.config()?;

    let chroma_info_opt = client.get_chroma_info(args.chroma).await?;

    let Some(chroma_info) = chroma_info_opt else {
        println!("Token info not found");

        return Ok(());
    };

    println!("Chroma: {}", args.chroma.to_address(config.network()));

    if let Some(announcement) = chroma_info.announcement {
        println!("Name: {}", announcement.name);
        println!("Symbol: {}", announcement.symbol);
        println!("Decimal: {}", announcement.decimal);

        let max_supply = if announcement.max_supply == 0 {
            "unlimited".to_owned()
        } else {
            announcement.max_supply.to_string()
        };
        println!("Max supply: {}", max_supply);
        println!("Is freezable: {}", announcement.is_freezable);
    };

    println!("Total supply: {}", chroma_info.total_supply);

    let network = config.network();
    let address = if let Some(owner_script) = chroma_info.owner {
        Address::from_script(&owner_script, network)?
    } else {
        args.chroma.to_address(network)
    };
    println!("Owner address: {}", address);

    Ok(())
}
