use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum ConfigurationError {
    #[error("Configuration loading error: {0}")]
    ConfigLoad(#[from] ConfigError),
    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),
}

pub type Result<T> = std::result::Result<T, ConfigurationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// General system settings
    pub system: SystemSettings,
    
    
    /// Redis configuration
    pub redis: RedisConfig,
    
    /// DexScreener configuration
    pub dexscreener: DexScreenerConfig,
    
    /// BirdEye API configuration
    pub birdeye: BirdEyeConfig,
    
    /// P&L calculation settings
    pub pnl: PnLConfig,
    
    /// Advanced trader filtering for copy trading
    pub trader_filter: TraderFilterConfig,
    
    /// API server configuration
    pub api: ApiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSettings {
    /// Enable debug mode
    pub debug_mode: bool,
    
    /// Redis mode for 24/7 operation (true) vs batch mode (false)
    pub redis_mode: bool,
    
    /// Process loop interval in milliseconds for continuous mode
    pub process_loop_ms: u64,
    
    /// Output CSV file path
    pub output_csv_file: String,
    
    /// Parallel batch size for P&L queue processing (defaults to 10)
    pub pnl_parallel_batch_size: Option<usize>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    
    /// Connection timeout in seconds
    pub connection_timeout_seconds: u64,
    
    /// Default lock TTL in seconds
    pub default_lock_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexScreenerConfig {
    /// NEW: Official DexScreener API base URL for HTTP-based trending discovery
    pub api_base_url: String,
    
    /// LEGACY: WebSocket URL for trending pairs (deprecated but kept for fallback)
    pub websocket_url: String,
    
    /// LEGACY: HTTP API base URL for pair details (deprecated)
    pub http_base_url: String,
    
    /// User agent string for requests
    pub user_agent: String,
    
    /// Reconnection delay in seconds (for legacy WebSocket)
    pub reconnect_delay_seconds: u64,
    
    /// Maximum reconnection attempts (for legacy WebSocket)
    pub max_reconnect_attempts: u32,
    
    /// NEW: Trending token discovery settings
    pub trending: TrendingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingConfig {
    /// Minimum 24h volume in USD for trending threshold
    pub min_volume_24h: f64,
    
    /// Minimum 24h transaction count for trending threshold
    pub min_txns_24h: u64,
    
    /// Minimum liquidity in USD for trending threshold
    pub min_liquidity_usd: f64,
    
    /// Minimum 24h price change percentage for trending (optional)
    pub min_price_change_24h: Option<f64>,
    
    /// Maximum pair age in hours for trending analysis
    pub max_pair_age_hours: Option<u64>,
    
    /// Polling interval in seconds for trending discovery
    pub polling_interval_seconds: u64,
    
    /// Maximum tokens to analyze per discovery cycle
    pub max_tokens_per_cycle: u32,
    
    /// Wallet discovery limit per trending pair
    pub wallet_discovery_limit: u32,
    
    /// Rate limit between API requests in milliseconds
    pub rate_limit_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdEyeConfig {
    /// BirdEye API key
    pub api_key: String,
    
    /// BirdEye API base URL  
    pub api_base_url: String,
    
    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
    
    /// Price cache TTL in seconds
    pub price_cache_ttl_seconds: u64,
    
    /// Rate limit per second
    pub rate_limit_per_second: u32,
    
    /// Maximum traders to fetch per token (API supports max 100)
    pub max_traders_per_token: u32,
    
    /// Maximum transactions per trader (API supports max 100)
    pub max_transactions_per_trader: u32,
    
    /// Default maximum transactions to fetch/analyze per trader (across all paginated calls)
    pub default_max_transactions: u32,
    
    /// Maximum rank for top tokens (used in trending discovery)
    pub max_token_rank: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLConfig {
    /// Timeframe mode: "none", "general", or "specific"
    pub timeframe_mode: String,
    
    /// General timeframe (e.g., "1m", "1h", "1d", "1y")
    pub timeframe_general: Option<String>,
    
    /// Specific timeframe start (ISO 8601 format)
    pub timeframe_specific: Option<String>,
    
    /// Minimum wallet capital in SOL
    pub wallet_min_capital: f64,
    
    /// Minimum average hold time in minutes
    pub aggregator_min_hold_minutes: f64,
    
    /// Minimum number of trades required
    pub amount_trades: u32,
    
    /// Minimum win rate percentage (0-100)
    pub win_rate: f64,
    
    /// Batch size for processing
    pub aggregator_batch_size: u32,
    
    /// Maximum number of transaction signatures to analyze per wallet
    pub max_signatures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderFilterConfig {
    /// Minimum realized P&L in USD to qualify as real trader
    pub min_realized_pnl_usd: f64,
    
    /// Minimum total trades
    pub min_total_trades: u32,
    
    /// Minimum winning trades
    pub min_winning_trades: u32,
    
    /// Minimum win rate percentage (0-100)
    pub min_win_rate: f64,
    
    /// Minimum ROI percentage
    pub min_roi_percentage: f64,
    
    /// Minimum capital deployed in SOL
    pub min_capital_deployed_sol: f64,
    
    /// Maximum average hold time in minutes
    pub max_avg_hold_time_minutes: f64,
    
    /// Minimum average hold time in minutes
    pub min_avg_hold_time_minutes: f64,
    
    /// Exclude wallets with only buy transactions (holders)
    pub exclude_holders_only: bool,
    
    /// Exclude wallets with zero P&L
    pub exclude_zero_pnl: bool,
    
    /// Minimum transaction frequency (trades per day in timeframe)
    pub min_transaction_frequency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API server host
    pub host: String,
    
    /// API server port
    pub port: u16,
    
    /// Enable CORS
    pub enable_cors: bool,
    
    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system: SystemSettings {
                debug_mode: false,
                redis_mode: false,
                process_loop_ms: 60000,
                output_csv_file: "final_output.csv".to_string(),
                pnl_parallel_batch_size: Some(10),
            },
            redis: RedisConfig {
                url: "redis://127.0.0.1:6379".to_string(),
                connection_timeout_seconds: 10,
                default_lock_ttl_seconds: 600,
            },
            dexscreener: DexScreenerConfig {
                api_base_url: "https://api.dexscreener.com".to_string(),
                websocket_url: "wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1?rankBy[key]=trendingScoreH24&rankBy[order]=desc".to_string(),
                http_base_url: "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana".to_string(),
                user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36".to_string(),
                reconnect_delay_seconds: 30,
                max_reconnect_attempts: 5,
                trending: TrendingConfig {
                    min_volume_24h: 1_270_000.0,    // $1.27M based on analysis
                    min_txns_24h: 45_000,           // 45K transactions based on analysis
                    min_liquidity_usd: 10_000.0,    // $10K minimum liquidity
                    min_price_change_24h: Some(50.0), // 50% price change for high volatility
                    max_pair_age_hours: Some(168),   // 1 week old max
                    polling_interval_seconds: 60,    // 1 minute between trending discovery cycles
                    max_tokens_per_cycle: 20,        // Analyze top 20 boosted tokens per cycle
                    wallet_discovery_limit: 10,      // Max 10 wallets per trending pair
                    rate_limit_ms: 200,             // 200ms between API requests (300 req/min limit)
                },
            },
            birdeye: BirdEyeConfig {
                api_key: "".to_string(), // Must be set in .env or config file
                api_base_url: "https://public-api.birdeye.so".to_string(),
                request_timeout_seconds: 30,
                price_cache_ttl_seconds: 60,
                rate_limit_per_second: 100,
                max_traders_per_token: 10,        // Default to 10 traders per token
                max_transactions_per_trader: 100, // BirdEye API limit is 100
                default_max_transactions: 1000, // Default to 1000 total transactions
                max_token_rank: 1000,             // Top 1000 ranked tokens
            },
            pnl: PnLConfig {
                timeframe_mode: "none".to_string(),
                timeframe_general: None,
                timeframe_specific: None,
                wallet_min_capital: 0.0,
                aggregator_min_hold_minutes: 0.0,
                amount_trades: 0,
                win_rate: 0.0,
                aggregator_batch_size: 20,
                max_signatures: 1000,
            },
            trader_filter: TraderFilterConfig {
                min_realized_pnl_usd: 0.10,
                min_total_trades: 3,
                min_winning_trades: 2,
                min_win_rate: 35.0,
                min_roi_percentage: 10.0,
                min_capital_deployed_sol: 0.05,
                max_avg_hold_time_minutes: 1440.0, // 24 hours
                min_avg_hold_time_minutes: 1.0,
                exclude_holders_only: true,
                exclude_zero_pnl: true,
                min_transaction_frequency: 0.1, // 0.1 trades/day min
            },
            api: ApiConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                enable_cors: true,
                request_timeout_seconds: 30,
            },
        }
    }
}

impl BirdEyeConfig {
    /// Validate BirdEye configuration values against API limits
    pub fn validate(&self) -> Result<()> {
        if self.max_traders_per_token > 100 {
            return Err(ConfigurationError::InvalidValue("max_traders_per_token cannot exceed 100 (BirdEye API limit)".to_string()));
        }
        if self.max_transactions_per_trader > 100 {
            return Err(ConfigurationError::InvalidValue("max_transactions_per_trader cannot exceed 100 (BirdEye API limit)".to_string()));
        }
        Ok(())
    }
}

impl SystemConfig {
    /// Load configuration from file and environment variables
    pub fn load() -> Result<Self> {
        Self::load_from_path("config.toml")
    }
    
    /// Load configuration from a specific file path
    pub fn load_from_path<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let mut config_builder = Config::builder()
            // Start with defaults
            .add_source(Config::try_from(&SystemConfig::default())?);
        
        // Add config file if it exists
        if config_path.as_ref().exists() {
            info!("Loading configuration from: {}", config_path.as_ref().display());
            config_builder = config_builder.add_source(File::from(config_path.as_ref()));
        } else {
            debug!("Config file not found, using defaults and environment variables");
        }
        
        // Add environment variables with prefix
        config_builder = config_builder.add_source(
            Environment::with_prefix("PNL")
                .try_parsing(true)
                .separator("__")
                .list_separator(",")
        );
        
        let config = config_builder.build()?;
        
        // Debug: Print the raw config values to understand the parsing issue
        if let Ok(birdeye_key) = config.get::<String>("birdeye.api_key") {
            debug!("Raw birdeye.api_key value: '{}'", birdeye_key);
        } else {
            debug!("Failed to get birdeye.api_key as string");
        }
        
        let system_config: SystemConfig = config.try_deserialize()?;
        
        // Validate configuration
        system_config.validate()?;
        
        Ok(system_config)
    }
    
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate timeframe mode
        if !["none", "general", "specific"].contains(&self.pnl.timeframe_mode.as_str()) {
            return Err(ConfigurationError::InvalidValue(
                format!("Invalid timeframe_mode: {}", self.pnl.timeframe_mode)
            ));
        }
        
        // Validate general timeframe format if specified
        if self.pnl.timeframe_mode == "general" {
            if let Some(ref general) = self.pnl.timeframe_general {
                if !is_valid_general_timeframe(general) {
                    return Err(ConfigurationError::InvalidValue(
                        format!("Invalid timeframe_general format: {}", general)
                    ));
                }
            } else {
                return Err(ConfigurationError::InvalidValue(
                    "timeframe_general must be specified when timeframe_mode is 'general'".to_string()
                ));
            }
        }
        
        // Validate specific timeframe format if specified
        if self.pnl.timeframe_mode == "specific" {
            if let Some(ref specific) = self.pnl.timeframe_specific {
                if !is_valid_iso8601_date(specific) {
                    return Err(ConfigurationError::InvalidValue(
                        format!("Invalid timeframe_specific format: {}", specific)
                    ));
                }
            } else {
                return Err(ConfigurationError::InvalidValue(
                    "timeframe_specific must be specified when timeframe_mode is 'specific'".to_string()
                ));
            }
        }
        
        // Validate numeric ranges
        if self.pnl.win_rate < 0.0 || self.pnl.win_rate > 100.0 {
            return Err(ConfigurationError::InvalidValue(
                format!("win_rate must be between 0 and 100, got: {}", self.pnl.win_rate)
            ));
        }
        
        if self.api.port == 0 {
            return Err(ConfigurationError::InvalidValue(
                "API port cannot be 0".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Get configuration as a JSON value for API responses
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
    
    /// Update configuration from JSON value
    pub fn update_from_json(&mut self, value: serde_json::Value) -> Result<()> {
        let updated_config: SystemConfig = serde_json::from_value(value)
            .map_err(|e| ConfigurationError::InvalidValue(e.to_string()))?;
        
        updated_config.validate()?;
        *self = updated_config;
        Ok(())
    }
}

/// Validate general timeframe format (e.g., "1m", "1h", "1d", "1y")
fn is_valid_general_timeframe(timeframe: &str) -> bool {
    let pattern = regex::Regex::new(r"^(\d+)(s|min|h|d|m|y)$").unwrap();
    pattern.is_match(timeframe)
}

/// Basic validation for ISO 8601 date format
fn is_valid_iso8601_date(date: &str) -> bool {
    // Basic check for common ISO 8601 formats
    let patterns = [
        r"^\d{4}-\d{2}-\d{2}$",                           // YYYY-MM-DD
        r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z?$",     // YYYY-MM-DDTHH:MM:SSZ
        r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z?$", // YYYY-MM-DDTHH:MM:SS.sssZ
    ];
    
    patterns.iter().any(|pattern| {
        regex::Regex::new(pattern).unwrap().is_match(date)
    })
}

/// Configuration manager for loading and managing system configuration
#[derive(Debug)]
pub struct ConfigManager {
    config: SystemConfig,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new() -> Result<Self> {
        let config = SystemConfig::load()?;
        info!("Configuration loaded successfully");
        debug!("Configuration: {:#?}", config);
        
        Ok(Self { config })
    }
    
    /// Create configuration manager from a specific file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = SystemConfig::load_from_path(path)?;
        Ok(Self { config })
    }
    
    /// Get a reference to the current configuration
    pub fn config(&self) -> &SystemConfig {
        &self.config
    }
    
    /// Get a mutable reference to the configuration
    pub fn config_mut(&mut self) -> &mut SystemConfig {
        &mut self.config
    }
    
    /// Update configuration
    pub fn update_config(&mut self, new_config: SystemConfig) -> Result<()> {
        new_config.validate()?;
        self.config = new_config;
        info!("Configuration updated");
        Ok(())
    }
    
    /// Reload configuration from file and environment
    pub fn reload(&mut self) -> Result<()> {
        self.config = SystemConfig::load()?;
        info!("Configuration reloaded");
        Ok(())
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            Self {
                config: SystemConfig::default(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SystemConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_timeframe_validation() {
        assert!(is_valid_general_timeframe("1m"));
        assert!(is_valid_general_timeframe("30min"));
        assert!(is_valid_general_timeframe("24h"));
        assert!(is_valid_general_timeframe("7d"));
        assert!(is_valid_general_timeframe("1y"));
        assert!(!is_valid_general_timeframe("invalid"));
        assert!(!is_valid_general_timeframe("1"));
    }

    #[test]
    fn test_iso8601_validation() {
        assert!(is_valid_iso8601_date("2024-12-10"));
        assert!(is_valid_iso8601_date("2024-12-10T18:00:00Z"));
        assert!(is_valid_iso8601_date("2024-12-10T18:00:00"));
        assert!(is_valid_iso8601_date("2024-12-10T18:00:00.123Z"));
        assert!(!is_valid_iso8601_date("invalid-date"));
        assert!(!is_valid_iso8601_date("2024-13-10"));
    }
}