use dex_client::{BirdEyeClient, BirdEyeConfig};
use job_orchestrator::JobOrchestrator;
use config_manager::SystemConfig;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    println!("Testing transaction conversion pipeline...");
    
    // Load config
    let config = SystemConfig::load_from_path("config.toml")?;
    
    // Create orchestrator
    let orchestrator = JobOrchestrator::new(config).await?;
    
    // Test wallet with known transactions
    let wallet = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa";
    
    // Step 1: Fetch transactions (we know this works)
    println!("Step 1: Fetching transactions...");
    let birdeye_config = BirdEyeConfig {
        api_key: "5ff313b239ac42e297b830b10ea1871d".to_string(),
        api_base_url: "https://public-api.birdeye.so".to_string(),
        request_timeout_seconds: 30,
        rate_limit_per_second: 100,
    };
    
    let client = BirdEyeClient::new(birdeye_config)?;
    let transactions = client.get_all_trader_transactions(wallet, None, None, Some(5)).await?;
    println!("✅ Fetched {} transactions", transactions.len());
    
    // Step 2: Test conversion to financial events
    println!("Step 2: Converting to financial events...");
    match orchestrator.convert_general_birdeye_transactions_to_events(&transactions, wallet) {
        Ok(events) => {
            println!("✅ Converted to {} financial events", events.len());
            if !events.is_empty() {
                println!("First event: {:?}", events[0]);
            }
            
            // Step 3: Test P&L calculation
            println!("Step 3: Testing P&L calculation...");
            use pnl_core::{PnLFilters, calculate_pnl_with_embedded_prices};
            use rust_decimal::Decimal;
            
            let filters = PnLFilters {
                min_capital_sol: Decimal::ZERO,
                min_hold_minutes: Decimal::ZERO,
                min_trades: 0,
                min_win_rate: Decimal::ZERO,
                max_signatures: None,
                timeframe_filter: None,
            };
            
            match calculate_pnl_with_embedded_prices(wallet, events, filters).await {
                Ok(report) => {
                    println!("✅ P&L calculation successful!");
                    println!("Total P&L: {} USD", report.summary.total_pnl_usd);
                }
                Err(e) => {
                    println!("❌ P&L calculation failed: {}", e);
                }
            }
            
        }
        Err(e) => {
            println!("❌ Conversion failed: {}", e);
        }
    }
    
    Ok(())
}