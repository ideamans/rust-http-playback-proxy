use crate::traits::{FileSystem, RealFileSystem};
use crate::types::Inventory;
use crate::utils::get_port_or_default;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod proxy;
mod tests;
mod transaction;

#[cfg(test)]
mod transaction_tests;

#[cfg(test)]
mod inventory_tests;

pub async fn run_playback_mode(
    port: Option<u16>,
    inventory_dir: PathBuf,
    control_port: Option<u16>,
) -> Result<()> {
    let port = get_port_or_default(port)?;

    println!("Starting playback mode on port {}", port);
    println!("Inventory directory: {:?}", inventory_dir);

    if let Some(ctrl_port) = control_port {
        println!("Control API port: {}", ctrl_port);
    }

    // Load inventory
    let file_system = Arc::new(RealFileSystem);
    let inventory = load_inventory(&inventory_dir, file_system.clone()).await?;

    println!(
        "Loaded {} resources from inventory",
        inventory.resources.len()
    );

    // Convert resources to transactions
    let transactions =
        transaction::convert_resources_to_transactions(&inventory, &inventory_dir, file_system)
            .await?;

    println!("Created {} transactions", transactions.len());

    proxy::start_playback_proxy(port, transactions, control_port).await
}

pub async fn load_inventory<F: FileSystem>(
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Inventory> {
    let inventory_path = inventory_dir.join("index.json");
    let inventory_content = file_system.read_to_string(&inventory_path).await?;
    let inventory: Inventory = serde_json::from_str(&inventory_content)?;
    Ok(inventory)
}
