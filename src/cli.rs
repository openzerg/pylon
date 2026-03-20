use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pylon")]
#[command(about = "Pylon - LLM API Gateway for OpenZerg")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Start the gateway server")]
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

pub async fn handle_command(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Serve { port } => {
            crate::proxy::serve(port).await?;
        }
    }
    Ok(())
}