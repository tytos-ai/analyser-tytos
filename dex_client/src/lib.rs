// BirdEye Client - Modern API-based Trading Data Discovery
// Provides clean, high-quality trading data and wallet discovery

pub mod birdeye_client;

// Re-export BirdEyeConfig from config_manager
pub use config_manager::BirdEyeConfig;

pub use birdeye_client::{
    BirdEyeClient, BirdEyeError, TopTrader, TopTraderFilter, 
    TrendingToken, TrendingTokenFilter, TraderTransaction, HistoricalPriceResponse, PriceResponse,
    GeneralTraderTransaction, GeneralTraderTransactionsResponse, TokenTransactionSide
};

// Legacy compatibility layer - all functionality moved to BirdEye
// This will be fully removed once configuration migration is complete

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DexClientError {
    #[error("BirdEye API error: {0}")]
    BirdEye(#[from] BirdEyeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        // BirdEyeConfig now comes from config_manager with different defaults
        let config = BirdEyeConfig {
            api_key: "test".to_string(),
            api_base_url: "https://public-api.birdeye.so".to_string(),
            request_timeout_seconds: 30,
            price_cache_ttl_seconds: 60,
            rate_limit_per_second: 100,
            max_traders_per_token: 10,
            max_transactions_per_trader: 100,
            max_token_rank: 1000,
        };
        assert_eq!(config.api_base_url, "https://public-api.birdeye.so");
        assert_eq!(config.request_timeout_seconds, 30);
    }

    #[test]
    fn test_trending_token_filter() {
        let filter = TrendingTokenFilter::default();
        assert_eq!(filter.min_volume_usd, Some(10000.0));
        assert_eq!(filter.min_price_change_24h, Some(5.0));
        assert_eq!(filter.max_tokens, Some(50));
    }

    #[test]
    fn test_top_trader_filter() {
        let filter = TopTraderFilter::default();
        assert_eq!(filter.min_volume_usd, 1000.0);
        assert_eq!(filter.min_trades, 5);
        assert_eq!(filter.min_win_rate, Some(60.0));
    }
}