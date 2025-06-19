use config_manager::{ConfigManager, SystemConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config_manager = ConfigManager::new()?;
    let config = config_manager.config();
    
    println!("=== Configuration Analysis ===");
    println!("BirdEye API Key: '{}'", config.birdeye.api_key);
    println!("BirdEye API Base URL: '{}'", config.birdeye.api_base_url);
    println!("Redis URL: '{}'", config.redis.url);
    
    // Test environment variable loading
    println!("\n=== Environment Variables ===");
    if let Ok(birdeye_key) = std::env::var("BIRDEYE_API_KEY") {
        println!("BIRDEYE_API_KEY from env: '{}'", birdeye_key);
    } else {
        println!("BIRDEYE_API_KEY not found in environment");
    }
    
    if let Ok(pnl_birdeye_key) = std::env::var("PNL__BIRDEYE__API_KEY") {
        println!("PNL__BIRDEYE__API_KEY from env: '{}'", pnl_birdeye_key);
    } else {
        println!("PNL__BIRDEYE__API_KEY not found in environment");
    }
    
    // Check if the API key is empty
    if config.birdeye.api_key.is_empty() {
        println!("\n❌ BirdEye API key is EMPTY - this will cause HTTP 401 errors!");
        println!("   The configuration system expects PNL__BIRDEYE__API_KEY environment variable");
        println!("   But the .env file has BIRDEYE_API_KEY");
    } else {
        println!("\n✅ BirdEye API key is configured: {}", config.birdeye.api_key);
    }
    
    Ok(())
}