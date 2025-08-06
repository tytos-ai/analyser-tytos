use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
use serde_json;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::{PersistenceError, Result, BatchJob, JobStatus};

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
            .max_connections(20)              // Limit per app instance
            .min_connections(5)               // Keep warm connections
            .acquire_timeout(Duration::from_secs(30))  // How long to wait for a connection
            .idle_timeout(Duration::from_secs(600))    // Close idle connections after 10 minutes
            .max_lifetime(Duration::from_secs(1800))   // Force refresh connections after 30 minutes
            .connect(database_url)
            .await
            .map_err(|e| PersistenceError::PoolCreation(format!("PostgreSQL connection error: {}", e)))?;

        info!("PostgreSQL pool initialized: max_connections=20, min_connections=5, acquire_timeout=30s");
        Ok(Self { pool })
    }

    /// Get connection pool metrics for monitoring
    pub fn get_pool_metrics(&self) -> (u32, u32, u32) {
        let size = self.pool.size();
        let idle = self.pool.num_idle();
        // For SQLx 0.6, we'll use a hardcoded max (matching our configuration)
        let max_size = 20u32; // This matches our max_connections setting
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
        // Store with chain field for multichain support
        let portfolio_json = serde_json::to_string(portfolio_result)
            .map_err(PersistenceError::Serialization)?;

        // Extract key metrics for fast queries from rich format
        let total_pnl_usd = portfolio_result.total_pnl_usd.to_string().parse::<f64>().unwrap_or(0.0);
        let realized_pnl_usd = portfolio_result.total_realized_pnl_usd.to_string().parse::<f64>().unwrap_or(0.0);
        let unrealized_pnl_usd = portfolio_result.total_unrealized_pnl_usd.to_string().parse::<f64>().unwrap_or(0.0);
        let total_trades = portfolio_result.total_trades as i32;
        let win_rate = portfolio_result.overall_win_rate_percentage.to_string().parse::<f64>().unwrap_or(0.0);
        let tokens_analyzed = portfolio_result.tokens_analyzed as i32;
        let avg_hold_time = portfolio_result.avg_hold_time_minutes.to_string().parse::<f64>().unwrap_or(0.0);

        // Clear existing data for this wallet and chain
        sqlx::query("DELETE FROM pnl_results WHERE wallet_address = $1 AND chain = $2")
            .bind(wallet_address)
            .bind(chain)
            .execute(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error clearing old data: {}", e)
            ))))?;

        // Insert new rich format data
        sqlx::query(
            r#"
            INSERT INTO pnl_results 
            (wallet_address, chain, total_pnl_usd, realized_pnl_usd, unrealized_pnl_usd, total_trades, win_rate, 
             tokens_analyzed, avg_hold_time_minutes, portfolio_json, analyzed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
        .bind(portfolio_json)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        debug!("Stored rich P&L portfolio result for wallet {} with {} tokens", 
               wallet_address, portfolio_result.tokens_analyzed);
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
            SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived
            FROM pnl_results 
            WHERE wallet_address = $1 AND chain = $2
            "#
        )
        .bind(wallet_address)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        match row {
            Some(row) => {
                let wallet_address: String = row.get("wallet_address");
                let chain: String = row.get("chain");
                let portfolio_json: String = row.get("portfolio_json");
                let analyzed_at: DateTime<Utc> = row.get("analyzed_at");
                let is_favorited: bool = row.get("is_favorited");
                let is_archived: bool = row.get("is_archived");

                let portfolio_result: pnl_core::PortfolioPnLResult = serde_json::from_str(&portfolio_json)
                    .map_err(PersistenceError::Serialization)?;

                let stored_result = crate::StoredPortfolioPnLResult {
                    wallet_address,
                    chain,
                    portfolio_result,
                    analyzed_at,
                    is_favorited,
                    is_archived,
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
            sqlx::query("SELECT COUNT(*) as count FROM pnl_results WHERE chain = $1")
                .bind(chain)
        } else {
            sqlx::query("SELECT COUNT(*) as count FROM pnl_results")
        };
        let count_row = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e)
            ))))?;

        let total_count: i64 = count_row.get("count");

        if total_count == 0 {
            return Ok((Vec::new(), 0));
        }

        // Get paginated results using new schema with optional chain filtering
        let rows = if let Some(chain) = chain_filter {
            sqlx::query(
                r#"
                SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived
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
                SELECT wallet_address, chain, portfolio_json, analyzed_at, is_favorited, is_archived
                FROM pnl_results 
                ORDER BY analyzed_at DESC
                LIMIT $1 OFFSET $2
                "#
            )
            .bind(limit as i64)
            .bind(offset as i64)
        };
        let rows = rows
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error: {}", e)
        ))))?;

        let mut results = Vec::new();
        for row in rows {
            let wallet_address: String = row.get("wallet_address");
            let chain: String = row.get("chain");
            let portfolio_json: String = row.get("portfolio_json");
            let analyzed_at: DateTime<Utc> = row.get("analyzed_at");
            let is_favorited: bool = row.get("is_favorited");
            let is_archived: bool = row.get("is_archived");

            match serde_json::from_str::<pnl_core::PortfolioPnLResult>(&portfolio_json) {
                Ok(portfolio_result) => {
                    let stored_result = crate::StoredPortfolioPnLResult {
                        wallet_address,
                        chain,
                        portfolio_result,
                        analyzed_at,
                        is_favorited,
                        is_archived,
                    };
                    results.push(stored_result);
                }
                Err(e) => {
                    warn!("Failed to deserialize portfolio P&L result for {}: {}", 
                          wallet_address, e);
                }
            }
        }

        debug!("Retrieved {} rich P&L portfolio results (offset: {}, limit: {})", results.len(), offset, limit);
        Ok((results, total_count as usize))
    }

    // =====================================
    // Batch Job Storage
    // =====================================

    /// Store a batch job
    pub async fn store_batch_job(&self, job: &BatchJob) -> Result<()> {
        let wallet_addresses_json = serde_json::to_string(&job.wallet_addresses)
            .map_err(PersistenceError::Serialization)?;
        let filters_json = serde_json::to_string(&job.filters)
            .map_err(PersistenceError::Serialization)?;
        let status_str = format!("{:?}", job.status);

        sqlx::query(
            r#"
            INSERT INTO batch_jobs 
            (id, wallet_addresses, chain, status, created_at, started_at, completed_at, filters_json)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) 
            DO UPDATE SET 
                wallet_addresses = EXCLUDED.wallet_addresses,
                chain = EXCLUDED.chain,
                status = EXCLUDED.status,
                created_at = EXCLUDED.created_at,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                filters_json = EXCLUDED.filters_json
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
            SELECT id, wallet_addresses, chain, status, created_at, started_at, completed_at, filters_json
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

                let wallet_addresses: Vec<String> = serde_json::from_str(&wallet_addresses_json)
                    .map_err(PersistenceError::Serialization)?;
                let filters: serde_json::Value = serde_json::from_str(&filters_json)
                    .map_err(PersistenceError::Serialization)?;

                let status = match status_str.as_str() {
                    "Pending" => JobStatus::Pending,
                    "Running" => JobStatus::Running,
                    "Completed" => JobStatus::Completed,
                    "Failed" => JobStatus::Failed,
                    "Cancelled" => JobStatus::Cancelled,
                    _ => JobStatus::Failed,
                };

                let job = BatchJob {
                    id: Uuid::parse_str(&id)
                        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("UUID parsing error: {}", e)
                        ))))?,
                    wallet_addresses,
                    chain,
                    status,
                    created_at,
                    started_at,
                    completed_at,
                    filters,
                    individual_jobs: Vec::new(), // Will be populated from batch_results if needed
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
    // Health and Utility
    // =====================================

    /// Test PostgreSQL connectivity
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1 as test")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL health check failed: {}", e)
            ))))?;

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

        debug!("Stored legacy P&L result for wallet {} token {}", wallet_address, token_symbol);
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<(usize, usize)> {
        let pnl_count = sqlx::query("SELECT COUNT(*) as count FROM pnl_results")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e)
            ))))?;
        let pnl_count: i64 = pnl_count.get("count");

        let batch_count = sqlx::query("SELECT COUNT(*) as count FROM batch_jobs")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("PostgreSQL error: {}", e)
            ))))?;
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
            "#
        )
        .bind(is_favorited)
        .bind(wallet_address)
        .bind(chain)
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error updating favorite status: {}", e)
        ))))?;

        debug!("Updated favorite status for wallet {} on chain {} to {}", 
               wallet_address, chain, is_favorited);
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
            "#
        )
        .bind(is_archived)
        .bind(wallet_address)
        .bind(chain)
        .execute(&self.pool)
        .await
        .map_err(|e| PersistenceError::Connection(redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("PostgreSQL error updating archive status: {}", e)
        ))))?;

        debug!("Updated archive status for wallet {} on chain {} to {}", 
               wallet_address, chain, is_archived);
        Ok(())
    }
}