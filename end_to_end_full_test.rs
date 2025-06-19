use anyhow::Result;
use chrono::Utc;
use config_manager::SystemConfig;
use csv::Writer;
use job_orchestrator::{JobOrchestrator, birdeye_trending_orchestrator::{BirdEyeTrendingOrchestrator, BirdEyeTrendingConfig}};
use dex_client::TopTraderFilter;
use persistence_layer::RedisClient;
use std::io::Cursor;
use tokio::time::{sleep, Duration};
use tracing::{info, warn, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("debug,end_to_end_full_test=info")
        .init();

    info!("🚀 Starting End-to-End Full System Test");
    info!("📋 Test Flow:");
    info!("   1. Clear Redis data");
    info!("   2. Discover trending tokens via BirdEye");
    info!("   3. Extract top traders for discovered tokens");
    info!("   4. Queue wallet-token pairs for P&L analysis");
    info!("   5. Process P&L analysis");
    info!("   6. Export results to CSV");

    // Load configuration
    let config = SystemConfig::load()?;
    info!("✅ Configuration loaded");

    // Initialize components
    let redis_client = RedisClient::new(&config.redis.url).await?;
    let orchestrator = JobOrchestrator::new(config.clone()).await?;
    
    // Create BirdEye trending config
    let trending_config = BirdEyeTrendingConfig {
        api_key: config.birdeye.api_key.clone(),
        api_base_url: config.birdeye.api_base_url.clone(),
        chain: "solana".to_string(),
        top_trader_filter: TopTraderFilter {
            min_volume_usd: 5000.0,
            min_trades: 3,
            min_win_rate: Some(50.0),
            max_last_trade_hours: Some(24),
            max_traders: Some(5),
        },
        max_trending_tokens: 10,
        max_traders_per_token: 5,
        cycle_interval_seconds: 300,
        debug_mode: true,
    };
    
    let trending_orchestrator = BirdEyeTrendingOrchestrator::new(trending_config, Some(redis_client.clone()))?;
    info!("✅ Components initialized");

    // Step 1: Clear Redis data
    info!("🧹 Step 1: Clearing Redis data...");
    redis_client.clear_temp_data().await?;
    info!("✅ Redis data cleared");

    // Step 2: Discover trending tokens
    info!("🔍 Step 2: Discovering trending tokens...");
    let _wallet_count = trending_orchestrator.execute_discovery_cycle().await?;
    let discovery_stats = trending_orchestrator.get_discovery_stats().await?;
    info!("✅ Discovery completed: {} tokens discovered, {} wallet-token pairs found", 
          discovery_stats.tokens_discovered, discovery_stats.wallet_token_pairs_discovered);

    if discovery_stats.wallet_token_pairs_discovered == 0 {
        warn!("⚠️  No wallet-token pairs discovered. Test cannot continue.");
        return Ok(());
    }

    // Step 3: Wait a bit and check queue
    info!("⏳ Step 3: Checking discovery queue...");
    sleep(Duration::from_secs(2)).await;
    
    let queue_size = redis_client.get_wallet_token_pairs_queue_size().await?;
    info!("📊 Queue contains {} wallet-token pairs ready for analysis", queue_size);

    if queue_size == 0 {
        warn!("⚠️  Queue is empty. Discovery may have failed.");
        return Ok(());
    }

    // Step 4: Process P&L analysis for discovered pairs
    info!("💰 Step 4: Processing P&L analysis for discovered wallet-token pairs...");
    let max_pairs_to_process = std::cmp::min(queue_size, 10); // Limit to 10 for testing
    let mut processed_pairs = 0;
    let mut successful_analyses = 0;

    for i in 0..max_pairs_to_process {
        info!("🔄 Processing pair {}/{}", i + 1, max_pairs_to_process);
        
        // Run a single continuous cycle (processes one wallet-token pair)
        match orchestrator.start_continuous_mode_single_cycle().await {
            Ok(processed) => {
                if processed {
                    processed_pairs += 1;
                    successful_analyses += 1;
                    info!("✅ Successfully processed wallet-token pair {}", i + 1);
                } else {
                    info!("⏭️  No more pairs in queue");
                    break;
                }
            }
            Err(e) => {
                error!("❌ Failed to process wallet-token pair {}: {}", i + 1, e);
                processed_pairs += 1;
            }
        }

        // Small delay between processing
        sleep(Duration::from_millis(500)).await;
    }

    info!("📈 P&L Analysis Summary:");
    info!("   • Total pairs processed: {}", processed_pairs);
    info!("   • Successful analyses: {}", successful_analyses);

    if successful_analyses == 0 {
        warn!("⚠️  No successful P&L analyses. Cannot export results.");
        return Ok(());
    }

    // Step 5: Fetch all results from Redis
    info!("📊 Step 5: Fetching P&L results from storage...");
    let (stored_results, total_count) = redis_client.get_all_pnl_results(0, 100).await?;
    info!("✅ Retrieved {} P&L results from storage", stored_results.len());

    if stored_results.is_empty() {
        warn!("⚠️  No stored results found. Export cannot proceed.");
        return Ok(());
    }

    // Step 6: Export to CSV
    info!("📄 Step 6: Exporting results to CSV...");
    let csv_content = generate_results_csv(&stored_results)?;
    let filename = format!("end_to_end_test_results_{}.csv", Utc::now().format("%Y%m%d_%H%M%S"));
    
    std::fs::write(&filename, csv_content)?;
    info!("✅ Results exported to: {}", filename);

    // Step 7: Display summary
    info!("🎉 End-to-End Test Completed Successfully!");
    info!("📋 Final Summary:");
    info!("   • Trending tokens discovered: {}", discovery_stats.tokens_discovered);
    info!("   • Wallet-token pairs found: {}", discovery_stats.wallet_token_pairs_discovered);
    info!("   • Pairs processed for P&L: {}", processed_pairs);
    info!("   • Successful P&L analyses: {}", successful_analyses);
    info!("   • Results stored in Redis: {}", stored_results.len());
    info!("   • CSV file created: {}", filename);

    // Display top results
    info!("🏆 Top P&L Results:");
    let mut sorted_results = stored_results;
    sorted_results.sort_by(|a, b| b.pnl_report.summary.total_pnl_usd.cmp(&a.pnl_report.summary.total_pnl_usd));
    
    for (i, result) in sorted_results.iter().take(5).enumerate() {
        info!("   {}. Wallet: {}...{} | Token: {} | P&L: ${:.2} | Trades: {} | Win Rate: {:.1}%",
              i + 1,
              &result.wallet_address[..8],
              &result.wallet_address[result.wallet_address.len()-8..],
              result.token_symbol,
              result.pnl_report.summary.total_pnl_usd,
              result.pnl_report.summary.total_trades,
              result.pnl_report.summary.win_rate * rust_decimal::Decimal::from(100)
        );
    }

    Ok(())
}

fn generate_results_csv(results: &[persistence_layer::StoredPnLResult]) -> Result<String> {
    let mut wtr = Writer::from_writer(Cursor::new(Vec::new()));

    // Write CSV headers
    wtr.write_record(&[
        "wallet_address",
        "token_address", 
        "token_symbol",
        "total_pnl_usd",
        "realized_pnl_usd",
        "unrealized_pnl_usd",
        "roi_percentage",
        "total_trades",
        "winning_trades",
        "losing_trades",
        "win_rate_percentage",
        "total_capital_deployed_sol",
        "total_fees_usd",
        "first_trade_time",
        "last_trade_time",
        "analysis_timeframe_start",
        "analysis_timeframe_end",
        "analyzed_at",
        "discovery_source",
        "trader_volume_usd",
        "trader_trade_count"
    ])?;

    // Write data rows
    for result in results {
        let report = &result.pnl_report;
        let summary = &report.summary;
        
        // Calculate win rate percentage
        let win_rate_pct = summary.win_rate * rust_decimal::Decimal::from(100);
        
        // Format timestamps
        let first_trade = report.token_breakdown.iter()
            .filter_map(|t| t.first_buy_time.as_ref())
            .min()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_default();
        let last_trade = report.token_breakdown.iter()
            .filter_map(|t| t.last_sell_time.as_ref())
            .max()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_default();
        
        let timeframe_start = report.timeframe.start_time.as_ref()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_default();
        let timeframe_end = report.timeframe.end_time.as_ref()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_default();

        wtr.write_record(&[
            &result.wallet_address,
            &result.token_address,
            &result.token_symbol,
            &summary.total_pnl_usd.to_string(),
            &summary.realized_pnl_usd.to_string(),
            &summary.unrealized_pnl_usd.to_string(),
            &summary.roi_percentage.to_string(),
            &summary.total_trades.to_string(),
            &summary.winning_trades.to_string(),
            &summary.losing_trades.to_string(),
            &format!("{:.2}", win_rate_pct),
            &summary.total_capital_deployed_sol.to_string(),
            &summary.total_fees_usd.to_string(),
            &first_trade,
            &last_trade,
            &timeframe_start,
            &timeframe_end,
            &result.analyzed_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            "BirdEye_TopTraders", // Discovery source
            "N/A", // Trader volume USD (not stored currently)
            "N/A", // Trader trade count (not stored currently)
        ])?;
    }

    let data = wtr.into_inner()?.into_inner();
    Ok(String::from_utf8(data)?)
}