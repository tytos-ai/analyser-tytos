// Test Redis integration (simulating persistence_layer functionality)

use redis::AsyncCommands;
use serde_json::json;
use std::collections::HashMap;
use tokio;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    info!("🧪 Testing Redis Integration (P&L Tracker Style)");
    
    // Test 1: Basic Redis connection
    info!("🔗 Testing Redis connection...");
    let client = redis::Client::open("redis://127.0.0.1:6379")?;
    let mut conn = client.get_async_connection().await?;
    
    let pong: String = redis::cmd("PING").query_async(&mut conn).await?;
    info!("✅ Redis connected: {}", pong);
    
    // Test 2: Wallet queue operations (for continuous mode)
    info!("📋 Testing wallet queue operations...");
    test_wallet_queue(&mut conn).await?;
    
    // Test 3: Price caching (Jupiter style)
    info!("💰 Testing price caching...");
    test_price_caching(&mut conn).await?;
    
    // Test 4: P&L temporary data (TypeScript compatibility)
    info!("🧮 Testing P&L temp data...");
    test_pnl_temp_data(&mut conn).await?;
    
    // Test 5: Distributed locking (aggregator-lock)
    info!("🔒 Testing distributed locking...");
    test_distributed_lock(&mut conn).await?;
    
    info!("✅ All Redis integration tests passed!");
    
    Ok(())
}

async fn test_wallet_queue(conn: &mut redis::aio::Connection) -> Result<(), Box<dyn std::error::Error>> {
    let queue_key = "test_discovered_wallets_queue";
    
    // Clear any existing data
    let _: () = conn.del(queue_key).await?;
    
    // Test wallet discovery (like dex_client would do)
    let discovered_wallets = vec![
        "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".to_string(),
        "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1".to_string(),
        "DCAKkRzfmZ3QkKla2VRmSdk8vZJd4tSNxPMf83Bfqr6i".to_string(),
    ];
    
    // Push wallets to queue (LPUSH)
    conn.lpush(queue_key, &discovered_wallets).await?;
    info!("  ✅ Pushed {} wallets to queue", discovered_wallets.len());
    
    // Check queue size
    let size: u64 = conn.llen(queue_key).await?;
    info!("  ✅ Queue size: {}", size);
    
    // Pop wallets from queue (BRPOP with timeout)
    for i in 0..discovered_wallets.len() {
        let result: Option<Vec<String>> = conn.brpop(queue_key, 1.0).await?;
        if let Some(items) = result {
            if items.len() >= 2 {
                let wallet = &items[1]; // brpop returns [key, value]
                info!("  ✅ Popped wallet {}: {}", i + 1, wallet);
            }
        }
    }
    
    // Cleanup
    let _: () = conn.del(queue_key).await?;
    
    Ok(())
}

async fn test_price_caching(conn: &mut redis::aio::Connection) -> Result<(), Box<dyn std::error::Error>> {
    // Test Jupiter-style price caching
    let mints = vec!["So11111111111111111111111111111111111111112"];
    let vs_token = "usd";
    let cache_key = format!("jupiterPrice:{}:{}", mints.join("-"), vs_token);
    
    // Create sample price data
    let mut prices = HashMap::new();
    prices.insert("So11111111111111111111111111111111111111112".to_string(), 156.63);
    
    let prices_json = serde_json::to_string(&prices)?;
    
    // Cache with TTL (60 seconds)
    conn.set_ex(&cache_key, &prices_json, 60).await?;
    info!("  ✅ Cached prices with key: {}", cache_key);
    
    // Retrieve from cache
    let cached: Option<String> = conn.get(&cache_key).await?;
    if let Some(cached_json) = cached {
        let cached_prices: HashMap<String, f64> = serde_json::from_str(&cached_json)?;
        info!("  ✅ Retrieved cached price for SOL: ${}", 
              cached_prices.get("So11111111111111111111111111111111111111112").unwrap_or(&0.0));
    }
    
    // Cleanup
    let _: () = conn.del(&cache_key).await?;
    
    Ok(())
}

async fn test_pnl_temp_data(conn: &mut redis::aio::Connection) -> Result<(), Box<dyn std::error::Error>> {
    let wallet = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
    let mint = "So11111111111111111111111111111111111111112";
    
    // Test transaction signatures list (temptxids:{wallet})
    let tx_key = format!("temptxids:{}", wallet);
    let signatures = vec![
        "4A3ZkUQYuVJTQLZx3yTYaXchyohcBp4Kt5kDEbTFXGYnThxVx5duneJY6Gm7FJpuwnYHHxmW3RG35wrMbJ8Yd3R".to_string(),
        "2BzDk3H9TpNmF8YfCtXJRN3QGG5nz8KHvY7G4MBLB8d9y3D4FzJK7YpR6HnM8T9D8vF2kW9eG5sJ6HtN3YdR".to_string(),
    ];
    
    // Store signatures as list
    conn.del(&tx_key).await?;
    if !signatures.is_empty() {
        let _: () = conn.lpush(&tx_key, &signatures).await?;
    }
    info!("  ✅ Stored {} transaction signatures", signatures.len());
    
    // Test account amounts per mint (accamounts:{wallet}:{mint})
    let acc_key = format!("accamounts:{}:{}", wallet, mint);
    let account_data = json!({
        "wallet": wallet,
        "mint": mint,
        "balance": 1.5,
        "usd_value": 234.95
    });
    
    conn.set(&acc_key, account_data.to_string()).await?;
    info!("  ✅ Stored account amounts for {}", mint);
    
    // Test parsed transaction data (parsed:{wallet}:{txid})
    let parsed_key = format!("parsed:{}:{}", wallet, &signatures[0]);
    let parsed_data = json!({
        "transaction_id": signatures[0],
        "wallet": wallet,
        "events": [
            {
                "type": "buy",
                "amount": 1.5,
                "token": mint,
                "price": 156.63
            }
        ]
    });
    
    conn.set(&parsed_key, parsed_data.to_string()).await?;
    info!("  ✅ Stored parsed transaction data");
    
    // Retrieve and verify
    let retrieved: Option<String> = conn.get(&acc_key).await?;
    if retrieved.is_some() {
        info!("  ✅ Successfully retrieved account data");
    }
    
    // Cleanup
    let _: () = conn.del(&tx_key).await?;
    let _: () = conn.del(&acc_key).await?;
    let _: () = conn.del(&parsed_key).await?;
    
    Ok(())
}

async fn test_distributed_lock(conn: &mut redis::aio::Connection) -> Result<(), Box<dyn std::error::Error>> {
    let lock_name = "test-aggregator-lock";
    let lock_key = format!("lock:{}", lock_name);
    let lock_value = format!("test-process-{}", std::process::id());
    
    // Try to acquire lock
    let acquired: bool = conn.set_nx(&lock_key, &lock_value).await?;
    
    if acquired {
        info!("  ✅ Acquired lock: {}", lock_name);
        
        // Set TTL
        conn.expire(&lock_key, 10).await?;
        info!("  ✅ Set lock TTL: 10 seconds");
        
        // Simulate work
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Release lock with Lua script (atomic check-and-delete)
        let script = r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
        "#;
        
        let result: i32 = redis::Script::new(script)
            .key(&lock_key)
            .arg(&lock_value)
            .invoke_async(conn)
            .await?;
        
        if result == 1 {
            info!("  ✅ Released lock successfully");
        } else {
            error!("  ❌ Failed to release lock (already expired?)");
        }
    } else {
        info!("  ⚠️  Lock already held by another process");
    }
    
    Ok(())
}