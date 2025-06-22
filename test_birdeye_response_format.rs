use anyhow::Result;
use config_manager::SystemConfig;
use dex_client::{BirdEyeClient, BirdEyeConfig};
use serde_json;
use tokio;
use tracing::{info, debug, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("🔍 Examining BirdEye API Response Format and Parsing");

    // Load system config
    let system_config = SystemConfig::load()?;
    
    // Create BirdEye client
    let birdeye_config = BirdEyeConfig {
        api_key: system_config.birdeye.api_key.clone(),
        api_base_url: system_config.birdeye.api_base_url.clone(),
        request_timeout_seconds: system_config.birdeye.request_timeout_seconds,
        rate_limit_per_second: system_config.birdeye.rate_limit_per_second,
    };
    
    let birdeye_client = BirdEyeClient::new(birdeye_config)?;
    let test_wallet = "HV1KXxWFaSeriyFvXyx48FqG9BoFbfinB8njCJonqP7K";
    let test_token = "71Jvq4Epe2FCJ7JFSF7jLXdNk1Wy4Bhqd9iL6bEFELvg"; // GOR token

    info!("🎯 Testing both BirdEye API endpoints for wallet: {}", test_wallet);
    
    // Test 1: The endpoint that was failing (get_trader_transactions)
    info!("\n=== TEST 1: get_trader_transactions (was failing) ===");
    info!("Endpoint: /trader/txs/seek_by_time with wallet + token parameters");
    
    match birdeye_client.get_trader_transactions(
        test_wallet, 
        test_token, 
        None, 
        None, 
        Some(5) // Just get 5 for examination
    ).await {
        Ok(transactions) => {
            info!("✅ SUCCESS: get_trader_transactions returned {} transactions", transactions.len());
            if !transactions.is_empty() {
                info!("📋 Sample transaction structure (TraderTransaction):");
                let sample = &transactions[0];
                info!("  tx_hash: {}", sample.tx_hash);
                info!("  side: {}", sample.side);
                info!("  token_address: {}", sample.token_address);
                info!("  token_amount: {}", sample.token_amount);
                info!("  token_price: {}", sample.token_price);
                info!("  volume_usd: {}", sample.volume_usd);
                info!("  source: {:?}", sample.source);
                
                // Show raw JSON for comparison
                info!("📄 Sample as JSON:");
                if let Ok(json) = serde_json::to_string_pretty(sample) {
                    info!("{}", json);
                }
            }
        }
        Err(e) => {
            info!("❌ FAILED: get_trader_transactions error: {}", e);
        }
    }

    // Test 2: The endpoint that works (get_all_trader_transactions)
    info!("\n=== TEST 2: get_all_trader_transactions (working) ===");
    info!("Endpoint: /trader/txs/seek_by_time with wallet only parameters");
    
    match birdeye_client.get_all_trader_transactions(
        test_wallet, 
        None, 
        None, 
        Some(5) // Just get 5 for examination
    ).await {
        Ok(transactions) => {
            info!("✅ SUCCESS: get_all_trader_transactions returned {} transactions", transactions.len());
            if !transactions.is_empty() {
                info!("📋 Sample transaction structure (GeneralTraderTransaction):");
                let sample = &transactions[0];
                info!("  tx_hash: {}", sample.tx_hash);
                info!("  tx_type: {}", sample.tx_type);
                info!("  source: {}", sample.source);
                info!("  owner: {}", sample.owner);
                info!("  quote.symbol: {}", sample.quote.symbol);
                info!("  quote.address: {}", sample.quote.address);
                info!("  quote.ui_amount: {}", sample.quote.ui_amount);
                info!("  quote.type_swap: {}", sample.quote.type_swap);
                info!("  base.symbol: {}", sample.base.symbol);
                info!("  base.address: {}", sample.base.address);
                info!("  base.ui_amount: {}", sample.base.ui_amount);
                info!("  base.type_swap: {}", sample.base.type_swap);
                
                // Show raw JSON for comparison
                info!("📄 Sample as JSON:");
                if let Ok(json) = serde_json::to_string_pretty(sample) {
                    info!("{}", json);
                }
            }
        }
        Err(e) => {
            info!("❌ FAILED: get_all_trader_transactions error: {}", e);
        }
    }

    // Test 3: Raw API analysis
    info!("\n=== TEST 3: Raw API Response Analysis ===");
    info!("Understanding why the same endpoint returns different formats...");
    
    // Manually construct the requests to see the difference
    let http_client = reqwest::Client::new();
    
    // Request 1: With token_address parameter (what get_trader_transactions does)
    info!("📡 Making request WITH token_address parameter...");
    let url1 = format!("{}/trader/txs/seek_by_time", system_config.birdeye.api_base_url);
    let response1 = http_client
        .get(&url1)
        .header("X-API-KEY", &system_config.birdeye.api_key)
        .header("x-chain", "solana")
        .query(&[
            ("address", test_wallet),
            ("token_address", test_token),
            ("limit", "2"),
        ])
        .send()
        .await?;
    
    info!("📊 Response 1 status: {}", response1.status());
    let body1 = response1.text().await?;
    info!("📄 Response 1 body length: {} characters", body1.len());
    info!("📄 Response 1 first 500 chars: {}", &body1[..body1.len().min(500)]);
    
    // Request 2: Without token_address parameter (what get_all_trader_transactions does)
    info!("\n📡 Making request WITHOUT token_address parameter...");
    let response2 = http_client
        .get(&url1)
        .header("X-API-KEY", &system_config.birdeye.api_key)
        .header("x-chain", "solana")
        .query(&[
            ("address", test_wallet),
            ("limit", "2"),
        ])
        .send()
        .await?;
    
    info!("📊 Response 2 status: {}", response2.status());
    let body2 = response2.text().await?;
    info!("📄 Response 2 body length: {} characters", body2.len());
    info!("📄 Response 2 first 500 chars: {}", &body2[..body2.len().min(500)]);
    
    // Compare responses
    info!("\n=== COMPARISON ===");
    info!("🔍 Same endpoint, different parameters:");
    info!("  With token_address: {} chars", body1.len());
    info!("  Without token_address: {} chars", body2.len());
    info!("  Bodies are identical: {}", body1 == body2);
    
    if body1 != body2 {
        info!("❗ Different responses! token_address parameter DOES affect the response format");
    } else {
        info!("✅ Same responses. token_address parameter is ignored by BirdEye API");
    }

    Ok(())
}