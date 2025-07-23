use anyhow::Result;
use std::path::PathBuf;
use crate::utils::get_port_or_default;
use crate::types::Inventory;

mod proxy;
mod transaction;
mod tests;

pub async fn run_playback_mode(port: Option<u16>, inventory_dir: PathBuf) -> Result<()> {
    let port = get_port_or_default(port)?;
    
    println!("Starting playback mode on port {}", port);
    println!("Inventory directory: {:?}", inventory_dir);

    // Load inventory
    let inventory = load_inventory(&inventory_dir).await?;
    
    println!("Loaded {} resources from inventory", inventory.resources.len());
    
    // Convert resources to transactions
    let transactions = transaction::convert_resources_to_transactions(&inventory, &inventory_dir).await?;
    
    println!("Created {} transactions", transactions.len());

    proxy::start_playback_proxy(port, transactions).await
}

pub async fn load_inventory(inventory_dir: &PathBuf) -> Result<Inventory> {
    let inventory_path = inventory_dir.join("inventory.json");
    let inventory_content = tokio::fs::read_to_string(inventory_path).await?;
    let inventory: Inventory = serde_json::from_str(&inventory_content)?;
    Ok(inventory)
}