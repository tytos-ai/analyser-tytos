use anyhow::Result;
use dex_client::{BirdEyeClient, BirdEyeConfig};
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("🔧 Testing BirdEye Top Traders API");

    // Create BirdEye client
    let config = BirdEyeConfig {
        api_base_url: "https://public-api.birdeye.so".to_string(),
        api_key: "5ff313b239ac42e297b830b10ea1871d".to_string(),
        request_timeout_seconds: 30,
        rate_limit_per_second: 100,
    };

    let client = BirdEyeClient::new(config)?;

    // Test with the GOR token that we know works with curl
    let token_address = "71Jvq4Epe2FCJ7JFSF7jLXdNk1Wy4Bhqd9iL6bEFELvg";
    
    info!("🎯 Testing top traders for token: {}", token_address);

    match client.get_top_traders(token_address, Some(10)).await {
        Ok(traders) => {
            info!("✅ SUCCESS: Retrieved {} top traders", traders.len());
            for (i, trader) in traders.iter().enumerate().take(3) {
                info!("  {}. {} - Volume: ${:.0}, Trades: {}", 
                       i + 1, trader.owner, trader.volume, trader.trade);
            }
        }
        Err(e) => {
            info!("❌ FAILED: {}", e);
            
            // Let's also try getting the raw error details
            info!("🔍 Raw error: {:?}", e);
        }
    }

    Ok(())
}