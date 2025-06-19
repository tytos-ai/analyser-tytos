use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
    // Check environment variables
    println!("=== Environment Variables ===");
    if let Ok(birdeye_key) = env::var("BIRDEYE_API_KEY") {
        println!("BIRDEYE_API_KEY: '{}'", birdeye_key);
    } else {
        println!("BIRDEYE_API_KEY: not found");
    }
    
    if let Ok(pnl_birdeye_key) = env::var("PNL__BIRDEYE__API_KEY") {
        println!("PNL__BIRDEYE__API_KEY: '{}'", pnl_birdeye_key);
    } else {
        println!("PNL__BIRDEYE__API_KEY: not found");
    }
    
    // Test a simple HTTP request with the API key
    let api_key = env::var("PNL__BIRDEYE__API_KEY")
        .unwrap_or_else(|_| "".to_string());
    
    if api_key.is_empty() {
        println!("\n❌ API key is empty - authentication will fail");
        return Ok(());
    }
    
    println!("\n✅ API key found: {}", api_key);
    
    // Make a simple HTTP request to test authentication
    let client = reqwest::Client::new();
    let response = client
        .get("https://public-api.birdeye.so/defi/token_trending")
        .header("X-API-KEY", &api_key)
        .query(&[("chain", "solana")])
        .send()
        .await?;
    
    println!("HTTP Response Status: {}", response.status());
    
    if response.status().is_success() {
        println!("✅ BirdEye API authentication successful!");
        let response_text = response.text().await?;
        println!("Response length: {} characters", response_text.len());
    } else {
        println!("❌ BirdEye API authentication failed!");
        let error_text = response.text().await?;
        println!("Error response: {}", error_text);
    }
    
    Ok(())
}