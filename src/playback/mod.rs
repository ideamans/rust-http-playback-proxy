use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use crate::utils::get_port_or_default;
use crate::types::Inventory;
use crate::traits::{FileSystem, RealFileSystem};

mod proxy;
mod transaction;
mod tests;

#[cfg(test)]
mod transaction_tests;

#[cfg(test)]
mod inventory_tests;

pub async fn run_playback_mode(port: Option<u16>, inventory_dir: PathBuf, _ignore_tls_errors: bool) -> Result<()> {
    let port = get_port_or_default(port)?;

    println!("Starting playback mode on port {}", port);
    println!("Inventory directory: {:?}", inventory_dir);

    // Note: ignore_tls_errors is not needed in playback mode since we serve prerecorded responses

    // Load inventory
    let file_system = Arc::new(RealFileSystem);
    let inventory = load_inventory(&inventory_dir, file_system.clone()).await?;
    
    println!("Loaded {} resources from inventory", inventory.resources.len());
    
    // Convert resources to transactions
    let transactions = transaction::convert_resources_to_transactions(&inventory, &inventory_dir, file_system).await?;
    
    println!("Created {} transactions", transactions.len());

    proxy::start_playback_proxy(port, transactions).await
}

pub async fn load_inventory<F: FileSystem>(
    inventory_dir: &PathBuf,
    file_system: Arc<F>,
) -> Result<Inventory> {
    let inventory_path = inventory_dir.join("inventory.json");
    let inventory_content = file_system.read_to_string(&inventory_path).await?;
    let inventory: Inventory = serde_json::from_str(&inventory_content)?;
    Ok(inventory)
}