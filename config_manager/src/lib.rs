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

// Data source is simplified to just a string since only BirdEye is used
// Previously was a complex enum with primary/fallback logic that was never utilized

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// General system settings
    pub system: SystemSettings,
    
    /// Data source for transaction data (currently only "BirdEye" is supported)
    pub data_source: String,
    
    /// Redis configuration
    pub redis: RedisConfig,
    
    /// BirdEye API configuration
    pub birdeye: BirdEyeConfig,
    
    /// DexScreener API configuration
    pub dexscreener: DexScreenerConfig,
    
    
    /// Advanced trader filtering for copy trading
    pub trader_filter: TraderFilterConfig,
    
    /// API server configuration
    pub api: ApiConfig,
    
    /// Database configuration
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL
    pub postgres_url: String,
    
    /// Enable PostgreSQL for P&L result storage
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSettings {
    /// Enable debug mode
    pub debug_mode: bool,
    
    /// Process loop interval in milliseconds for continuous mode
    pub process_loop_ms: u64,
    
    /// Parallel batch size for P&L queue processing (defaults to 10)
    pub pnl_parallel_batch_size: Option<usize>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    
    /// Default lock TTL in seconds
    pub default_lock_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirdEyeConfig {
    /// BirdEye API key
    pub api_key: String,
    
    /// BirdEye API base URL  
    pub api_base_url: String,
    
    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
    
    /// Maximum traders to fetch per token (API supports max 100)
    pub max_traders_per_token: u32,
    
    /// Maximum transactions per trader (API supports max 100)
    pub max_transactions_per_trader: u32,
    
    /// Default maximum transactions to fetch/analyze per trader (across all paginated calls)
    pub default_max_transactions: u32,
    
    /// New listing token discovery enabled
    pub new_listing_enabled: bool,
    
    /// Minimum liquidity for new listing tokens
    pub new_listing_min_liquidity: f64,
    
    /// Maximum age in hours for new listing tokens
    pub new_listing_max_age_hours: u32,
    
    /// Maximum number of new listing tokens to process
    pub new_listing_max_tokens: usize,
    
    /// Maximum number of trending tokens to process (0 = unlimited)
    pub max_trending_tokens: usize,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexScreenerConfig {
    /// DexScreener API base URL
    pub api_base_url: String,
    
    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
    
    /// Rate limit delay between requests in milliseconds (60 requests per minute)
    pub rate_limit_delay_ms: u64,
    
    /// Maximum retry attempts for failed requests
    pub max_retries: u32,
    
    /// Enable DexScreener for boosted token discovery
    pub enabled: bool,
    
    /// Minimum boost amount to consider a token (filters out low-boost tokens)
    pub min_boost_amount: f64,
    
    /// Maximum number of boosted tokens to process per endpoint
    pub max_boosted_tokens: u32,
}


// PnLConfig struct removed - all fields were unused in actual P&L processing
// These were legacy configs from the JavaScript version that were never implemented

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderFilterConfig {
    /// Minimum capital deployed in SOL
    pub min_capital_deployed_sol: f64,
    
    /// Minimum total trades
    pub min_total_trades: u32,
    
    /// Minimum win rate percentage (0-100)
    pub min_win_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API server host
    pub host: String,
    
    /// API server port
    pub port: u16,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system: SystemSettings {
                debug_mode: false,
                process_loop_ms: 60000,
                pnl_parallel_batch_size: Some(10),
            },
            data_source: "BirdEye".to_string(),
            redis: RedisConfig {
                url: "redis://127.0.0.1:6379".to_string(),
                default_lock_ttl_seconds: 600,
            },
            birdeye: BirdEyeConfig {
                api_key: "".to_string(), // Must be set in .env or config file
                api_base_url: "https://public-api.birdeye.so".to_string(),
                request_timeout_seconds: 30,
                max_traders_per_token: 10,        // Default to 10 traders per token
                max_transactions_per_trader: 100, // BirdEye API limit is 100
                default_max_transactions: 1000, // Default to 1000 total transactions
                new_listing_enabled: true,        // Enable new listing discovery by default
                new_listing_min_liquidity: 1000.0, // $1k minimum liquidity
                new_listing_max_age_hours: 24,   // Last 24 hours
                new_listing_max_tokens: 25,      // Top 25 tokens max
                max_trending_tokens: 500,        // Default to 500 trending tokens
            },
            dexscreener: DexScreenerConfig {
                api_base_url: "https://api.dexscreener.com".to_string(),
                request_timeout_seconds: 30,
                rate_limit_delay_ms: 1000,       // 1 request per second (60 req/min)
                max_retries: 3,
                enabled: true,                   // Enable DexScreener by default
                min_boost_amount: 100.0,         // Minimum boost amount to consider
                max_boosted_tokens: 20,          // Max boosted tokens per endpoint
            },
            trader_filter: TraderFilterConfig {
                min_capital_deployed_sol: 0.05,
                min_total_trades: 3,
                min_win_rate: 35.0,
            },
            api: ApiConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            database: DatabaseConfig {
                postgres_url: "postgresql://postgres:password@localhost:5432/wallet_analyzer".to_string(),
                enabled: true,
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
        // Validate individual components
        self.birdeye.validate()?;
        
        // Validate data source
        if self.data_source != "BirdEye" {
            return Err(ConfigurationError::InvalidValue(
                format!("Invalid data_source: {}. Only 'BirdEye' is currently supported", self.data_source)
            ));
        }
        
        if self.api.port == 0 {
            return Err(ConfigurationError::InvalidValue(
                "API port cannot be 0".to_string()
            ));
        }
        
        // Validate BirdEye API key if not default config
        let is_default_config = self.birdeye.api_key.is_empty();
        if !is_default_config && self.data_source == "BirdEye" && self.birdeye.api_key.is_empty() {
            return Err(ConfigurationError::InvalidValue(
                "BirdEye API key is required when BirdEye is used as a data source".to_string()
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

// Removed timeframe validation functions - they were only used for unused PnL configs

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

    // Removed tests for timeframe validation functions since they were deleted
}