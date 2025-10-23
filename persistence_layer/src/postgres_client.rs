use chrono::{DateTime, Utc};
use serde_json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::time::Duration;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    BatchJob, JobStatus, PersistenceError, Result, TokenAnalysisJob, TokenAnalysisJobStats,
};

/// PostgreSQL client for persistent storage of P&L results and batch jobs
#[derive(Debug, Clone)]
pub struct PostgresClient {
    pool: PgPool,
}

impl PostgresClient {
    /// Create a new PostgreSQL client with production-grade connection pool settings
    pub async fn new(database_url: &str) -> Result<Self> {
        // Configure connection pool with production settings
        let pool = PgPoolOptions::new()
            .max_connections(100) // Increased from 20 for better concurrency
            .min_connections(20) // Increased from 5 for better warm pool
            .acquire_timeout(Duration::from_secs(30)) // How long to wait for a connection
            .idle_timeout(Duration::from_secs(600)) // Close idle connections after 10 minutes
            .max_lifetime(Duration::from_secs(1800)) // Force refresh connections after 30 minutes
            .connect(database_url)
            .await
            .map_err(|e| {
                PersistenceError::PoolCreation(format!("PostgreSQL connection error: {}", e))
            })?;

        info!("PostgreSQL pool initialized: max_connections=100, min_connections=20, acquire_timeout=30s");
        Ok(Self { pool })
    }

    /// Get connection pool metrics for monitoring
    pub fn get_pool_metrics(&self) -> (u32, u32, u32) {
        let size = self.pool.size();
        let idle = self.pool.num_idle();
        // For SQLx 0.6, we'll use a hardcoded max (matching our configuration)
        let max_size = 100u32; // Updated from 20u32 to match new max_connections
        (size, idle as u32, max_size)
    }

    // =====================================
    // P&L Results Storage
    // =====================================

    /// Store a P&L result for a wallet (rich PortfolioPnLResult format)
    pub async fn store_pnl_result(
        &self,
        wallet_address: &str,
        chain: &str,
        portfolio_result: &pnl_core::PortfolioPnLResult,
    ) -> Result<()> {
        self.store_pnl_result_with_source(wallet_address, chain, portfolio_result, "continuous", 0)
            .await
    }

    /// Store a P&L result for a wallet with specific analysis source
    pub async fn store_pnl_result_with_source(
        &self,
        wallet_address: &str,
        chain: &str,
        portfolio_result: &pnl_core::PortfolioPnLResult,
        analysis_source: &str,
        incomplete_trades_count: u32,
    ) -> Result<()> {
        // Store with chain field for multichain support
        let portfolio_json =
            serde_json::to_string(portfolio_result).map_err(PersistenceError::Serialization)?;

        // Extract key metrics for fast queries from rich format
        let total_pnl_usd = portfolio_result
            .total_pnl_usd
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        let realized_pnl_usd = portfolio_result
            .total_realized_pnl_usd
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        let unrealized_pnl_usd = portfolio_result
            .total_unrealized_pnl_usd
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        let total_trades = portfolio_result.total_trades as i32;
        let win_rate = portfolio_result
            .overall_win_rate_percentage
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        let tokens_analyzed = portfolio_result.tokens_analyzed as i32;
        let avg_hold_time = portfolio_result
            .avg_hold_time_minutes
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);

        // Calculate advanced filtering metrics
        let unique_tokens_count = portfolio_result.token_results.len() as i32;

        // Calculate active days from all trades
        let mut trading_days = std::collections::HashSet::new();
        for token_result in &portfolio_result.token_results {
            for trade in &token_result.matched_trades {
                let trade_date = trade.sell_event.timestamp.date_naive();
                trading_days.insert(trade_date);
            }
        }
        let active_days_count = trading_days.len() as i32;

        // Extract profit_percentage (ROI) from the portfolio result
        // This is already calculated by the PnL engine
        let roi_percentage = portfolio_result.profit_percentage.to_string().parse::<f64>().unwrap_or(0.0);

        // Clear existing data for this wallet and chain
        sqlx::query("DELETE FROM pnl_results WHERE wallet_address = $1 AND chain = $2")
            .bind(wallet_address)
            .bind(chain)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error clearing old data: {}", e),
                )))
            })?;

        // Insert new rich format data
        sqlx::query(
            r#"
            INSERT INTO pnl_results
            (wallet_address, chain, total_pnl_usd, realized_pnl_usd, unrealized_pnl_usd, total_trades, win_rate,
             tokens_analyzed, avg_hold_time_minutes, unique_tokens_count, active_days_count, roi_percentage, portfolio_json, analyzed_at, analysis_source, incomplete_trades_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            "#
        )
        .bind(wallet_address)
        .bind(chain)
        .bind(total_pnl_usd)
        .bind(realized_pnl_usd)
        .bind(unrealized_pnl_usd)
        .bind(total_trades)
        .bind(win_rate)
        .bind(tokens_analyzed)
        .bind(avg_hold_time)
        .bind(unique_tokens_count)
        .bind(active_days_count)
        .bind(roi_percentage)
        .bind(portfolio_json)
        .bind(Utc::now())
        .bind(analysis_source)
        .bind(incomplete_trades_count as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        debug!(
            "Stored rich P&L portfolio result for wallet {} with {} tokens",
            wallet_address, portfolio_result.tokens_analyzed
        );
        Ok(())
    }

    /// Get a rich P&L portfolio result for a specific wallet and chain
    pub async fn get_portfolio_pnl_result(
        &self,
        wallet_address: &str,
        chain: &str,
    ) -> Result<Option<crate::StoredPortfolioPnLResult>> {
        let row = sqlx::query(
            r#"
            SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived,
                   unique_tokens_count, active_days_count, incomplete_trades_count
            FROM pnl_results
            WHERE wallet_address = $1 AND chain = $2
            "#,
        )
        .bind(wallet_address)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        match row {
            Some(row) => {
                let wallet_address: String = row.get("wallet_address");
                let chain: String = row.get("chain");
                let portfolio_json: String = row.get("portfolio_json");
                let analyzed_at: DateTime<Utc> = row.get("analyzed_at");
                let is_favorited: bool = row.get("is_favorited");
                let is_archived: bool = row.get("is_archived");
                let unique_tokens_count: Option<i32> = row.get("unique_tokens_count");
                let active_days_count: Option<i32> = row.get("active_days_count");
                let incomplete_trades_count: Option<i32> = row.get("incomplete_trades_count");

                let portfolio_result: pnl_core::PortfolioPnLResult =
                    serde_json::from_str(&portfolio_json)
                        .map_err(PersistenceError::Serialization)?;

                let stored_result = crate::StoredPortfolioPnLResult {
                    wallet_address,
                    chain,
                    portfolio_result,
                    analyzed_at,
                    is_favorited,
                    is_archived,
                    unique_tokens_count: unique_tokens_count.map(|v| v as u32),
                    active_days_count: active_days_count.map(|v| v as u32),
                    incomplete_trades_count: incomplete_trades_count.map(|v| v as u32).unwrap_or(0),
                };

                Ok(Some(stored_result))
            }
            None => Ok(None),
        }
    }

    /// Legacy method - deprecated (for backward compatibility)
    pub async fn get_pnl_result(
        &self,
        _wallet_address: &str,
        _token_address: &str,
    ) -> Result<Option<crate::StoredPnLResult>> {
        // This method is deprecated and will return None for new rich format data
        // since we no longer store per-token results separately
        warn!("Using deprecated get_pnl_result method - use get_portfolio_pnl_result instead");
        Ok(None)
    }

    /// Get all rich P&L portfolio results with pagination and optional chain filtering
    pub async fn get_all_pnl_results(
        &self,
        offset: usize,
        limit: usize,
        chain_filter: Option<&str>,
    ) -> Result<(Vec<crate::StoredPortfolioPnLResult>, usize)> {
        // Get total count with optional chain filtering
        let count_query = if let Some(chain) = chain_filter {
            sqlx::query("SELECT COUNT(*) as count FROM pnl_results WHERE chain = $1").bind(chain)
        } else {
            sqlx::query("SELECT COUNT(*) as count FROM pnl_results")
        };
        let count_row = count_query.fetch_one(&self.pool).await.map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        let total_count: i64 = count_row.get("count");

        if total_count == 0 {
            return Ok((Vec::new(), 0));
        }

        // Get paginated results using new schema with optional chain filtering
        let rows = if let Some(chain) = chain_filter {
            sqlx::query(
                r#"
                SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived,
                       unique_tokens_count, active_days_count, incomplete_trades_count
                FROM pnl_results
                WHERE chain = $1
                ORDER BY analyzed_at DESC
                LIMIT $2 OFFSET $3
                "#
            )
            .bind(chain)
            .bind(limit as i64)
            .bind(offset as i64)
        } else {
            sqlx::query(
                r#"
                SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived,
                       unique_tokens_count, active_days_count, incomplete_trades_count
                FROM pnl_results
                ORDER BY analyzed_at DESC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit as i64)
            .bind(offset as i64)
        };
        let rows = rows.fetch_all(&self.pool).await.map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let wallet_address: String = row.get("wallet_address");
            let chain: String = row.get("chain");
            let portfolio_json: String = row.get("portfolio_json");
            let analyzed_at: DateTime<Utc> = row.get("analyzed_at");
            let is_favorited: bool = row.get("is_favorited");
            let is_archived: bool = row.get("is_archived");
            let unique_tokens_count: Option<i32> = row.get("unique_tokens_count");
            let active_days_count: Option<i32> = row.get("active_days_count");
            let incomplete_trades_count: Option<i32> = row.get("incomplete_trades_count");

            match serde_json::from_str::<pnl_core::PortfolioPnLResult>(&portfolio_json) {
                Ok(portfolio_result) => {
                    let stored_result = crate::StoredPortfolioPnLResult {
                        wallet_address,
                        chain,
                        portfolio_result,
                        analyzed_at,
                        is_favorited,
                        is_archived,
                        unique_tokens_count: unique_tokens_count.map(|v| v as u32),
                        active_days_count: active_days_count.map(|v| v as u32),
                        incomplete_trades_count: incomplete_trades_count.map(|v| v as u32).unwrap_or(0),
                    };
                    results.push(stored_result);
                }
                Err(e) => {
                    warn!(
                        "Failed to deserialize portfolio P&L result for {}: {}",
                        wallet_address, e
                    );
                }
            }
        }

        debug!(
            "Retrieved {} rich P&L portfolio results (offset: {}, limit: {})",
            results.len(),
            offset,
            limit
        );
        Ok((results, total_count as usize))
    }

    /// Get all P&L portfolio results as lightweight summaries (NO portfolio_json deserialization)
    /// This is a memory-optimized version for listing/filtering operations
    pub async fn get_all_pnl_results_summary(
        &self,
        offset: usize,
        limit: usize,
        chain_filter: Option<&str>,
    ) -> Result<(Vec<crate::StoredPortfolioPnLResultSummary>, usize)> {
        // Get total count with optional chain filtering
        let count_query = if let Some(chain) = chain_filter {
            sqlx::query("SELECT COUNT(*) as count FROM pnl_results WHERE chain = $1").bind(chain)
        } else {
            sqlx::query("SELECT COUNT(*) as count FROM pnl_results")
        };
        let count_row = count_query.fetch_one(&self.pool).await.map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        let total_count: i64 = count_row.get("count");

        if total_count == 0 {
            return Ok((Vec::new(), 0));
        }

        // Get paginated results - ONLY summary columns, NO portfolio_json
        // Cast NUMERIC columns to FLOAT8 to avoid type mismatch with Rust f64
        let rows = if let Some(chain) = chain_filter {
            sqlx::query(
                r#"
                SELECT wallet_address, chain,
                       total_pnl_usd::float8 as total_pnl_usd,
                       realized_pnl_usd::float8 as realized_pnl_usd,
                       unrealized_pnl_usd::float8 as unrealized_pnl_usd,
                       roi_percentage::float8 as roi_percentage,
                       total_trades,
                       win_rate::float8 as win_rate,
                       avg_hold_time_minutes::float8 as avg_hold_time_minutes,
                       unique_tokens_count, active_days_count, analyzed_at, is_favorited, is_archived,
                       incomplete_trades_count
                FROM pnl_results
                WHERE chain = $1
                ORDER BY analyzed_at DESC
                LIMIT $2 OFFSET $3
                "#
            )
            .bind(chain)
            .bind(limit as i64)
            .bind(offset as i64)
        } else {
            sqlx::query(
                r#"
                SELECT wallet_address, chain,
                       total_pnl_usd::float8 as total_pnl_usd,
                       realized_pnl_usd::float8 as realized_pnl_usd,
                       unrealized_pnl_usd::float8 as unrealized_pnl_usd,
                       roi_percentage::float8 as roi_percentage,
                       total_trades,
                       win_rate::float8 as win_rate,
                       avg_hold_time_minutes::float8 as avg_hold_time_minutes,
                       unique_tokens_count, active_days_count, analyzed_at, is_favorited, is_archived,
                       incomplete_trades_count
                FROM pnl_results
                ORDER BY analyzed_at DESC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit as i64)
            .bind(offset as i64)
        };
        let rows = rows.fetch_all(&self.pool).await.map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        // Parse rows directly from database columns - NO JSON deserialization!
        let mut results = Vec::new();
        for row in rows {
            let result = crate::StoredPortfolioPnLResultSummary {
                wallet_address: row.get("wallet_address"),
                chain: row.get("chain"),
                total_pnl_usd: row.get("total_pnl_usd"),
                realized_pnl_usd: row.get("realized_pnl_usd"),
                unrealized_pnl_usd: row.get("unrealized_pnl_usd"),
                roi_percentage: row.get("roi_percentage"),
                total_trades: row.get("total_trades"),
                win_rate: row.get("win_rate"),
                avg_hold_time_minutes: row.get("avg_hold_time_minutes"),
                unique_tokens_count: row.get::<Option<i32>, _>("unique_tokens_count").map(|v| v as u32),
                active_days_count: row.get::<Option<i32>, _>("active_days_count").map(|v| v as u32),
                analyzed_at: row.get("analyzed_at"),
                is_favorited: row.get("is_favorited"),
                is_archived: row.get("is_archived"),
                incomplete_trades_count: row.get::<Option<i32>, _>("incomplete_trades_count").map(|v| v as u32).unwrap_or(0),
            };
            results.push(result);
        }

        info!(
            "Retrieved {} P&L summary results (offset: {}, limit: {}) - NO portfolio_json loaded",
            results.len(),
            offset,
            limit
        );
        Ok((results, total_count as usize))
    }

    // =====================================
    // Batch Job Storage
    // =====================================

    /// Store a batch job
    pub async fn store_batch_job(&self, job: &BatchJob) -> Result<()> {
        let wallet_addresses_json = serde_json::to_string(&job.wallet_addresses)
            .map_err(PersistenceError::Serialization)?;
        let filters_json =
            serde_json::to_string(&job.filters).map_err(PersistenceError::Serialization)?;
        let status_str = format!("{:?}", job.status);
        let successful_wallets_json = serde_json::to_string(&job.successful_wallets)
            .map_err(PersistenceError::Serialization)?;
        let failed_wallets_json = serde_json::to_string(&job.failed_wallets)
            .map_err(PersistenceError::Serialization)?;

        sqlx::query(
            r#"
            INSERT INTO batch_jobs
            (id, wallet_addresses, chain, status, created_at, started_at, completed_at, filters_json,
             successful_wallets, failed_wallets, error_summary)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id)
            DO UPDATE SET
                wallet_addresses = EXCLUDED.wallet_addresses,
                chain = EXCLUDED.chain,
                status = EXCLUDED.status,
                created_at = EXCLUDED.created_at,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                filters_json = EXCLUDED.filters_json,
                successful_wallets = EXCLUDED.successful_wallets,
                failed_wallets = EXCLUDED.failed_wallets,
                error_summary = EXCLUDED.error_summary
            "#
        )
        .bind(job.id.to_string())
        .bind(wallet_addresses_json)
        .bind(&job.chain)
        .bind(status_str)
        .bind(job.created_at)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(filters_json)
        .bind(successful_wallets_json)
        .bind(failed_wallets_json)
        .bind(&job.error_summary)
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        debug!("Stored batch job {}", job.id);
        Ok(())
    }

    /// Get a batch job by ID
    pub async fn get_batch_job(&self, job_id: &str) -> Result<Option<BatchJob>> {
        let row = sqlx::query(
            r#"
            SELECT id, wallet_addresses, chain, status, created_at, started_at, completed_at, filters_json,
                   successful_wallets, failed_wallets, error_summary
            FROM batch_jobs
            WHERE id = $1
            "#
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        match row {
            Some(row) => {
                let id: String = row.get("id");
                let wallet_addresses_json: String = row.get("wallet_addresses");
                let chain: String = row.get("chain");
                let status_str: String = row.get("status");
                let created_at: DateTime<Utc> = row.get("created_at");
                let started_at: Option<DateTime<Utc>> = row.get("started_at");
                let completed_at: Option<DateTime<Utc>> = row.get("completed_at");
                let filters_json: String = row.get("filters_json");
                // New fields with backward compatibility - may be NULL in old records
                let successful_wallets_json: Option<String> = row.try_get("successful_wallets").ok();
                let failed_wallets_json: Option<String> = row.try_get("failed_wallets").ok();
                let error_summary: Option<String> = row.try_get("error_summary").ok().flatten();

                let wallet_addresses: Vec<String> = serde_json::from_str(&wallet_addresses_json)
                    .map_err(PersistenceError::Serialization)?;
                let filters: serde_json::Value =
                    serde_json::from_str(&filters_json).map_err(PersistenceError::Serialization)?;

                // Backward compatibility: default to empty Vec if field is NULL
                let successful_wallets: Vec<String> = successful_wallets_json
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default();
                let failed_wallets: Vec<String> = failed_wallets_json
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default();

                let status = match status_str.as_str() {
                    "Pending" => JobStatus::Pending,
                    "Running" => JobStatus::Running,
                    "Completed" => JobStatus::Completed,
                    "Failed" => JobStatus::Failed,
                    "Cancelled" => JobStatus::Cancelled,
                    _ => JobStatus::Failed,
                };

                let job = BatchJob {
                    id: Uuid::parse_str(&id).map_err(|e| {
                        PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("UUID parsing error: {}", e),
                        )))
                    })?,
                    wallet_addresses,
                    chain,
                    status,
                    created_at,
                    started_at,
                    completed_at,
                    filters,
                    individual_jobs: Vec::new(), // Will be populated from batch_results if needed
                    successful_wallets,
                    failed_wallets,
                    error_summary,
                };

                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    // Note: store_batch_job_results method removed - batch results are now stored
    // directly in pnl_results table, eliminating duplicate storage

    // Note: get_batch_job_results method removed - batch results are now fetched
    // directly from pnl_results table using wallet addresses from batch job

    // =====================================
    // Token Analysis Job Storage
    // =====================================

    /// Store a token analysis job
    pub async fn store_token_analysis_job(&self, job: &TokenAnalysisJob) -> Result<()> {
        let token_addresses_json =
            serde_json::to_string(&job.token_addresses).map_err(PersistenceError::Serialization)?;
        let filters_json =
            serde_json::to_string(&job.filters).map_err(PersistenceError::Serialization)?;
        let discovered_wallets_json = serde_json::to_string(&job.discovered_wallets)
            .map_err(PersistenceError::Serialization)?;
        let analyzed_wallets_json = serde_json::to_string(&job.analyzed_wallets)
            .map_err(PersistenceError::Serialization)?;
        let failed_wallets_json =
            serde_json::to_string(&job.failed_wallets).map_err(PersistenceError::Serialization)?;
        let status_str = job.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO token_analysis_jobs 
            (id, token_addresses, chain, status, created_at, started_at, completed_at, filters_json, discovered_wallets, analyzed_wallets, failed_wallets)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) 
            DO UPDATE SET 
                token_addresses = EXCLUDED.token_addresses,
                chain = EXCLUDED.chain,
                status = EXCLUDED.status,
                created_at = EXCLUDED.created_at,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                filters_json = EXCLUDED.filters_json,
                discovered_wallets = EXCLUDED.discovered_wallets,
                analyzed_wallets = EXCLUDED.analyzed_wallets,
                failed_wallets = EXCLUDED.failed_wallets
            "#
        )
        .bind(job.id.to_string())
        .bind(token_addresses_json)
        .bind(&job.chain)
        .bind(status_str)
        .bind(job.created_at)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(filters_json)
        .bind(discovered_wallets_json)
        .bind(analyzed_wallets_json)
        .bind(failed_wallets_json)
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        debug!("Stored token analysis job {}", job.id);
        Ok(())
    }

    /// Update a token analysis job
    pub async fn update_token_analysis_job(&self, job: &TokenAnalysisJob) -> Result<()> {
        self.store_token_analysis_job(job).await
    }

    /// Get a token analysis job by ID
    pub async fn get_token_analysis_job(&self, job_id: &str) -> Result<Option<TokenAnalysisJob>> {
        let row = sqlx::query(
            r#"
            SELECT id, token_addresses, chain, status, created_at, started_at, completed_at, 
                   filters_json, discovered_wallets, analyzed_wallets, failed_wallets
            FROM token_analysis_jobs 
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        match row {
            Some(row) => {
                let token_addresses: Vec<String> = serde_json::from_str(row.get("token_addresses"))
                    .map_err(PersistenceError::Serialization)?;
                let filters: serde_json::Value = serde_json::from_str(row.get("filters_json"))
                    .map_err(PersistenceError::Serialization)?;
                let discovered_wallets: Vec<String> =
                    serde_json::from_str(row.get("discovered_wallets"))
                        .map_err(PersistenceError::Serialization)?;
                let analyzed_wallets: Vec<String> =
                    serde_json::from_str(row.get("analyzed_wallets"))
                        .map_err(PersistenceError::Serialization)?;
                let failed_wallets: Vec<String> = serde_json::from_str(row.get("failed_wallets"))
                    .map_err(PersistenceError::Serialization)?;

                // Parse status string back to enum
                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "Pending" => JobStatus::Pending,
                    "Running" => JobStatus::Running,
                    "Completed" => JobStatus::Completed,
                    "Failed" => JobStatus::Failed,
                    "Cancelled" => JobStatus::Cancelled,
                    _ => JobStatus::Failed, // Default to Failed for unknown status
                };

                let id_str: String = row.get("id");
                let id = uuid::Uuid::parse_str(&id_str).map_err(|e| {
                    PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid UUID: {}", e),
                    )))
                })?;

                let job = TokenAnalysisJob {
                    id,
                    token_addresses,
                    chain: row.get("chain"),
                    status,
                    created_at: row.get("created_at"),
                    started_at: row.get("started_at"),
                    completed_at: row.get("completed_at"),
                    filters,
                    discovered_wallets,
                    analyzed_wallets,
                    failed_wallets,
                };

                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Get all token analysis jobs with pagination
    pub async fn get_all_token_analysis_jobs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<TokenAnalysisJob>, usize)> {
        // Get total count
        let count_row = sqlx::query("SELECT COUNT(*) as count FROM token_analysis_jobs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error: {}", e),
                )))
            })?;
        let total_count: i64 = count_row.get("count");

        // Get jobs with pagination, ordered by created_at DESC (newest first)
        let rows = sqlx::query(
            r#"
            SELECT id, token_addresses, chain, status, created_at, started_at, completed_at, 
                   filters_json, discovered_wallets, analyzed_wallets, failed_wallets
            FROM token_analysis_jobs 
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            let token_addresses: Vec<String> = serde_json::from_str(row.get("token_addresses"))
                .map_err(PersistenceError::Serialization)?;
            let filters: serde_json::Value = serde_json::from_str(row.get("filters_json"))
                .map_err(PersistenceError::Serialization)?;
            let discovered_wallets: Vec<String> =
                serde_json::from_str(row.get("discovered_wallets"))
                    .map_err(PersistenceError::Serialization)?;
            let analyzed_wallets: Vec<String> = serde_json::from_str(row.get("analyzed_wallets"))
                .map_err(PersistenceError::Serialization)?;
            let failed_wallets: Vec<String> = serde_json::from_str(row.get("failed_wallets"))
                .map_err(PersistenceError::Serialization)?;

            // Parse status string back to enum
            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "Pending" => JobStatus::Pending,
                "Running" => JobStatus::Running,
                "Completed" => JobStatus::Completed,
                "Failed" => JobStatus::Failed,
                "Cancelled" => JobStatus::Cancelled,
                _ => JobStatus::Failed, // Default to Failed for unknown status
            };

            let id_str: String = row.get("id");
            let id = uuid::Uuid::parse_str(&id_str).map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid UUID: {}", e),
                )))
            })?;

            let job = TokenAnalysisJob {
                id,
                token_addresses,
                chain: row.get("chain"),
                status,
                created_at: row.get("created_at"),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                filters,
                discovered_wallets,
                analyzed_wallets,
                failed_wallets,
            };

            jobs.push(job);
        }

        debug!(
            "Retrieved {} token analysis jobs (offset: {}, limit: {})",
            jobs.len(),
            offset,
            limit
        );
        Ok((jobs, total_count as usize))
    }

    /// Get token analysis job statistics
    pub async fn get_token_analysis_job_stats(&self) -> Result<TokenAnalysisJobStats> {
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_jobs,
                COUNT(CASE WHEN status = 'Running' THEN 1 END) as running_jobs,
                COUNT(CASE WHEN status = 'Completed' THEN 1 END) as completed_jobs,
                COUNT(CASE WHEN status = 'Failed' THEN 1 END) as failed_jobs,
                COUNT(CASE WHEN status = 'Pending' THEN 1 END) as pending_jobs,
                COUNT(CASE WHEN status = 'Cancelled' THEN 1 END) as cancelled_jobs
            FROM token_analysis_jobs
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e),
            )))
        })?;

        Ok(TokenAnalysisJobStats {
            total_jobs: row.get::<i64, _>("total_jobs") as u64,
            running_jobs: row.get::<i64, _>("running_jobs") as u64,
            completed_jobs: row.get::<i64, _>("completed_jobs") as u64,
            failed_jobs: row.get::<i64, _>("failed_jobs") as u64,
            pending_jobs: row.get::<i64, _>("pending_jobs") as u64,
            cancelled_jobs: row.get::<i64, _>("cancelled_jobs") as u64,
        })
    }

    // =====================================
    // Health and Utility
    // =====================================

    /// Test PostgreSQL connectivity
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1 as test")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL health check failed: {}", e),
                )))
            })?;

        Ok(())
    }

    /// Store legacy P&L result (for migration purposes)
    pub async fn store_legacy_pnl_result(
        &self,
        wallet_address: &str,
        token_address: &str,
        token_symbol: &str,
        total_pnl_usd: f64,
        realized_pnl_usd: f64,
        total_trades: i32,
        win_rate: f64,
        report_json: &str,
        analyzed_at: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO pnl_results 
            (wallet_address, token_address, token_symbol, total_pnl_usd, realized_pnl_usd, total_trades, win_rate, report_json, analyzed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (wallet_address) 
            DO UPDATE SET 
                token_address = EXCLUDED.token_address,
                token_symbol = EXCLUDED.token_symbol,
                total_pnl_usd = EXCLUDED.total_pnl_usd,
                realized_pnl_usd = EXCLUDED.realized_pnl_usd,
                total_trades = EXCLUDED.total_trades,
                win_rate = EXCLUDED.win_rate,
                report_json = EXCLUDED.report_json,
                analyzed_at = EXCLUDED.analyzed_at
            "#
        )
        .bind(wallet_address)
        .bind(token_address)
        .bind(token_symbol)
        .bind(total_pnl_usd)
        .bind(realized_pnl_usd)
        .bind(total_trades)
        .bind(win_rate)
        .bind(report_json)
        .bind(analyzed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        debug!(
            "Stored legacy P&L result for wallet {} token {}",
            wallet_address, token_symbol
        );
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<(usize, usize)> {
        let pnl_count = sqlx::query("SELECT COUNT(*) as count FROM pnl_results")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error: {}", e),
                )))
            })?;
        let pnl_count: i64 = pnl_count.get("count");

        let batch_count = sqlx::query("SELECT COUNT(*) as count FROM batch_jobs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error: {}", e),
                )))
            })?;
        let batch_count: i64 = batch_count.get("count");

        Ok((pnl_count as usize, batch_count as usize))
    }

    /// Update favorite status for a wallet
    pub async fn update_wallet_favorite_status(
        &self,
        wallet_address: &str,
        chain: &str,
        is_favorited: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE pnl_results 
            SET is_favorited = $1
            WHERE wallet_address = $2 AND chain = $3
            "#,
        )
        .bind(is_favorited)
        .bind(wallet_address)
        .bind(chain)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error updating favorite status: {}", e),
            )))
        })?;

        debug!(
            "Updated favorite status for wallet {} on chain {} to {}",
            wallet_address, chain, is_favorited
        );
        Ok(())
    }

    /// Update archive status for a wallet
    pub async fn update_wallet_archive_status(
        &self,
        wallet_address: &str,
        chain: &str,
        is_archived: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE pnl_results 
            SET is_archived = $1
            WHERE wallet_address = $2 AND chain = $3
            "#,
        )
        .bind(is_archived)
        .bind(wallet_address)
        .bind(chain)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error updating archive status: {}", e),
            )))
        })?;

        debug!(
            "Updated archive status for wallet {} on chain {} to {}",
            wallet_address, chain, is_archived
        );
        Ok(())
    }

    /// Apply database migrations for advanced filtering features
    pub async fn apply_advanced_filtering_migration(&self) -> Result<()> {
        // Add new columns if they don't exist (for existing installations)
        sqlx::query("ALTER TABLE pnl_results ADD COLUMN IF NOT EXISTS unique_tokens_count INTEGER DEFAULT 0")
            .execute(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error adding unique_tokens_count column: {}", e)
            ))))?;

        sqlx::query(
            "ALTER TABLE pnl_results ADD COLUMN IF NOT EXISTS active_days_count INTEGER DEFAULT 0",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error adding active_days_count column: {}", e),
            )))
        })?;

        // Create indexes for performance
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_pnl_results_unique_tokens ON pnl_results(unique_tokens_count)")
            .execute(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error creating unique_tokens_count index: {}", e)
            ))))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_pnl_results_active_days ON pnl_results(active_days_count)")
            .execute(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error creating active_days_count index: {}", e)
            ))))?;

        info!("Applied advanced filtering migration: added unique_tokens_count and active_days_count columns with indexes");
        Ok(())
    }

    /// Backfill metrics for existing P&L results that have zero values
    pub async fn backfill_advanced_filtering_metrics(&self) -> Result<()> {
        use sqlx::Row;
        use futures::stream::StreamExt;

        info!("Starting backfill of advanced filtering metrics for existing records");

        // Use streaming fetch() instead of fetch_all() to avoid holding connection
        // for the entire result set - releases connection between row fetches
        let mut row_stream = sqlx::query(
            r#"
            SELECT wallet_address, chain, portfolio_json
            FROM pnl_results
            WHERE (unique_tokens_count = 0 OR active_days_count = 0) AND portfolio_json IS NOT NULL
            "#,
        )
        .fetch(&self.pool);

        let mut updated_count = 0;

        // Process rows one at a time using streaming fetch
        while let Some(row_result) = row_stream.next().await {
            let row = row_result.map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error fetching row: {}", e),
                )))
            })?;
            let wallet_address: String = row.get("wallet_address");
            let chain: String = row.get("chain");
            let portfolio_json_str: String = row.get("portfolio_json");
            let portfolio_json: serde_json::Value = serde_json::from_str(&portfolio_json_str)
                .map_err(|e| {
                    PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("JSON parsing error: {}", e),
                    )))
                })?;

            // Parse the portfolio result to calculate metrics
            match serde_json::from_value::<pnl_core::PortfolioPnLResult>(portfolio_json) {
                Ok(portfolio_result) => {
                    // Calculate metrics from portfolio data
                    let unique_tokens_count = portfolio_result.token_results.len() as i32;

                    let mut trading_days = std::collections::HashSet::new();
                    for token_result in &portfolio_result.token_results {
                        for trade in &token_result.matched_trades {
                            let trade_date = trade.sell_event.timestamp.date_naive();
                            trading_days.insert(trade_date);
                        }
                    }
                    let active_days_count = trading_days.len() as i32;

                    // Update the record with calculated metrics
                    sqlx::query(
                        r#"
                        UPDATE pnl_results 
                        SET unique_tokens_count = $1, active_days_count = $2
                        WHERE wallet_address = $3 AND chain = $4
                        "#,
                    )
                    .bind(unique_tokens_count)
                    .bind(active_days_count)
                    .bind(&wallet_address)
                    .bind(&chain)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("PostgreSQL error updating metrics: {}", e),
                        )))
                    })?;

                    updated_count += 1;

                    if updated_count % 100 == 0 {
                        info!("Backfilled metrics for {} records so far...", updated_count);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to parse portfolio JSON for wallet {} on {}: {}",
                        wallet_address, chain, e
                    );
                }
            }
        }

        info!(
            "Backfill completed: updated metrics for {} existing records",
            updated_count
        );
        Ok(())
    }

    /// Get all P&L results with advanced filtering support
    pub async fn get_all_pnl_results_with_filters(
        &self,
        offset: usize,
        limit: usize,
        chain_filter: Option<&str>,
        min_unique_tokens: Option<u32>,
        min_active_days: Option<u32>,
        analysis_source_filter: Option<&str>,
    ) -> Result<(Vec<crate::StoredPortfolioPnLResult>, usize)> {
        // Build WHERE clauses
        let mut where_clauses = Vec::new();
        let mut bind_params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Postgres> + Send + Sync>> =
            Vec::new();
        let mut param_count = 0;

        if let Some(chain) = chain_filter {
            param_count += 1;
            where_clauses.push(format!("chain = ${}", param_count));
            bind_params.push(Box::new(chain.to_string()));
        }

        if let Some(min_tokens) = min_unique_tokens {
            param_count += 1;
            where_clauses.push(format!("unique_tokens_count >= ${}", param_count));
            bind_params.push(Box::new(min_tokens as i32));
        }

        if let Some(min_days) = min_active_days {
            param_count += 1;
            where_clauses.push(format!("active_days_count >= ${}", param_count));
            bind_params.push(Box::new(min_days as i32));
        }

        if let Some(analysis_source) = analysis_source_filter {
            param_count += 1;
            where_clauses.push(format!("analysis_source = ${}", param_count));
            bind_params.push(Box::new(analysis_source.to_string()));
        }

        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // Get total count with filtering
        let count_query = format!("SELECT COUNT(*) as count FROM pnl_results {}", where_clause);
        let mut count_query_builder = sqlx::query(&count_query);

        // Bind parameters for count query
        if let Some(chain) = chain_filter {
            count_query_builder = count_query_builder.bind(chain);
        }
        if let Some(min_tokens) = min_unique_tokens {
            count_query_builder = count_query_builder.bind(min_tokens as i32);
        }
        if let Some(min_days) = min_active_days {
            count_query_builder = count_query_builder.bind(min_days as i32);
        }
        if let Some(analysis_source) = analysis_source_filter {
            count_query_builder = count_query_builder.bind(analysis_source);
        }

        let count_row = count_query_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error: {}", e),
                )))
            })?;

        let total_count: i64 = count_row.get("count");

        if total_count == 0 {
            return Ok((Vec::new(), 0));
        }

        // Get paginated results with filtering
        let results_query = format!(
            r#"
            SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived,
                   unique_tokens_count, active_days_count, incomplete_trades_count
            FROM pnl_results
            {}
            ORDER BY analyzed_at DESC
            LIMIT ${} OFFSET ${}
            "#,
            where_clause,
            param_count + 1,
            param_count + 2
        );

        let mut results_query_builder = sqlx::query(&results_query);

        // Bind parameters for results query
        if let Some(chain) = chain_filter {
            results_query_builder = results_query_builder.bind(chain);
        }
        if let Some(min_tokens) = min_unique_tokens {
            results_query_builder = results_query_builder.bind(min_tokens as i32);
        }
        if let Some(min_days) = min_active_days {
            results_query_builder = results_query_builder.bind(min_days as i32);
        }
        if let Some(analysis_source) = analysis_source_filter {
            results_query_builder = results_query_builder.bind(analysis_source);
        }

        // Add limit and offset
        results_query_builder = results_query_builder.bind(limit as i64).bind(offset as i64);

        let rows = results_query_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("PostgreSQL error: {}", e),
                )))
            })?;

        let mut results = Vec::new();
        for row in rows {
            let wallet_address: String = row.get("wallet_address");
            let chain: String = row.get("chain");
            let portfolio_json_str: String = row.get("portfolio_json");
            let analyzed_at: chrono::DateTime<chrono::Utc> = row.get("analyzed_at");
            let is_favorited: Option<bool> = row.try_get("is_favorited").ok();
            let is_archived: Option<bool> = row.try_get("is_archived").ok();
            let unique_tokens_count: Option<i32> = row.get("unique_tokens_count");
            let active_days_count: Option<i32> = row.get("active_days_count");
            let incomplete_trades_count: Option<i32> = row.get("incomplete_trades_count");

            // Parse the portfolio JSON to get the rich result (same pattern as working functions)
            match serde_json::from_str::<pnl_core::PortfolioPnLResult>(&portfolio_json_str) {
                Ok(portfolio_result) => {
                    let stored_result = crate::StoredPortfolioPnLResult {
                        wallet_address,
                        chain,
                        portfolio_result,
                        analyzed_at,
                        is_favorited: is_favorited.unwrap_or(false),
                        is_archived: is_archived.unwrap_or(false),
                        unique_tokens_count: unique_tokens_count.map(|v| v as u32),
                        active_days_count: active_days_count.map(|v| v as u32),
                        incomplete_trades_count: incomplete_trades_count.map(|v| v as u32).unwrap_or(0),
                    };
                    results.push(stored_result);
                }
                Err(e) => {
                    warn!(
                        "Failed to parse portfolio JSON for wallet {}: {}",
                        wallet_address, e
                    );
                    continue;
                }
            }
        }

        info!(
            "Retrieved {} P&L results with advanced filtering (total: {})",
            results.len(),
            total_count
        );
        Ok((results, total_count as usize))
    }

    /// Get P&L results filtered by analysis source
    pub async fn get_pnl_results_by_analysis_source(
        &self,
        analysis_source: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<crate::StoredPortfolioPnLResult>, usize)> {
        self.get_all_pnl_results_with_filters(
            offset,
            limit,
            None, // chain_filter
            None, // min_unique_tokens
            None, // min_active_days
            Some(analysis_source),
        )
        .await
    }
}
