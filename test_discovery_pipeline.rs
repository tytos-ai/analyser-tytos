use anyhow::Result;
use dex_client::{TopTraderFilter};
use job_orchestrator::{BirdEyeTrendingOrchestrator, BirdEyeTrendingConfig};
use persistence_layer::RedisClient;
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("🔧 Testing BirdEye Discovery Pipeline");

    // Initialize Redis client
    let redis_client = match RedisClient::new("redis://127.0.0.1:6379").await {
        Ok(client) => Some(client),
        Err(e) => {
            info!("⚠️ Redis not available, testing without persistence: {}", e);
            None
        }
    };

    // Configure BirdEye trending orchestrator with very lenient filters
    let config = BirdEyeTrendingConfig {
        api_key: "5ff313b239ac42e297b830b10ea1871d".to_string(),
        api_base_url: "https://public-api.birdeye.so".to_string(),
        chain: "solana".to_string(),
        top_trader_filter: TopTraderFilter {
            min_volume_usd: 100.0, // Very low filter
            min_trades: 1,         // Very low filter
            min_win_rate: None,    // No win rate filter
            max_last_trade_hours: None, // No time filter
            max_traders: Some(5),
        },
        max_trending_tokens: 3,    // Test with just 3 tokens
        max_traders_per_token: 5,  // Get 5 traders per token
        cycle_interval_seconds: 60,
        debug_mode: true,          // Enable debug logging
    };

    // Create orchestrator
    let orchestrator = BirdEyeTrendingOrchestrator::new(config, redis_client)?;

    info!("🚀 Starting discovery cycle test...");
    
    // Execute one discovery cycle
    match orchestrator.execute_discovery_cycle().await {
        Ok(discovered_wallets) => {
            info!("✅ Discovery cycle completed successfully!");
            info!("📊 Discovered {} wallets", discovered_wallets);
            
            if discovered_wallets > 0 {
                info!("🎉 SUCCESS: Wallet discovery pipeline is working!");
            } else {
                info!("⚠️ No wallets discovered - this could be due to filters or lack of qualifying traders");
            }
        }
        Err(e) => {
            info!("❌ Discovery cycle failed: {}", e);
            return Err(e);
        }
    }

    Ok(())
}