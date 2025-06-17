use jprice_client::{JupiterPriceClient, JupiterClientConfig};
use pnl_core::PriceFetcher;

#[tokio::main]
async fn main() {
    let config = JupiterClientConfig::default();
    let client = JupiterPriceClient::new(config, None).await.unwrap();
    
    let tokens = vec!["AQCq97gywgAvsUVMr1SMYCq1pu541evPEPUcXfG8tWsn".to_string()];
    
    println!("🚀 Testing Jupiter client directly...");
    match client.fetch_prices(&tokens, None).await {
        Ok(prices) => {
            for (token, price) in prices {
                println!("💰 Token {}: price = {}", token, price);
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }
}