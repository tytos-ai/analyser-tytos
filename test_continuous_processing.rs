use anyhow::Result;
use config_manager::SystemConfig;
use job_orchestrator::JobOrchestrator;
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🔧 Testing Continuous P&L Processing");

    // Load system config
    let system_config = SystemConfig::load()?;
    
    // Create job orchestrator
    let orchestrator = JobOrchestrator::new(system_config.clone()).await?;

    info!("🎯 Testing single continuous cycle to process queued wallets...");

    // Test if the continuous processing can pick up and process wallets from queue
    match orchestrator.start_continuous_mode_single_cycle().await {
        Ok(processed) => {
            info!("✅ SUCCESS: Continuous cycle completed. Processed a wallet: {}", processed);
            if processed {
                info!("🎉 At least one wallet was successfully processed from the queue!");
            } else {
                info!("⚠️ No wallets were processed (queue might be empty or processing failed)");
            }
        }
        Err(e) => {
            info!("❌ FAILED: Continuous cycle failed: {}", e);
        }
    }

    Ok(())
}