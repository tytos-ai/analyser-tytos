use anyhow::Result;
use config_manager::SystemConfig;
use job_orchestrator::JobOrchestrator;
use persistence_layer::DiscoveredWalletToken;
use pnl_core::PnLFilters;
use rust_decimal::Decimal;
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("🔧 Testing Wallet-Token P&L Analysis (Complete Trading History)");

    // Load system config
    let system_config = SystemConfig::load()?;
    
    // Create job orchestrator
    let orchestrator = JobOrchestrator::new(system_config.clone()).await?;

    // Create a discovered wallet-token pair like the discovery service would
    let wallet_token_pair = DiscoveredWalletToken {
        wallet_address: "HV1KXxWFaSeriyFvXyx48FqG9BoFbfinB8njCJonqP7K".to_string(), // Top trader we found
        token_address: "71Jvq4Epe2FCJ7JFSF7jLXdNk1Wy4Bhqd9iL6bEFELvg".to_string(), // GOR token that led to discovery
        token_symbol: "GOR".to_string(),
        trader_volume_usd: 63692932.0,
        trader_trades: 3434,
        discovered_at: chrono::Utc::now(),
    };

    // Create P&L filters
    let filters = PnLFilters {
        min_capital_sol: Decimal::from_f64_retain(0.1).unwrap(),
        min_hold_minutes: Decimal::from_f64_retain(0.1).unwrap(),
        min_trades: 1,
        min_win_rate: Decimal::ZERO,
        max_signatures: Some(100),
        timeframe_filter: None,
    };

    info!("🎯 Testing complete trading history P&L analysis for discovered trader...");
    info!("   Wallet: {}", wallet_token_pair.wallet_address);
    info!("   Discovered via token: {} ({})", wallet_token_pair.token_symbol, wallet_token_pair.token_address);
    info!("   Volume: ${:.0}", wallet_token_pair.trader_volume_usd);
    info!("   Will analyze COMPLETE trading history (all tokens)");

    // Test the targeted P&L analysis that the continuous mode uses
    match orchestrator.process_single_wallet_token_pair(&wallet_token_pair, filters.clone()).await {
        Ok(report) => {
            info!("✅ SUCCESS: Complete trading history P&L analysis completed!");
            info!("📊 P&L Report Summary:");
            info!("   Total P&L: ${}", report.summary.total_pnl_usd);
            info!("   Realized P&L: ${}", report.summary.realized_pnl_usd);
            info!("   Total Trades: {}", report.summary.total_trades);
            info!("   Win Rate: {}%", report.summary.win_rate);
            info!("   Tokens Traded: {}", report.token_breakdown.len());
        }
        Err(e) => {
            info!("❌ FAILED: Complete trading history P&L analysis failed: {}", e);
            info!("🔍 This explains why the continuous mode isn't working!");
            
            // Let's also test the general wallet analysis to compare
            info!("🔄 Testing general wallet P&L analysis for comparison...");
            match orchestrator.process_single_wallet(&wallet_token_pair.wallet_address, filters).await {
                Ok(report) => {
                    info!("✅ General wallet analysis works: P&L = ${}", report.summary.total_pnl_usd);
                    info!("   This means the issue is specific to the wallet-token pair processing method");
                }
                Err(e) => {
                    info!("❌ General wallet analysis also fails: {}", e);
                    info!("   This indicates a broader issue with BirdEye API or transaction processing");
                }
            }
            
            return Err(e.into());
        }
    }

    Ok(())
}