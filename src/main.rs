use clap::Parser;

mod beautify;
mod cli;
mod ghost_server;
mod llm;
mod llmgen;
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
        Commands::Llm { format, regenerate } => {
            if regenerate {
                llmgen::regenerate()?;
            } else {
                print!("{}", llm::render(&format)?);
            }
        }
        Commands::Recording {
            entry_url,
            port,
            device,
            inventory,
            extra_urls,
            exclude_patterns,
        } => {
            let extra = if extra_urls.is_empty() {
                None
            } else {
                Some(extra_urls)
            };
            recording::run_recording_mode(
                entry_url,
                extra,
                port,
                device,
                inventory,
                exclude_patterns,
            )
            .await?;
        }
        Commands::Playback {
            port,
            inventory,
            full_throttle,
            passthrough,
        } => {
            playback::run_playback_mode(port, inventory, full_throttle, passthrough).await?;
        }
        Commands::Signal { pid, kind } => {
            let signal_kind = signal_sender::SignalKind::from_str(&kind)?;
            signal_sender::send_signal(pid, signal_kind)?;
            println!("Signal sent successfully to process {}", pid);
        }
    }

    Ok(())
}
