use dex_client::{BirdEyeClient, BirdEyeConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    println!("Testing BirdEye API integration...");
    
    let config = BirdEyeConfig {
        api_key: "5ff313b239ac42e297b830b10ea1871d".to_string(),
        api_base_url: "https://public-api.birdeye.so".to_string(),
        request_timeout_seconds: 30,
        rate_limit_per_second: 100,
    };
    
    let client = BirdEyeClient::new(config)?;
    
    // Test wallet with known transactions
    let wallet = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa";
    
    println!("Fetching transactions for wallet: {}", wallet);
    
    match client.get_all_trader_transactions(wallet, None, None, Some(5)).await {
        Ok(transactions) => {
            println!("✅ SUCCESS: Retrieved {} transactions", transactions.len());
            if !transactions.is_empty() {
                println!("First transaction: {:?}", transactions[0]);
            }
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
    
    Ok(())
}