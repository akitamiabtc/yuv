use crate::context::Context;
use color_eyre::eyre;

pub async fn run(mut ctx: Context) -> eyre::Result<()> {
    ctx.wallet().await?;

    Ok(())
}
