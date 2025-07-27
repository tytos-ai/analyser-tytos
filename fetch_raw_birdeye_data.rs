use dex_client::{BirdEyeClient, BirdEyeConfig};
use config_manager::SystemConfig;
use chrono::{DateTime, Utc, TimeZone};
use serde_json;
use std::fs;
use tokio;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::init();

    // Load configuration
    let config = SystemConfig::load().expect("Failed to load configuration");
    
    // Create BirdEye client
    let birdeye_client = BirdEyeClient::new(config.birdeye.clone())?;
    
    // Wallet address to fetch data for
    let wallet_address = "5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw";
    
    // Date range: June 1, 2025 to July 26, 2025
    let from_time = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap().timestamp();
    let to_time = Utc.with_ymd_and_hms(2025, 7, 26, 23, 59, 59).unwrap().timestamp();
    
    info!("Fetching BirdEye transaction data for wallet: {}", wallet_address);
    info!("Date range: {} to {}", 
          DateTime::from_timestamp(from_time, 0).unwrap().format("%Y-%m-%d %H:%M:%S"),
          DateTime::from_timestamp(to_time, 0).unwrap().format("%Y-%m-%d %H:%M:%S"));
    
    // Fetch all transactions with pagination to get comprehensive data
    match birdeye_client.get_all_trader_transactions_paginated(
        wallet_address,
        Some(from_time),
        Some(to_time),
        2000, // Fetch up to 2000 transactions
    ).await {
        Ok(transactions) => {
            info!("Successfully fetched {} raw BirdEye transactions", transactions.len());
            
            // Serialize to JSON with pretty formatting
            let json_data = serde_json::to_string_pretty(&transactions)?;
            
            // Write to file
            let filename = "raw_birdeye_transactions.json";
            fs::write(filename, json_data)?;
            
            info!("Raw BirdEye transaction data saved to: {}", filename);
            info!("Transaction data summary:");
            info!("- Total transactions: {}", transactions.len());
            
            if !transactions.is_empty() {
                let first_tx = &transactions[0];
                let last_tx = &transactions[transactions.len() - 1];
                
                info!("- Date range in data: {} to {}", 
                      DateTime::from_timestamp(first_tx.block_unix_time, 0)
                          .unwrap()
                          .format("%Y-%m-%d %H:%M:%S"),
                      DateTime::from_timestamp(last_tx.block_unix_time, 0)
                          .unwrap()
                          .format("%Y-%m-%d %H:%M:%S"));
                
                // Sample transaction info
                info!("- Sample transaction hash: {}", first_tx.tx_hash);
                info!("- Sample volume USD: ${:.2}", first_tx.volume_usd);
                info!("- Sample quote token: {} ({})", first_tx.quote.symbol, first_tx.quote.address);
                info!("- Sample base token: {} ({})", first_tx.base.symbol, first_tx.base.address);
            }
        }
        Err(e) => {
            error!("Failed to fetch BirdEye transaction data: {}", e);
            return Err(Box::new(e));
        }
    }
    
    Ok(())
}