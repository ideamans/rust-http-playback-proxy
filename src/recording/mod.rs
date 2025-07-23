use anyhow::Result;
use std::path::PathBuf;
use crate::types::{DeviceType, Inventory};
use crate::utils::get_port_or_default;

mod proxy;
mod processor;
mod tests;

pub async fn run_recording_mode(
    entry_url: Option<String>,
    port: Option<u16>,
    device: DeviceType,
    inventory_dir: PathBuf,
) -> Result<()> {
    let port = get_port_or_default(port)?;
    
    println!("Starting recording mode on port {}", port);
    println!("Device type: {:?}", device);
    println!("Inventory directory: {:?}", inventory_dir);
    
    if let Some(url) = &entry_url {
        println!("Entry URL: {}", url);
    }

    let mut inventory = Inventory::new();
    inventory.entry_url = entry_url.clone();
    inventory.device_type = Some(device);

    proxy::start_recording_proxy(port, inventory, inventory_dir).await
}