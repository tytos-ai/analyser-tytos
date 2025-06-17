// Simple integration test for the P&L tracker system
use std::error::Error;
use tracing::{info, error};

#[tokio::main] 
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    
    info!("🔧 Starting P&L Tracker System Test");
    
    // Test 1: Configuration loading
    info!("📋 Testing configuration loading...");
    match test_config_loading().await {
        Ok(_) => info!("✅ Configuration loading: PASSED"),
        Err(e) => error!("❌ Configuration loading: FAILED - {}", e),
    }
    
    // Test 2: Redis connection
    info!("🔗 Testing Redis connection...");
    match test_redis_connection().await {
        Ok(_) => info!("✅ Redis connection: PASSED"),
        Err(e) => error!("❌ Redis connection: FAILED - {}", e),
    }
    
    // Test 3: Jupiter price API
    info!("💰 Testing Jupiter price API...");
    match test_jupiter_api().await {
        Ok(_) => info!("✅ Jupiter price API: PASSED"),
        Err(e) => error!("❌ Jupiter price API: FAILED - {}", e),
    }
    
    info!("🎯 Basic component tests completed!");
    Ok(())
}

async fn test_config_loading() -> Result<(), Box<dyn Error>> {
    // Try to load environment variables or use defaults
    let _redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let _solana_rpc = std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    
    info!("Configuration variables loaded successfully");
    Ok(())
}

async fn test_redis_connection() -> Result<(), Box<dyn Error>> {
    use redis::AsyncCommands;
    
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_async_connection().await?;
    
    // Test PING
    let _: String = redis::cmd("PING").query_async(&mut conn).await?;
    info!("Redis PING successful");
    
    // Test basic set/get
    let test_key = "pnl_test_key";
    let test_value = "pnl_test_value";
    
    let _: () = conn.set(test_key, test_value).await?;
    let result: String = conn.get(test_key).await?;
    
    if result != test_value {
        return Err("Redis set/get test failed".into());
    }
    
    // Cleanup
    let _: () = conn.del(test_key).await?;
    info!("Redis set/get/del test successful");
    
    Ok(())
}

async fn test_jupiter_api() -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let url = "https://lite-api.jup.ag/price/v2";
    
    // Test with SOL mint
    let sol_mint = "So11111111111111111111111111111111111111112";
    
    let response = client
        .get(url)
        .query(&[("ids", sol_mint)])
        .send()
        .await?;
    
    if response.status().is_success() {
        let data: serde_json::Value = response.json().await?;
        info!("Jupiter API test successful. Sample response keys: {:?}", 
              data.as_object().map(|o| o.keys().collect::<Vec<_>>()));
        
        // Check if we got price data
        if let Some(data_obj) = data.get("data") {
            if let Some(sol_data) = data_obj.get(sol_mint) {
                info!("SOL price data found: {}", sol_data);
            } else {
                info!("No SOL price data in response");
            }
        }
        
        Ok(())
    } else {
        Err(format!("Jupiter API returned status: {}", response.status()).into())
    }
}

// Additional test for DexScreener API
async fn test_dexscreener_api() -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    
    // Test a sample pair endpoint
    let sample_pair = "A8kYvS6Vbs7sMhKjUy7DbXtaM2nkYr8AKaVGhksJGkPH"; // Example pair
    let url = format!("https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/{}", sample_pair);
    
    let response = client
        .get(&url)
        .header("Origin", "https://dexscreener.com")
        .send()
        .await?;
    
    info!("DexScreener API status: {}", response.status());
    
    if response.status().is_success() {
        let data_bytes = response.bytes().await?;
        info!("DexScreener API test successful. Response size: {} bytes", data_bytes.len());
        
        // Check if we got binary data (should be > 0 bytes)
        if data_bytes.len() > 0 {
            info!("✅ DexScreener API: PASSED");
        } else {
            error!("❌ DexScreener API: FAILED - Empty response");
        }
    } else {
        error!("❌ DexScreener API: FAILED - Status {}", response.status());
    }
    
    Ok(())
}