use clap::Parser;

mod beautify;
mod cli;
mod playback;
mod recording;
mod traits;
mod types;
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
            control_port,
        } => {
            recording::run_recording_mode(entry_url, port, device, inventory, control_port).await?;
        }
        Commands::Playback {
            port,
            inventory,
            control_port,
        } => {
            playback::run_playback_mode(port, inventory, control_port).await?;
        }
    }

    Ok(())
}
