use clap::Parser;
use pylon::cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    pylon::cli::handle_command(cli).await
}