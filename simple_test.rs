// Simple integration test for the P&L tracker system
use tokio;

#[tokio::main] 
async fn main() {
    println!("🔧 Starting P&L Tracker System Test");
    
    // Test 1: Configuration loading
    println!("📋 Testing configuration loading...");
    match test_config_loading().await {
        Ok(_) => println!("✅ Configuration loading: PASSED"),
        Err(e) => println!("❌ Configuration loading: FAILED - {}", e),
    }
    
    // Test 2: Redis connection
    println!("🔗 Testing Redis connection...");
    match test_redis_connection().await {
        Ok(_) => println!("✅ Redis connection: PASSED"),
        Err(e) => println!("❌ Redis connection: FAILED - {}", e),
    }
    
    // Test 3: Jupiter price API
    println!("💰 Testing Jupiter price API...");
    match test_jupiter_api().await {
        Ok(_) => println!("✅ Jupiter price API: PASSED"),
        Err(e) => println!("❌ Jupiter price API: FAILED - {}", e),
    }
    
    println!("🎯 Basic component tests completed!");
}

async fn test_config_loading() -> Result<(), Box<dyn std::error::Error>> {
    // Try to load environment variables or config file
    std::env::var("REDIS_URL").or_else(|_| Ok("redis://127.0.0.1:6379".to_string()))?;
    Ok(())
}

async fn test_redis_connection() -> Result<(), Box<dyn std::error::Error>> {
    use redis::AsyncCommands;
    
    let client = redis::Client::open("redis://127.0.0.1:6379")?;
    let mut conn = client.get_async_connection().await?;
    
    let _: String = redis::cmd("PING").query_async(&mut conn).await?;
    
    // Test basic set/get
    let _: () = conn.set("test_key", "test_value").await?;
    let result: String = conn.get("test_key").await?;
    
    if result != "test_value" {
        return Err("Redis set/get test failed".into());
    }
    
    // Cleanup
    let _: () = conn.del("test_key").await?;
    
    Ok(())
}

async fn test_jupiter_api() -> Result<(), Box<dyn std::error::Error>> {
    use reqwest;
    
    let client = reqwest::Client::new();
    let url = "https://lite-api.jup.ag/price/v2";
    
    let response = client
        .get(url)
        .query(&[("ids", "So11111111111111111111111111111111111111112")])
        .send()
        .await?;
    
    if response.status().is_success() {
        let data: serde_json::Value = response.json().await?;
        println!("Jupiter API response sample: {}", data);
        Ok(())
    } else {
        Err(format!("Jupiter API returned status: {}", response.status()).into())
    }
}