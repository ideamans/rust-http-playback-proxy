use clap::{Parser, Subcommand};
use std::path::PathBuf;
use crate::types::DeviceType;

#[derive(Parser)]
#[command(name = "http-playback-proxy")]
#[command(about = "HTTP playback proxy for recording and replaying HTTP traffic")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Record HTTP traffic")]
    Recording {
        #[arg(help = "Entry URL to start recording from")]
        entry_url: Option<String>,

        #[arg(short, long, help = "Port to use for the proxy server (default: auto-detect from 8080)")]
        port: Option<u16>,

        #[arg(short, long, default_value = "mobile", help = "Device type")]
        device: DeviceType,

        #[arg(short, long, default_value = "./inventory", help = "Inventory directory")]
        inventory: PathBuf,
    },

    #[command(about = "Playback recorded HTTP traffic")]
    Playback {
        #[arg(short, long, help = "Port to use for the proxy server (default: auto-detect from 8080)")]
        port: Option<u16>,

        #[arg(short, long, default_value = "./inventory", help = "Inventory directory")]
        inventory: PathBuf,
    },
}