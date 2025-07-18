use config_manager::HeliusConfig;
use pnl_core::{TransactionRecord, TokenChangeRecord, FinancialEvent, EventType, EventMetadata, PriceFetcher};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::collections::HashMap;
use thiserror::Error;
use tokio::time;
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;
use crate::token_metadata_service::{TokenMetadataService, TokenMetadataError};
use crate::PriceFetchingService;
use pnl_core::GeneralTraderTransaction;

#[derive(Error, Debug)]
pub enum HeliusError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("JSON parsing failed: {0}")]
    JsonParsingFailed(#[from] serde_json::Error),
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Rate limit exceeded, retry after: {retry_after_ms}ms")]
    RateLimitExceeded { retry_after_ms: u64 },
    #[error("Invalid wallet address: {0}")]
    InvalidWalletAddress(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("Token metadata error: {0}")]
    TokenMetadata(#[from] TokenMetadataError),
    #[error("Price fetching error: {0}")]
    PriceFetching(String),
}

pub type Result<T> = std::result::Result<T, HeliusError>;

/// Helius API client for fetching enhanced transaction data
#[derive(Debug, Clone)]
pub struct HeliusClient {
    /// HTTP client for making requests
    http_client: Client,
    
    /// Helius API configuration
    config: HeliusConfig,
    
    /// Token metadata service for fetching token names and symbols
    token_metadata_service: Option<TokenMetadataService>,
    
    /// Price fetching service for historical prices
    price_fetching_service: Option<PriceFetchingService>,
}

impl HeliusClient {
    /// Create a new Helius client with the given configuration
    pub fn new(config: HeliusConfig) -> Result<Self> {
        // Validate configuration
        if config.enabled && config.api_key.is_empty() {
            return Err(HeliusError::ConfigError(
                "Helius API key is required when Helius is enabled".to_string(),
            ));
        }

        // Create HTTP client with timeout
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .user_agent("wallet-analyzer/1.0")
            .build()
            .map_err(|e| HeliusError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            http_client,
            config,
            token_metadata_service: None,
            price_fetching_service: None,
        })
    }

    /// Get decimals for a token mint (with fallback to common values)
    fn get_token_decimals(&self, mint: &str) -> u8 {
        match mint {
            "So11111111111111111111111111111111111111112" => 9,  // SOL
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => 6,  // USDC
            "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" => 6,  // USDT
            "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263" => 5,  // BONK
            "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs" => 8,  // ETHER
            "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So" => 9,   // mSOL
            _ => {
                // Try to get from token metadata service if available
                if let Some(ref _service) = self.token_metadata_service {
                    // This would be async, so for now use a reasonable default
                    debug!("Using default decimals for unknown token: {}", mint);
                }
                6 // Default to 6 decimals (most common)
            }
        }
    }

    /// Check if Helius client is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Set the token metadata service for fetching token names and symbols
    pub fn with_token_metadata_service(mut self, service: TokenMetadataService) -> Self {
        self.token_metadata_service = Some(service);
        self
    }

    /// Set the price fetching service for historical prices
    pub fn with_price_fetching_service(mut self, service: PriceFetchingService) -> Self {
        self.price_fetching_service = Some(service);
        self
    }

    /// Fetch current prices for a list of token mints
    pub async fn fetch_token_prices(&self, mints: Vec<String>) -> Result<HashMap<String, Decimal>> {
        if let Some(ref price_service) = self.price_fetching_service {
            match price_service.fetch_prices(&mints, None).await {
                Ok(prices) => {
                    debug!("Successfully fetched {} token prices", prices.len());
                    Ok(prices)
                }
                Err(e) => {
                    error!("Failed to fetch token prices: {}", e);
                    Err(HeliusError::ConfigError(format!("Price fetching failed: {}", e)))
                }
            }
        } else {
            warn!("No price fetching service configured");
            Ok(HashMap::new())
        }
    }

    /// Fetch historical price for a single token at a specific timestamp
    pub async fn fetch_historical_price(&self, mint: &str, timestamp: i64) -> Result<Option<Decimal>> {
        if let Some(ref price_service) = self.price_fetching_service {
            // Convert timestamp to DateTime<Utc>
            let datetime = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);
            
            match price_service.fetch_historical_price(mint, datetime, None).await {
                Ok(price_opt) => {
                    if let Some(price) = price_opt {
                        debug!("Successfully fetched historical price for {} at timestamp {}: {}", mint, timestamp, price);
                        Ok(Some(price))
                    } else {
                        debug!("No historical price found for {} at timestamp {}", mint, timestamp);
                        Ok(None)
                    }
                }
                Err(e) => {
                    error!("Failed to fetch historical price for {}: {}", mint, e);
                    Err(HeliusError::ConfigError(format!("Historical price fetching failed: {}", e)))
                }
            }
        } else {
            warn!("No price fetching service configured");
            Ok(None)
        }
    }

    /// Make a rate-limited HTTP request to Helius API
    async fn make_request(&self, request_builder: RequestBuilder) -> Result<reqwest::Response> {
        let mut attempt = 0;
        let max_attempts = self.config.max_retry_attempts;

        loop {
            attempt += 1;
            
            // Clone the request for retry attempts
            let request = request_builder
                .try_clone()
                .ok_or_else(|| HeliusError::ConfigError("Failed to clone request".to_string()))?
                .build()
                .map_err(HeliusError::RequestFailed)?;

            debug!("Making Helius API request (attempt {}/{}): {}", 
                   attempt, max_attempts, request.url());

            // Make the request
            let response = self.http_client.execute(request).await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    
                    if status.is_success() {
                        debug!("Helius API request successful");
                        return Ok(resp);
                    } else if status.as_u16() == 429 {
                        // Rate limit exceeded
                        let retry_after_ms = self.config.rate_limit_ms * 2; // Exponential backoff
                        warn!("Helius rate limit exceeded, retrying after {}ms", retry_after_ms);
                        
                        if attempt >= max_attempts {
                            return Err(HeliusError::RateLimitExceeded { retry_after_ms });
                        }
                        
                        time::sleep(Duration::from_millis(retry_after_ms)).await;
                        continue;
                    } else {
                        // Other HTTP error
                        let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        error!("Helius API error: {} - {}", status, error_text);
                        
                        if attempt >= max_attempts {
                            return Err(HeliusError::ApiError {
                                status: status.as_u16(),
                                message: error_text,
                            });
                        }
                        
                        // Wait before retry
                        let delay_ms = self.config.rate_limit_ms * attempt as u64;
                        time::sleep(Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                }
                Err(e) => {
                    error!("Helius API request failed: {}", e);
                    
                    if attempt >= max_attempts {
                        return Err(HeliusError::RequestFailed(e));
                    }
                    
                    // Wait before retry
                    let delay_ms = self.config.rate_limit_ms * attempt as u64;
                    time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
            }
        }
    }

    /// Apply rate limiting between requests
    async fn apply_rate_limit(&self) {
        if self.config.rate_limit_ms > 0 {
            time::sleep(Duration::from_millis(self.config.rate_limit_ms)).await;
        }
    }

    /// Validate wallet address format
    fn validate_wallet_address(&self, wallet_address: &str) -> Result<()> {
        if wallet_address.is_empty() {
            return Err(HeliusError::InvalidWalletAddress("Wallet address cannot be empty".to_string()));
        }
        
        // Basic Solana address validation (44 characters, base58)
        if wallet_address.len() < 32 || wallet_address.len() > 44 {
            return Err(HeliusError::InvalidWalletAddress(
                format!("Invalid wallet address length: {}", wallet_address.len())
            ));
        }
        
        Ok(())
    }

    /// Build the Helius API URL for fetching wallet transactions
    fn build_transactions_url(&self, wallet_address: &str, before: Option<&str>, limit: u32) -> String {
        let mut url = format!(
            "{}/addresses/{}/transactions?api-key={}&type=SWAP&limit={}",
            self.config.api_base_url,
            wallet_address,
            self.config.api_key,
            limit
        );

        if let Some(before_signature) = before {
            url.push_str(&format!("&before={}", before_signature));
        }

        url
    }

    /// Fetch wallet transactions with pagination support
    /// This is the main public method for getting swap transactions for a wallet
    pub async fn fetch_wallet_transactions(
        &self,
        wallet_address: &str,
        transaction_count: Option<u32>,
        timeframe: Option<(i64, i64)>, // (start_timestamp, end_timestamp)
    ) -> Result<Vec<HeliusTransaction>> {
        // Validate inputs
        self.validate_wallet_address(wallet_address)?;

        if !self.is_enabled() {
            return Err(HeliusError::ConfigError(
                "Helius client is disabled in configuration".to_string()
            ));
        }

        debug!("Fetching transactions for wallet: {}, count: {:?}, timeframe: {:?}", 
               wallet_address, transaction_count, timeframe);

        let mut all_transactions = Vec::new();
        let mut before_signature: Option<String> = None;
        let batch_size = 100; // Helius max per request
        let max_batches = transaction_count
            .map(|count| count.div_ceil(batch_size)) // Ceiling division
            .unwrap_or(u32::MAX); // Unlimited if no count specified

        let mut batch_count = 0;

        loop {
            // Check if we've hit the batch limit
            if batch_count >= max_batches {
                debug!("Reached maximum batch limit: {}", max_batches);
                break;
            }

            // Determine how many transactions to request in this batch
            let current_batch_size = if let Some(total_count) = transaction_count {
                let remaining = total_count.saturating_sub(all_transactions.len() as u32);
                if remaining == 0 {
                    break;
                }
                remaining.min(batch_size)
            } else {
                batch_size
            };

            debug!("Fetching batch {} with size {}", batch_count + 1, current_batch_size);

            // Fetch the batch
            let batch = self.fetch_transaction_batch(
                wallet_address,
                before_signature.as_deref(),
                current_batch_size,
            ).await?;

            if batch.is_empty() {
                debug!("No more transactions available");
                break;
            }

            debug!("Received {} transactions in batch", batch.len());

            // Apply timeframe filtering if specified
            let mut filtered_batch = Vec::new();
            for tx in batch {
                if let Some((start_time, end_time)) = timeframe {
                    if tx.timestamp < start_time || tx.timestamp > end_time {
                        // If we've gone past the timeframe, we can stop
                        if tx.timestamp < start_time {
                            debug!("Reached transaction older than timeframe, stopping");
                            if !filtered_batch.is_empty() {
                                all_transactions.extend(filtered_batch);
                            }
                            return Ok(all_transactions);
                        }
                        continue; // Skip transactions outside timeframe
                    }
                }
                filtered_batch.push(tx);
            }

            // Update before_signature for next batch (use last transaction in original batch)
            if let Some(last_tx) = all_transactions.last().or(filtered_batch.last()) {
                before_signature = Some(last_tx.signature.clone());
            }

            all_transactions.extend(filtered_batch);
            batch_count += 1;

            // Check if we've reached the requested count
            if let Some(total_count) = transaction_count {
                if all_transactions.len() >= total_count as usize {
                    all_transactions.truncate(total_count as usize);
                    break;
                }
            }

            // Apply rate limiting between batches
            self.apply_rate_limit().await;
        }

        debug!("Fetched {} total transactions for wallet {}", 
               all_transactions.len(), wallet_address);

        Ok(all_transactions)
    }

    /// Fetch a single batch of transactions from Helius API
    async fn fetch_transaction_batch(
        &self,
        wallet_address: &str,
        before: Option<&str>,
        limit: u32,
    ) -> Result<Vec<HeliusTransaction>> {
        let url = self.build_transactions_url(wallet_address, before, limit);
        
        debug!("Making request to: {}", url);

        let request = self.http_client.get(&url);
        let response = self.make_request(request).await?;

        let transactions: Vec<HeliusTransaction> = response
            .json()
            .await
            .map_err(|e| {
                error!("Failed to parse Helius response as JSON: {}", e);
                HeliusError::RequestFailed(e)
            })?;

        Ok(transactions)
    }

    /// Extract token balance changes for a specific wallet from Helius transactions
    /// This method processes both account_data and tokenTransfers for comprehensive extraction
    pub fn extract_token_balance_changes(
        &self,
        transactions: &[HeliusTransaction],
        wallet_address: &str,
    ) -> Vec<TokenChange> {
        let mut token_changes = Vec::new();

        for transaction in transactions {
            debug!("Processing transaction: {} for wallet: {}", 
                   transaction.signature, wallet_address);

            // Method 1: Extract from tokenTransfers (direct transfers)
            let transfer_changes = self.extract_from_token_transfers(transaction, wallet_address);
            for change in transfer_changes {
                debug!("Extracted token transfer: mint={}, amount={}, operation={:?}", 
                       change.mint, change.ui_amount, change.operation);
                token_changes.push(change);
            }

            // Method 2: Extract from accountData (balance changes)
            let balance_changes = self.extract_from_account_data(transaction, wallet_address);
            for change in balance_changes {
                debug!("Extracted balance change: mint={}, amount={}, operation={:?}", 
                       change.mint, change.ui_amount, change.operation);
                token_changes.push(change);
            }
        }

        // Remove duplicates by signature + mint + amount
        self.deduplicate_token_changes(token_changes)
    }

    /// Extract token changes from tokenTransfers array with price information
    fn extract_from_token_transfers(
        &self,
        transaction: &HeliusTransaction,
        wallet_address: &str,
    ) -> Vec<TokenChange> {
        let mut token_changes = Vec::new();

        for token_transfer in &transaction.token_transfers {
            let mut change: Option<TokenChange> = None;

            // Check if wallet is sending tokens (fromUserAccount)
            if token_transfer.from_user_account == wallet_address {
                // Wallet is sending tokens - this is a SELL
                let decimals = self.get_token_decimals(&token_transfer.mint);
                change = Some(TokenChange {
                    mint: token_transfer.mint.clone(),
                    raw_amount: -(token_transfer.token_amount * 10_f64.powi(decimals as i32)) as i64, // Make negative for sell
                    ui_amount: -token_transfer.token_amount, // Negative for sell
                    decimals,
                    operation: TokenOperation::Sell,
                    transaction_signature: transaction.signature.clone(),
                    timestamp: transaction.timestamp,
                    source: transaction.source.clone(),
                });
            }
            // Check if wallet is receiving tokens (toUserAccount)
            else if token_transfer.to_user_account == wallet_address {
                // Wallet is receiving tokens - this is a BUY
                let decimals = self.get_token_decimals(&token_transfer.mint);
                change = Some(TokenChange {
                    mint: token_transfer.mint.clone(),
                    raw_amount: (token_transfer.token_amount * 10_f64.powi(decimals as i32)) as i64, // Make positive for buy
                    ui_amount: token_transfer.token_amount, // Positive for buy
                    decimals,
                    operation: TokenOperation::Buy,
                    transaction_signature: transaction.signature.clone(),
                    timestamp: transaction.timestamp,
                    source: transaction.source.clone(),
                });
            }

            if let Some(token_change) = change {
                debug!("Found token transfer: {:?} {} {} tokens", 
                       token_change.operation, token_change.ui_amount, token_change.mint);
                token_changes.push(token_change);
            }
        }

        token_changes
    }

    /// Extract token changes with price information
    pub async fn extract_token_balance_changes_with_prices(
        &self,
        transactions: &[HeliusTransaction],
        wallet_address: &str,
    ) -> Result<Vec<TokenChangeWithPrice>> {
        // First extract all token changes
        let token_changes = self.extract_token_balance_changes(transactions, wallet_address);
        
        // Collect unique mints and timestamps for price fetching
        let mut mint_timestamps: HashMap<String, Vec<i64>> = HashMap::new();
        for change in &token_changes {
            mint_timestamps.entry(change.mint.clone())
                .or_default()
                .push(change.timestamp);
        }
        
        // Fetch prices for all unique mints (using current prices as fallback)
        let mut all_prices: HashMap<String, Decimal> = HashMap::new();
        let mints: Vec<String> = mint_timestamps.keys().cloned().collect();
        
        if !mints.is_empty() {
            match self.fetch_token_prices(mints).await {
                Ok(prices) => {
                    all_prices.extend(prices);
                    debug!("Fetched current prices for {} tokens", all_prices.len());
                }
                Err(e) => {
                    warn!("Failed to fetch current prices: {}", e);
                }
            }
        }
        
        // Convert token changes to include price information
        let mut changes_with_prices = Vec::new();
        for change in token_changes {
            let price = all_prices.get(&change.mint).cloned().unwrap_or_else(|| {
                debug!("No price available for token: {}", change.mint);
                Decimal::ZERO
            });
            
            let usd_value = if price > Decimal::ZERO {
                price * Decimal::from_f64_retain(change.ui_amount.abs()).unwrap_or(Decimal::ZERO)
            } else {
                Decimal::ZERO
            };
            
            changes_with_prices.push(TokenChangeWithPrice {
                mint: change.mint,
                raw_amount: change.raw_amount,
                ui_amount: change.ui_amount,
                decimals: change.decimals,
                operation: change.operation,
                transaction_signature: change.transaction_signature,
                timestamp: change.timestamp,
                source: change.source,
                price_usd: price,
                usd_value,
            });
        }
        
        debug!("Created {} token changes with price information", changes_with_prices.len());
        Ok(changes_with_prices)
    }

    /// Extract token changes from accountData array (existing logic)
    fn extract_from_account_data(
        &self,
        transaction: &HeliusTransaction,
        wallet_address: &str,
    ) -> Vec<TokenChange> {
        let mut token_changes = Vec::new();

        // Look for account data that matches our wallet
        for account_data in &transaction.account_data {
            // Process token balance changes for any account where our wallet is the user
            for balance_change in &account_data.token_balance_changes {
                // Only process if this balance change is for our wallet
                if balance_change.user_account == wallet_address {
                    match self.parse_token_balance_change(balance_change, transaction) {
                        Ok(token_change) => {
                            debug!("Extracted token balance change: mint={}, amount={}, operation={:?}", 
                                   token_change.mint, token_change.ui_amount, token_change.operation);
                            token_changes.push(token_change);
                        }
                        Err(e) => {
                            warn!("Failed to parse token balance change: {}", e);
                        }
                    }
                }
            }
        }

        // Also check for SOL balance changes on the wallet's account
        if let Some(wallet_account) = transaction.account_data.iter().find(|acc| acc.account == wallet_address) {
            if wallet_account.native_balance_change != 0 {
                let sol_change = TokenChange {
                    mint: "So11111111111111111111111111111111111111112".to_string(), // SOL mint
                    raw_amount: wallet_account.native_balance_change,
                    ui_amount: wallet_account.native_balance_change as f64 / 1_000_000_000.0, // Convert lamports to SOL
                    decimals: 9,
                    operation: if wallet_account.native_balance_change > 0 {
                        TokenOperation::Buy
                    } else {
                        TokenOperation::Sell
                    },
                    transaction_signature: transaction.signature.clone(),
                    timestamp: transaction.timestamp,
                    source: transaction.source.clone(),
                };
                
                debug!("Extracted SOL change: amount={}, operation={:?}", 
                       sol_change.ui_amount, sol_change.operation);
                token_changes.push(sol_change);
            }
        }

        token_changes
    }

    /// Remove duplicate token changes based on signature + mint + amount
    fn deduplicate_token_changes(&self, token_changes: Vec<TokenChange>) -> Vec<TokenChange> {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let mut deduplicated = Vec::new();
        let original_count = token_changes.len();

        for change in token_changes {
            // Create a unique key for deduplication
            let key = format!("{}:{}:{}:{}", 
                change.transaction_signature, 
                change.mint, 
                change.raw_amount,
                change.operation as u8
            );
            
            if !seen.contains(&key) {
                seen.insert(key);
                deduplicated.push(change);
            } else {
                debug!("Filtered duplicate token change: {:?} {} {}", 
                       change.operation, change.ui_amount, change.mint);
            }
        }

        debug!("Deduplicated {} token changes to {} unique changes", 
               original_count, deduplicated.len());

        deduplicated
    }

    /// Convert Helius transactions to TransactionRecord format
    /// This method creates structured records suitable for P&L analysis
    pub fn convert_to_transaction_records(
        &self,
        transactions: &[HeliusTransaction],
        wallet_address: &str,
    ) -> Vec<TransactionRecord> {
        let mut records = Vec::new();

        for transaction in transactions {
            match self.convert_single_transaction(transaction, wallet_address) {
                Ok(record) => records.push(record),
                Err(e) => {
                    warn!("Failed to convert transaction {}: {}", transaction.signature, e);
                    continue;
                }
            }
        }

        debug!("Converted {} Helius transactions to {} TransactionRecords", 
               transactions.len(), records.len());
        records
    }

    /// Fetch wallet transactions and convert to TransactionRecord format
    /// This is the main entry point for getting structured transaction data
    pub async fn fetch_and_convert_transactions(
        &self,
        wallet_address: &str,
        transaction_count: Option<u32>,
        timeframe: Option<(i64, i64)>,
    ) -> Result<Vec<TransactionRecord>> {
        // Fetch raw transactions from Helius
        let transactions = self.fetch_wallet_transactions(
            wallet_address,
            transaction_count,
            timeframe,
        ).await?;

        // Convert to structured records
        let records = self.convert_to_transaction_records(&transactions, wallet_address);

        info!("Fetched and converted {} transactions to {} transaction records for wallet {}", 
              transactions.len(), records.len(), wallet_address);

        Ok(records)
    }

    /// Convert a single Helius transaction to TransactionRecord
    fn convert_single_transaction(
        &self,
        transaction: &HeliusTransaction,
        wallet_address: &str,
    ) -> Result<TransactionRecord> {
        let mut token_changes = Vec::new();
        let mut sol_balance_change = 0i64;

        // Extract token changes from both tokenTransfers and accountData
        let extracted_changes = self.extract_token_balance_changes(&[transaction.clone()], wallet_address);
        
        // Convert TokenChange to TokenChangeRecord
        for change in extracted_changes {
            // Skip SOL changes for now, handle them separately
            if change.mint == "So11111111111111111111111111111111111111112" {
                sol_balance_change = change.raw_amount;
                continue;
            }
            
            let token_change_record = TokenChangeRecord {
                mint: change.mint,
                raw_amount: change.raw_amount,
                ui_amount: change.ui_amount,
                decimals: change.decimals,
                is_buy: change.operation == TokenOperation::Buy,
            };
            
            token_changes.push(token_change_record);
        }

        // If no token changes and no significant SOL change, skip this transaction
        if token_changes.is_empty() && sol_balance_change.abs() <= 1000000 { // < 0.001 SOL
            return Err(HeliusError::ConfigError(
                "No significant balance changes found".to_string()
            ));
        }

        let record = TransactionRecord {
            signature: transaction.signature.clone(),
            wallet_address: wallet_address.to_string(),
            timestamp: transaction.timestamp,
            source: transaction.source.clone(),
            fee: transaction.fee,
            token_changes,
            sol_balance_change,
        };

        Ok(record)
    }

    /// Parse a single token balance change into a TokenChange struct (legacy method)
    fn parse_token_balance_change(
        &self,
        balance_change: &HeliusTokenBalanceChange,
        transaction: &HeliusTransaction,
    ) -> Result<TokenChange> {
        // Parse the raw token amount (can be negative)
        let raw_amount = balance_change.raw_token_amount.token_amount
            .parse::<i64>()
            .map_err(|e| HeliusError::ConfigError(
                format!("Invalid token amount: {}", e)
            ))?;

        if raw_amount == 0 {
            return Err(HeliusError::ConfigError(
                "Zero token amount change".to_string()
            ));
        }

        // Calculate UI amount
        let decimals = balance_change.raw_token_amount.decimals;
        let ui_amount = raw_amount as f64 / 10_f64.powi(decimals as i32);

        // Determine operation type
        let operation = if raw_amount > 0 {
            TokenOperation::Buy  // Positive = received tokens
        } else {
            TokenOperation::Sell // Negative = sent tokens
        };

        Ok(TokenChange {
            mint: balance_change.mint.clone(),
            raw_amount,
            ui_amount,
            decimals,
            operation,
            transaction_signature: transaction.signature.clone(),
            timestamp: transaction.timestamp,
            source: transaction.source.clone(),
        })
    }
}

/// Represents a token balance change for a specific wallet
#[derive(Debug, Clone)]
pub struct TokenChange {
    /// Token mint address
    pub mint: String,
    /// Raw token amount change (positive = received, negative = sent)
    pub raw_amount: i64,
    /// Human-readable amount change
    pub ui_amount: f64,
    /// Token decimals
    pub decimals: u8,
    /// Operation type (buy/sell)
    pub operation: TokenOperation,
    /// Transaction signature
    pub transaction_signature: String,
    /// Transaction timestamp
    pub timestamp: i64,
    /// DEX source (Jupiter, Raydium, etc.)
    pub source: String,
}

/// Represents a token balance change with price information
#[derive(Debug, Clone)]
pub struct TokenChangeWithPrice {
    /// Token mint address
    pub mint: String,
    /// Raw token amount change (positive = received, negative = sent)
    pub raw_amount: i64,
    /// Human-readable amount change
    pub ui_amount: f64,
    /// Token decimals
    pub decimals: u8,
    /// Operation type (buy/sell)
    pub operation: TokenOperation,
    /// Transaction signature
    pub transaction_signature: String,
    /// Transaction timestamp
    pub timestamp: i64,
    /// DEX source (Jupiter, Raydium, etc.)
    pub source: String,
    /// Token price in USD at the time of transaction
    pub price_usd: Decimal,
    /// Total USD value of the token change
    pub usd_value: Decimal,
}

/// Token operation type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenOperation {
    /// Received tokens (positive balance change)
    Buy,
    /// Sent tokens (negative balance change)
    Sell,
}

// Helius API response structures based on Enhanced Transactions API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusTransaction {
    pub signature: String,
    pub timestamp: i64,
    pub slot: u64,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub source: String,
    pub description: String,
    pub fee: u64,
    #[serde(rename = "feePayer")]
    pub fee_payer: String,
    
    // Transaction data arrays
    #[serde(rename = "nativeTransfers")]
    pub native_transfers: Vec<HeliusNativeTransfer>,
    #[serde(rename = "tokenTransfers")]
    pub token_transfers: Vec<HeliusTokenTransfer>,
    #[serde(rename = "accountData")]
    pub account_data: Vec<HeliusAccountData>,
    
    // Events (for swap transactions)
    pub events: Option<HeliusEvents>,
    
    // Additional fields from Helius API
    #[serde(rename = "transactionError")]
    pub transaction_error: Option<HeliusTransactionError>,
    pub instructions: Vec<HeliusInstruction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusNativeTransfer {
    #[serde(rename = "fromUserAccount")]
    pub from_user_account: String,
    #[serde(rename = "toUserAccount")]
    pub to_user_account: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusTokenTransfer {
    #[serde(rename = "fromUserAccount")]
    pub from_user_account: String,
    #[serde(rename = "toUserAccount")]
    pub to_user_account: String,
    #[serde(rename = "fromTokenAccount")]
    pub from_token_account: String,
    #[serde(rename = "toTokenAccount")]
    pub to_token_account: String,
    #[serde(rename = "tokenAmount")]
    pub token_amount: f64, // Changed from u64 to f64 to match actual API
    pub mint: String,
    #[serde(rename = "tokenStandard")]
    pub token_standard: String, // Added missing field
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusAccountData {
    pub account: String,
    #[serde(rename = "nativeBalanceChange")]
    pub native_balance_change: i64,
    #[serde(rename = "tokenBalanceChanges")]
    pub token_balance_changes: Vec<HeliusTokenBalanceChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusTokenBalanceChange {
    #[serde(rename = "userAccount")]
    pub user_account: String,
    #[serde(rename = "tokenAccount")]
    pub token_account: String,
    pub mint: String,
    #[serde(rename = "rawTokenAmount")]
    pub raw_token_amount: HeliusRawTokenAmount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusRawTokenAmount {
    #[serde(rename = "tokenAmount")]
    pub token_amount: String,  // Can be negative (e.g., "-100000000")
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusEvents {
    pub swap: Option<HeliusSwapEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusSwapEvent {
    #[serde(rename = "nativeInput")]
    pub native_input: Option<HeliusNativeIO>,
    #[serde(rename = "nativeOutput")]
    pub native_output: Option<HeliusNativeIO>,
    #[serde(rename = "tokenInputs")]
    pub token_inputs: Vec<HeliusTokenIO>,
    #[serde(rename = "tokenOutputs")]
    pub token_outputs: Vec<HeliusTokenIO>,
    #[serde(rename = "innerSwaps")]
    pub inner_swaps: Vec<HeliusInnerSwap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusNativeIO {
    pub account: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusTokenIO {
    #[serde(rename = "userAccount")]
    pub user_account: String,
    #[serde(rename = "tokenAccount")]
    pub token_account: String,
    pub mint: String,
    #[serde(rename = "rawTokenAmount")]
    pub raw_token_amount: HeliusRawTokenAmount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusInnerSwap {
    #[serde(rename = "programInfo")]
    pub program_info: HeliusProgramInfo,
    #[serde(rename = "tokenInputs")]
    pub token_inputs: Vec<HeliusTokenIO>,
    #[serde(rename = "tokenOutputs")]
    pub token_outputs: Vec<HeliusTokenIO>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusProgramInfo {
    pub source: String,
    pub account: String,
    #[serde(rename = "programName")]
    pub program_name: String,
    #[serde(rename = "instructionName")]
    pub instruction_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusTransactionError {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusInstruction {
    pub accounts: Vec<String>,
    pub data: String,
    #[serde(rename = "programId")]
    pub program_id: String,
    #[serde(rename = "innerInstructions")]
    pub inner_instructions: Vec<HeliusInnerInstruction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusInnerInstruction {
    pub accounts: Vec<String>,
    pub data: String,
    #[serde(rename = "programId")]
    pub program_id: String,
}

impl HeliusClient {
    /// Convert Helius transactions to FinancialEvents with token metadata and price enrichment
    /// This is the main method for the new architecture
    pub async fn helius_to_financial_events(
        &self,
        transactions: &[HeliusTransaction],
        wallet_address: &str,
    ) -> Result<Vec<FinancialEvent>> {
        if transactions.is_empty() {
            return Ok(Vec::new());
        }

        info!("Converting {} Helius transactions to FinancialEvents for wallet {}", 
              transactions.len(), wallet_address);

        // Step 1: Extract unique token mints from all transactions
        let mut token_mints = std::collections::HashSet::new();
        for transaction in transactions {
            for account_data in &transaction.account_data {
                if account_data.account == wallet_address {
                    for balance_change in &account_data.token_balance_changes {
                        if balance_change.user_account == wallet_address {
                            token_mints.insert(balance_change.mint.clone());
                        }
                    }
                }
            }
        }

        let unique_mints: Vec<String> = token_mints.into_iter().collect();
        info!("Found {} unique token mints to fetch metadata for", unique_mints.len());

        // Step 2: Batch fetch token metadata
        let token_metadata = if let Some(ref metadata_service) = self.token_metadata_service {
            metadata_service.get_metadata_batch_with_fallback(&unique_mints).await
        } else {
            warn!("No token metadata service configured, using fallback metadata");
            self.create_fallback_metadata(&unique_mints)
        };

        // Step 3: Convert each transaction to FinancialEvents
        let mut all_events = Vec::new();
        for transaction in transactions {
            match self.convert_helius_transaction_to_events(
                transaction,
                wallet_address,
                &token_metadata,
            ).await {
                Ok(events) => all_events.extend(events),
                Err(e) => {
                    warn!("Failed to convert transaction {} to events: {}", 
                          transaction.signature, e);
                    continue;
                }
            }
        }

        info!("Successfully converted {} Helius transactions to {} FinancialEvents", 
              transactions.len(), all_events.len());

        Ok(all_events)
    }

    /// Convert a single Helius transaction to FinancialEvents
    async fn convert_helius_transaction_to_events(
        &self,
        transaction: &HeliusTransaction,
        wallet_address: &str,
        token_metadata: &HashMap<String, crate::token_metadata_service::TokenMetadata>,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        // Convert timestamp
        let timestamp = DateTime::from_timestamp(transaction.timestamp, 0)
            .unwrap_or_else(Utc::now);

        // Find account data for our wallet
        let wallet_account_data = transaction.account_data.iter()
            .find(|acc| acc.account == wallet_address);

        if let Some(account_data) = wallet_account_data {
            // Process token balance changes
            for balance_change in &account_data.token_balance_changes {
                if balance_change.user_account == wallet_address {
                    match self.convert_token_balance_change_to_event(
                        balance_change,
                        transaction,
                        wallet_address,
                        timestamp,
                        token_metadata,
                    ).await {
                        Ok(event) => events.push(event),
                        Err(e) => {
                            warn!("Failed to convert token balance change for mint {}: {}", 
                                  balance_change.mint, e);
                        }
                    }
                }
            }

            // Process SOL balance changes (if significant)
            if account_data.native_balance_change.abs() > 10_000_000 { // > 0.01 SOL
                let sol_event = self.create_sol_balance_event(
                    account_data.native_balance_change,
                    transaction,
                    wallet_address,
                    timestamp,
                )?;
                events.push(sol_event);
            }
        }

        Ok(events)
    }

    /// Convert a single token balance change to a FinancialEvent
    async fn convert_token_balance_change_to_event(
        &self,
        balance_change: &HeliusTokenBalanceChange,
        transaction: &HeliusTransaction,
        wallet_address: &str,
        timestamp: DateTime<Utc>,
        token_metadata: &HashMap<String, crate::token_metadata_service::TokenMetadata>,
    ) -> Result<FinancialEvent> {
        // Parse token amount
        let raw_amount: i64 = balance_change.raw_token_amount.token_amount.parse()
            .map_err(|e| HeliusError::ConfigError(format!("Invalid token amount: {}", e)))?;

        let decimals = balance_change.raw_token_amount.decimals;
        let token_amount = Decimal::from(raw_amount.abs()) / Decimal::from(10_i64.pow(decimals as u32));

        // Determine event type
        let event_type = if raw_amount > 0 {
            EventType::Buy
        } else {
            EventType::Sell
        };

        // Get token metadata
        let token_meta = token_metadata.get(&balance_change.mint);
        let token_symbol = token_meta.map(|m| m.symbol.clone()).unwrap_or_else(|| "UNKNOWN".to_string());
        let token_name = token_meta.map(|m| m.name.clone()).unwrap_or_else(|| "Unknown Token".to_string());

        // Get historical price - for now use zero and let P&L engine handle it
        let price_per_token = Decimal::ZERO; // Will be fetched by P&L engine as needed

        // Calculate USD value
        let usd_value = token_amount * price_per_token;

        // Determine SOL amount (only if this is actually SOL)
        let sol_mint = "So11111111111111111111111111111111111111112";
        let sol_amount = if balance_change.mint == sol_mint {
            if raw_amount > 0 { token_amount } else { -token_amount }
        } else {
            Decimal::ZERO
        };

        // Create metadata
        let mut extra = HashMap::new();
        extra.insert("token_symbol".to_string(), token_symbol);
        extra.insert("token_name".to_string(), token_name);
        extra.insert("source".to_string(), transaction.source.clone());
        extra.insert("transaction_type".to_string(), transaction.transaction_type.clone());

        let event = FinancialEvent {
            id: Uuid::new_v4(),
            transaction_id: transaction.signature.clone(),
            wallet_address: wallet_address.to_string(),
            event_type,
            token_mint: balance_change.mint.clone(),
            token_amount,
            sol_amount,
            usd_value,
            timestamp,
            transaction_fee: Decimal::from(transaction.fee) / Decimal::from(1_000_000_000), // Convert lamports to SOL
            metadata: EventMetadata {
                program_id: None,
                instruction_index: None,
                exchange: Some(transaction.source.clone()),
                price_per_token: if price_per_token > Decimal::ZERO { Some(price_per_token) } else { None },
                extra,
            },
        };

        Ok(event)
    }

    /// Create a FinancialEvent for SOL balance changes
    fn create_sol_balance_event(
        &self,
        native_balance_change: i64,
        transaction: &HeliusTransaction,
        wallet_address: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<FinancialEvent> {
        let sol_amount = Decimal::from(native_balance_change) / Decimal::from(1_000_000_000); // Convert lamports to SOL
        let sol_mint = "So11111111111111111111111111111111111111112";

        let event_type = if native_balance_change > 0 {
            EventType::Buy
        } else {
            EventType::Sell
        };

        // For SOL, we'll use a fixed price or fetch it - for now, use zero and let the P&L engine handle it
        let price_per_token = Decimal::ZERO; // Will be fetched by P&L engine
        let usd_value = Decimal::ZERO; // Will be calculated by P&L engine

        let mut extra = HashMap::new();
        extra.insert("token_symbol".to_string(), "SOL".to_string());
        extra.insert("token_name".to_string(), "Solana".to_string());
        extra.insert("source".to_string(), transaction.source.clone());
        extra.insert("transaction_type".to_string(), transaction.transaction_type.clone());

        let event = FinancialEvent {
            id: Uuid::new_v4(),
            transaction_id: transaction.signature.clone(),
            wallet_address: wallet_address.to_string(),
            event_type,
            token_mint: sol_mint.to_string(),
            token_amount: sol_amount.abs(),
            sol_amount,
            usd_value,
            timestamp,
            transaction_fee: Decimal::ZERO, // Don't double-count fees
            metadata: EventMetadata {
                program_id: None,
                instruction_index: None,
                exchange: Some(transaction.source.clone()),
                price_per_token: if price_per_token > Decimal::ZERO { Some(price_per_token) } else { None },
                extra,
            },
        };

        Ok(event)
    }

    /// Create fallback metadata for tokens when metadata service is not available
    fn create_fallback_metadata(&self, token_mints: &[String]) -> HashMap<String, crate::token_metadata_service::TokenMetadata> {
        let mut metadata = HashMap::new();

        for mint in token_mints {
            let fallback = crate::token_metadata_service::TokenMetadata {
                address: mint.clone(),
                symbol: Self::extract_symbol_from_mint(mint),
                name: format!("Token ({})", Self::shorten_mint(mint)),
                decimals: 9, // Default for most Solana tokens
                logo_uri: None,
                extensions: None,
            };
            metadata.insert(mint.clone(), fallback);
        }

        metadata
    }

    /// Extract a short symbol from mint address
    fn extract_symbol_from_mint(mint: &str) -> String {
        if mint.len() >= 8 {
            format!("{}..{}", &mint[..4], &mint[mint.len()-4..])
        } else {
            mint.to_string()
        }
    }

    /// Shorten mint for display
    fn shorten_mint(mint: &str) -> String {
        if mint.len() >= 12 {
            format!("{}..{}", &mint[..6], &mint[mint.len()-6..])
        } else {
            mint.to_string()
        }
    }

    /// Convert Helius transactions to GeneralTraderTransaction format (for BirdEye compatibility)
    /// This method provides backward compatibility with existing P&L algorithms
    pub async fn helius_to_general_trader_transactions(
        &self,
        transactions: &[HeliusTransaction],
        wallet_address: &str,
    ) -> Result<Vec<GeneralTraderTransaction>> {
        if transactions.is_empty() {
            return Ok(Vec::new());
        }

        info!("Converting {} Helius transactions to GeneralTraderTransaction format for wallet {}", 
              transactions.len(), wallet_address);

        let mut general_transactions = Vec::new();

        for transaction in transactions {
            match self.convert_helius_to_general_trader_transaction(transaction, wallet_address).await {
                Ok(Some(general_tx)) => general_transactions.push(general_tx),
                Ok(None) => {
                    debug!("Transaction {} has no relevant changes for wallet {}", 
                           transaction.signature, wallet_address);
                }
                Err(e) => {
                    warn!("Failed to convert transaction {} to GeneralTraderTransaction: {}", 
                          transaction.signature, e);
                    continue;
                }
            }
        }

        info!("Successfully converted {} Helius transactions to {} GeneralTraderTransactions", 
              transactions.len(), general_transactions.len());

        Ok(general_transactions)
    }

    /// Convert a single Helius transaction to GeneralTraderTransaction format
    async fn convert_helius_to_general_trader_transaction(
        &self,
        transaction: &HeliusTransaction,
        wallet_address: &str,
    ) -> Result<Option<GeneralTraderTransaction>> {
        // Find account data for our wallet
        let wallet_account_data = transaction.account_data.iter()
            .find(|acc| acc.account == wallet_address);

        let Some(account_data) = wallet_account_data else {
            return Ok(None); // No data for this wallet
        };

        // Look for token swaps in the balance changes
        let mut token_changes = Vec::new();
        for balance_change in &account_data.token_balance_changes {
            if balance_change.user_account == wallet_address {
                let raw_amount: i64 = balance_change.raw_token_amount.token_amount.parse()
                    .map_err(|e| HeliusError::ConfigError(format!("Invalid token amount: {}", e)))?;
                
                if raw_amount != 0 {
                    token_changes.push((balance_change, raw_amount));
                }
            }
        }

        // We need at least 2 token changes to create a swap (from/to)
        if token_changes.len() < 2 {
            return Ok(None);
        }

        // For simplicity, take the first sell (negative) and first buy (positive)
        let sell_change = token_changes.iter().find(|(_, amount)| *amount < 0);
        let buy_change = token_changes.iter().find(|(_, amount)| *amount > 0);

        if let (Some((sell_balance, sell_amount)), Some((buy_balance, buy_amount))) = (sell_change, buy_change) {
            // Create the GeneralTraderTransaction
            let general_tx = GeneralTraderTransaction {
                // Quote is typically the token being sold (from)
                quote: pnl_core::TokenTransactionSide {
                    symbol: self.get_token_symbol_fallback(&sell_balance.mint),
                    decimals: sell_balance.raw_token_amount.decimals as u32,
                    address: sell_balance.mint.clone(),
                    amount: sell_amount.unsigned_abs() as u128,
                    transfer_type: Some("transferChecked".to_string()),
                    type_swap: "from".to_string(),
                    ui_amount: (*sell_amount).abs() as f64 / 10_f64.powi(sell_balance.raw_token_amount.decimals as i32),
                    price: None, // Will be filled by price fetching service
                    nearest_price: None,
                    change_amount: *sell_amount as i128,
                    ui_change_amount: *sell_amount as f64 / 10_f64.powi(sell_balance.raw_token_amount.decimals as i32),
                    fee_info: None,
                },
                // Base is typically the token being bought (to)
                base: pnl_core::TokenTransactionSide {
                    symbol: self.get_token_symbol_fallback(&buy_balance.mint),
                    decimals: buy_balance.raw_token_amount.decimals as u32,
                    address: buy_balance.mint.clone(),
                    amount: *buy_amount as u128,
                    transfer_type: Some("transferChecked".to_string()),
                    type_swap: "to".to_string(),
                    ui_amount: *buy_amount as f64 / 10_f64.powi(buy_balance.raw_token_amount.decimals as i32),
                    price: None, // Will be filled by price fetching service
                    nearest_price: None,
                    change_amount: *buy_amount as i128,
                    ui_change_amount: *buy_amount as f64 / 10_f64.powi(buy_balance.raw_token_amount.decimals as i32),
                    fee_info: None,
                },
                base_price: None,
                quote_price: 0.0, // Will be filled by price fetching service
                tx_hash: transaction.signature.clone(),
                source: transaction.source.clone(),
                block_unix_time: transaction.timestamp,
                tx_type: "swap".to_string(),
                address: "".to_string(), // Program address - not available in Helius data
                owner: wallet_address.to_string(),
                volume_usd: 0.0, // Will be calculated by price fetching service
            };

            Ok(Some(general_tx))
        } else {
            Ok(None) // No valid swap found
        }
    }

    /// Get token symbol with fallback to mint address
    fn get_token_symbol_fallback(&self, mint: &str) -> String {
        // Check if this is SOL
        if mint == "So11111111111111111111111111111111111111112" {
            return "SOL".to_string();
        }
        
        // For other tokens, use fallback symbol
        Self::extract_symbol_from_mint(mint)
    }
}

