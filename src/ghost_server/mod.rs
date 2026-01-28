//! Ghost Server - Virtual Host HTTP Server for Playback
//!
//! This module provides a stable HTTP server that serves recorded resources
//! with timing control. It operates as a virtual host server, matching requests
//! based on Host header and path.

// Allow dead code for helper functions that may be used in future phases
#![allow(dead_code)]

mod handler;
mod server;

#[cfg(test)]
mod tests;

pub use server::GhostServer;

use crate::traits::FileSystem;
use crate::types::{Inventory, Transaction};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// Load inventory from the specified directory
pub async fn load_inventory<F: FileSystem>(
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Inventory> {
    let inventory_path = inventory_dir.join("index.json");
    let inventory_content = file_system.read_to_string(&inventory_path).await?;
    let inventory: Inventory = serde_json::from_str(&inventory_content)?;
    Ok(inventory)
}

/// Convert resources to transactions (re-export from playback for now)
pub async fn convert_resources_to_transactions<F: FileSystem>(
    inventory: &Inventory,
    inventory_dir: &Path,
    file_system: Arc<F>,
) -> Result<Vec<Transaction>> {
    // Re-use the existing transaction conversion logic
    crate::playback::transaction::convert_resources_to_transactions(
        inventory,
        inventory_dir,
        file_system,
    )
    .await
}
