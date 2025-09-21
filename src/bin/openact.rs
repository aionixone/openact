use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let cli = openact::cli::Cli::parse();
    openact::cli::run(cli).await
}


