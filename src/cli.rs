use crate::types::DeviceType;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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

        #[arg(
            short,
            long,
            help = "Port to use for the proxy server (default: auto-detect from 18080)"
        )]
        port: Option<u16>,

        #[arg(short, long, default_value = "mobile", help = "Device type")]
        device: DeviceType,

        #[arg(
            short,
            long,
            default_value = "./inventory",
            help = "Inventory directory"
        )]
        inventory: PathBuf,
    },

    #[command(about = "Playback recorded HTTP traffic")]
    Playback {
        #[arg(
            short,
            long,
            help = "Port to use for the proxy server (default: auto-detect from 18080)"
        )]
        port: Option<u16>,

        #[arg(
            short,
            long,
            default_value = "./inventory",
            help = "Inventory directory"
        )]
        inventory: PathBuf,
    },

    /// Send signal to a process (internal helper, primarily for Windows)
    #[command(hide = true)]
    Signal {
        #[arg(long, help = "Process ID to send signal to")]
        pid: u32,

        #[arg(
            long,
            help = "Signal kind: ctrl-break (Windows CTRL_BREAK), ctrl-c (Windows CTRL_C), term (Unix SIGTERM), int (Unix SIGINT)"
        )]
        kind: String,
    },
}
