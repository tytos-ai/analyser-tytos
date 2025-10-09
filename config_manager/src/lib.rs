use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::{debug, info, warn};

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

    /// Multichain configuration
    pub multichain: MultichainConfig,

    /// Redis configuration
    pub redis: RedisConfig,

    /// BirdEye API configuration (used for trending tokens and top traders discovery only)
    pub birdeye: BirdEyeConfig,

    /// Zerion API configuration (used for wallet transactions and balance fetching)
    pub zerion: ZerionConfig,

    /// DexScreener API configuration
    pub dexscreener: DexScreenerConfig,


    /// API server configuration
    pub api: ApiConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// Token discovery configuration
    pub discovery: DiscoveryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultichainConfig {
    /// List of enabled blockchain networks
    /// Supported values: solana, ethereum, binance-smart-chain, base
    /// Additional chains can be added as supported by Zerion API
    pub enabled_chains: Vec<String>,

    /// Default chain for operations when not specified
    pub default_chain: String,

    /// Whether to fetch transactions from all chains simultaneously
    pub fetch_all_chains: bool,
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
    /// BirdEye API key (used for trending tokens and top traders discovery only)
    pub api_key: String,

    /// BirdEye API base URL
    pub api_base_url: String,

    /// Request timeout in seconds
    pub request_timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerionConfig {
    /// Zerion API key
    pub api_key: String,

    /// Zerion API base URL
    pub api_base_url: String,

    /// Request timeout in seconds
    pub request_timeout_seconds: u64,

    /// Enable Zerion as data source
    pub enabled: bool,

    /// Default currency for transaction data (e.g., "usd")
    pub default_currency: String,

    /// Default maximum transactions per wallet
    pub default_max_transactions: u32,

    /// Rate limit delay between requests in milliseconds
    pub rate_limit_delay_ms: u64,

    /// Page size for transaction requests (default: 100)
    pub page_size: u32,

    /// Operation types to filter (e.g., "trade,send")
    pub operation_types: String,

    /// Chain IDs to filter (e.g., "solana,ethereum,binance-smart-chain,base")
    /// Primary supported chains: solana, ethereum, binance-smart-chain, base
    /// Additional chains supported by Zerion: arbitrum, optimism, polygon, avalanche, etc.
    pub chain_ids: String,

    /// Trash filter setting ("only_non_trash", "no_filter", "only_trash")
    pub trash_filter: String,
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

    // Browser automation settings for scraping trending tokens
    /// Chrome executable path (None = use system default)
    pub chrome_executable_path: Option<String>,

    /// Run browser in headless mode
    pub headless_mode: bool,

    /// Enable anti-detection features
    pub anti_detection_enabled: bool,

    /// ScraperAPI key for bypassing Cloudflare (always enabled)
    pub scraperapi_key: String,
}

// PnLConfig struct removed - all fields were unused in actual P&L processing
// These were legacy configs from the JavaScript version that were never implemented


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API server host
    pub host: String,

    /// API server port
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Discovery cycle interval in seconds
    pub cycle_interval_seconds: Option<u64>,

    /// Token cache duration in hours (how long to skip processing same token)
    pub token_cache_duration_hours: Option<i64>,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system: SystemSettings {
                debug_mode: false,
                process_loop_ms: 60000,
                pnl_parallel_batch_size: Some(10),
            },
            multichain: MultichainConfig {
                enabled_chains: vec![
                    "solana".to_string(),
                    "ethereum".to_string(),
                    "binance-smart-chain".to_string(), // Use Zerion's official chain ID
                    "base".to_string(),
                ],
                default_chain: "solana".to_string(),
                fetch_all_chains: true,
            },
            redis: RedisConfig {
                url: "redis://127.0.0.1:6379".to_string(),
                default_lock_ttl_seconds: 600,
            },
            zerion: ZerionConfig {
                api_key: "".to_string(), // Must be set in .env or config file
                api_base_url: "https://api.zerion.io/v1".to_string(),
                request_timeout_seconds: 30,
                enabled: false, // Disabled by default until API key is provided
                default_currency: "usd".to_string(),
                default_max_transactions: 1000,
                rate_limit_delay_ms: 200, // Conservative rate limiting
                page_size: 100,
                operation_types: "trade,send,receive".to_string(),
                chain_ids: "solana,ethereum,binance-smart-chain,base".to_string(),
                trash_filter: "only_non_trash".to_string(),
            },
            birdeye: BirdEyeConfig {
                api_key: "".to_string(), // Must be set in .env or config file
                api_base_url: "https://public-api.birdeye.so".to_string(),
                request_timeout_seconds: 30,
            },
            dexscreener: DexScreenerConfig {
                api_base_url: "https://api.dexscreener.com".to_string(),
                request_timeout_seconds: 30,
                rate_limit_delay_ms: 1000, // 1 request per second (60 req/min)
                max_retries: 3,
                enabled: true,           // Enable DexScreener by default
                min_boost_amount: 100.0, // Minimum boost amount to consider
                max_boosted_tokens: 20,  // Max boosted tokens per endpoint
                // Browser automation defaults
                chrome_executable_path: None, // Use system default Chrome
                headless_mode: true,          // Run in headless mode by default
                anti_detection_enabled: true, // Enable stealth mode by default
                scraperapi_key: "".to_string(), // Must be set in config.toml
            },
            api: ApiConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            database: DatabaseConfig {
                postgres_url: "postgresql://postgres:password@localhost:5432/wallet_analyzer"
                    .to_string(),
                enabled: true,
            },
            discovery: DiscoveryConfig {
                cycle_interval_seconds: Some(60), // Default 1 minute cycle interval
                token_cache_duration_hours: Some(1), // Default 1 hour cache duration
            },
        }
    }
}

impl BirdEyeConfig {
    /// Validate BirdEye configuration - simplified for portfolio-only usage
    pub fn validate(&self) -> Result<()> {
        if self.api_key.is_empty() {
            return Err(ConfigurationError::InvalidValue(
                "BirdEye API key is required".to_string(),
            ));
        }

        if self.request_timeout_seconds == 0 {
            return Err(ConfigurationError::InvalidValue(
                "Request timeout must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

impl ZerionConfig {
    /// Validate Zerion configuration
    pub fn validate(&self) -> Result<()> {
        if self.enabled && self.api_key.is_empty() {
            return Err(ConfigurationError::InvalidValue(
                "Zerion API key is required when Zerion is enabled".to_string(),
            ));
        }

        if self.request_timeout_seconds == 0 {
            return Err(ConfigurationError::InvalidValue(
                "Request timeout must be greater than 0".to_string(),
            ));
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
            info!(
                "Loading configuration from: {}",
                config_path.as_ref().display()
            );
            config_builder = config_builder.add_source(File::from(config_path.as_ref()));
        } else {
            debug!("Config file not found, using defaults and environment variables");
        }

        // Add environment variables with prefix
        config_builder = config_builder.add_source(
            Environment::with_prefix("PNL")
                .try_parsing(true)
                .separator("__")
                .list_separator(","),
        );

        let config = config_builder.build()?;

        // Debug: Print the raw config values to understand the parsing issue
        if let Ok(birdeye_key) = config.get::<String>("birdeye.api_key") {
            debug!("Raw birdeye.api_key value: '{}'", birdeye_key);
        } else {
            debug!("Failed to get birdeye.api_key as string");
        }

        let mut system_config: SystemConfig = config.try_deserialize()?;

        // Normalize chain_ids in configuration to ensure Zerion compatibility
        let original_chain_ids = system_config.zerion.chain_ids.clone();
        let chain_list: Vec<String> = original_chain_ids
            .split(',')
            .map(|chain| {
                normalize_chain_for_zerion(chain.trim())
                    .unwrap_or_else(|_| {
                        warn!("Skipping unsupported chain in config: '{}'", chain.trim());
                        chain.trim().to_string() // Keep original if normalization fails
                    })
            })
            .collect();

        system_config.zerion.chain_ids = chain_list.join(",");

        if original_chain_ids != system_config.zerion.chain_ids {
            info!(
                "Normalized chain_ids in configuration: '{}' -> '{}'",
                original_chain_ids, system_config.zerion.chain_ids
            );
        }

        // Also normalize multichain.enabled_chains
        let original_enabled_chains = system_config.multichain.enabled_chains.clone();
        system_config.multichain.enabled_chains = original_enabled_chains
            .iter()
            .map(|chain| {
                normalize_chain_for_zerion(chain.trim())
                    .unwrap_or_else(|_| {
                        warn!("Skipping unsupported chain in multichain config: '{}'", chain.trim());
                        chain.clone() // Keep original if normalization fails
                    })
            })
            .collect();

        if original_enabled_chains != system_config.multichain.enabled_chains {
            info!(
                "Normalized enabled_chains in configuration: {:?} -> {:?}",
                original_enabled_chains, system_config.multichain.enabled_chains
            );
        }

        // Normalize default_chain as well
        let original_default_chain = system_config.multichain.default_chain.clone();
        system_config.multichain.default_chain = normalize_chain_for_zerion(&original_default_chain)
            .unwrap_or_else(|_| {
                warn!("Using original default_chain as normalization failed: '{}'", original_default_chain);
                original_default_chain.clone()
            });

        if original_default_chain != system_config.multichain.default_chain {
            info!(
                "Normalized default_chain in configuration: '{}' -> '{}'",
                original_default_chain, system_config.multichain.default_chain
            );
        }

        // Validate configuration
        system_config.validate()?;

        Ok(system_config)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate individual components
        self.birdeye.validate()?;
        self.zerion.validate()?;

        if self.api.port == 0 {
            return Err(ConfigurationError::InvalidValue(
                "API port cannot be 0".to_string(),
            ));
        }

        // Validate required API keys
        if self.zerion.api_key.is_empty() {
            return Err(ConfigurationError::InvalidValue(
                "Zerion API key is required".to_string(),
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

/// Normalize chain names to Zerion API compatible format
/// This function ensures that no matter what format chain names come in,
/// they are standardized before reaching the Zerion API
pub fn normalize_chain_for_zerion(input: &str) -> std::result::Result<String, String> {
    match input.trim().to_lowercase().as_str() {
        "solana" | "sol" => Ok("solana".to_string()),
        "ethereum" | "eth" => Ok("ethereum".to_string()),
        "base" => Ok("base".to_string()),
        "binance" | "bsc" | "binance-smart-chain" | "bnb" | "binance smart chain" => {
            Ok("binance-smart-chain".to_string())
        },
        _ => Err(format!("Unsupported chain: '{}'", input))
    }
}

/// Normalize chain names to BirdEye API compatible format
/// BirdEye uses different chain identifiers than Zerion API
/// - Zerion: "binance-smart-chain" -> BirdEye: "bsc"
/// - Zerion: "ethereum" -> BirdEye: "ethereum"
/// - Zerion: "base" -> BirdEye: "base"
/// - Zerion: "solana" -> BirdEye: "solana"
pub fn normalize_chain_for_birdeye(input: &str) -> std::result::Result<String, String> {
    match input.trim().to_lowercase().as_str() {
        "solana" | "sol" => Ok("solana".to_string()),
        "ethereum" | "eth" => Ok("ethereum".to_string()),
        "base" => Ok("base".to_string()),
        "binance" | "bsc" | "binance-smart-chain" | "bnb" | "binance smart chain" => {
            Ok("bsc".to_string())
        },
        _ => Err(format!("Unsupported chain for BirdEye: '{}'", input))
    }
}

/// Denormalize chain names from Zerion format to frontend format
/// This converts backend internal chain names to frontend-friendly names
/// - Backend: "binance-smart-chain" -> Frontend: "bsc"
/// - Backend: "ethereum" -> Frontend: "ethereum"
/// - Backend: "base" -> Frontend: "base"
/// - Backend: "solana" -> Frontend: "solana"
pub fn denormalize_chain_for_frontend(input: &str) -> String {
    match input.trim().to_lowercase().as_str() {
        "binance-smart-chain" => "bsc".to_string(),
        "ethereum" => "ethereum".to_string(),
        "base" => "base".to_string(),
        "solana" => "solana".to_string(),
        // For any other value, return as-is
        _ => input.to_string(),
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
        Self::new().unwrap_or_else(|_| Self {
            config: SystemConfig::default(),
        })
    }
}

