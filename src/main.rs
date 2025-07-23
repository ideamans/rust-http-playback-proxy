use clap::Parser;
use tracing_subscriber;

mod cli;
mod types;
mod recording;
mod playback;
mod utils;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Recording {
            entry_url,
            port,
            device,
            inventory,
        } => {
            recording::run_recording_mode(entry_url, port, device, inventory).await?;
        }
        Commands::Playback { port, inventory } => {
            playback::run_playback_mode(port, inventory).await?;
        }
    }

    Ok(())
}
