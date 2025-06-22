use anyhow::Result;
use config_manager::SystemConfig;
use dex_client::{BirdEyeClient, BirdEyeConfig, TopTraderFilter};
use job_orchestrator::{BirdEyeTrendingOrchestrator, BirdEyeTrendingConfig};
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("🔧 Testing Configuration Loading and BirdEye Integration");

    // Load system config the same way the API server does
    let system_config = SystemConfig::load()?;
    
    info!("📋 System config loaded successfully");
    info!("🔑 BirdEye API Key: {}", if system_config.birdeye.api_key.is_empty() { 
        "❌ EMPTY" 
    } else { 
        "✅ PROVIDED" 
    });
    info!("🌐 BirdEye API URL: {}", system_config.birdeye.api_base_url);

    // Create the same BirdEye config that the orchestrator would use
    let birdeye_config = BirdEyeTrendingConfig {
        api_key: system_config.birdeye.api_key.clone(),
        api_base_url: system_config.birdeye.api_base_url.clone(),
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

    // Create the orchestrator the same way the service manager does
    let orchestrator = BirdEyeTrendingOrchestrator::new(birdeye_config, None)?;

    info!("🚀 Testing orchestrator discovery cycle...");
    
    // Execute one discovery cycle
    match orchestrator.execute_discovery_cycle().await {
        Ok(discovered_wallets) => {
            info!("✅ Discovery cycle completed successfully!");
            info!("📊 Discovered {} wallets", discovered_wallets);
        }
        Err(e) => {
            info!("❌ Discovery cycle failed: {}", e);
            
            // Also test the BirdEye client directly with the same config
            info!("🔧 Testing direct BirdEye client...");
            let direct_config = BirdEyeConfig {
                api_base_url: system_config.birdeye.api_base_url.clone(),
                api_key: system_config.birdeye.api_key.clone(),
                request_timeout_seconds: 30,
                rate_limit_per_second: 100,
            };
            
            let direct_client = BirdEyeClient::new(direct_config)?;
            match direct_client.get_trending_tokens("solana").await {
                Ok(tokens) => {
                    info!("✅ Direct client got {} trending tokens", tokens.len());
                    if let Some(token) = tokens.first() {
                        match direct_client.get_top_traders(&token.address, Some(5)).await {
                            Ok(traders) => {
                                info!("✅ Direct client got {} traders for {}", traders.len(), token.symbol);
                            }
                            Err(e) => {
                                info!("❌ Direct client failed to get traders: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    info!("❌ Direct client failed to get tokens: {}", e);
                }
            }
            
            return Err(e);
        }
    }

    Ok(())
}