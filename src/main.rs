use clap::Parser;

mod beautify;
mod cli;
mod playback;
mod recording;
mod signal_sender;
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
        } => {
            recording::run_recording_mode(entry_url, port, device, inventory).await?;
        }
        Commands::Playback { port, inventory } => {
            playback::run_playback_mode(port, inventory).await?;
        }
        Commands::Signal { pid, kind } => {
            let signal_kind = signal_sender::SignalKind::from_str(&kind)?;
            signal_sender::send_signal(pid, signal_kind)?;
            println!("Signal sent successfully to process {}", pid);
        }
    }

    Ok(())
}
