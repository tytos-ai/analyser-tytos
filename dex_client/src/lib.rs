// BirdEye Client - Modern API-based Trading Data Discovery
// Provides clean, high-quality trading data and wallet discovery

pub mod birdeye_client;

pub use birdeye_client::{
    BirdEyeClient, BirdEyeConfig, BirdEyeError, TopTrader, TopTraderFilter, 
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
        let config = BirdEyeConfig::default();
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