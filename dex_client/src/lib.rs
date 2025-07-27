// BirdEye Client - Modern API-based Trading Data Discovery
// Provides clean, high-quality trading data and wallet discovery

pub mod birdeye_client;
pub mod dexscreener_client;
pub mod token_metadata_service;

// Re-export configs from config_manager
pub use config_manager::{BirdEyeConfig, DexScreenerConfig};

pub use pnl_core::{GeneralTraderTransaction, TokenTransactionSide};
pub use birdeye_client::{
    BirdEyeClient, BirdEyeError, TrendingToken, TopTrader, GainerLoser,
    GeneralTraderTransactionsResponse,
    TrendingTokenFilter, TopTraderFilter,
    NewListingToken, NewListingTokenFilter,
};

pub use dexscreener_client::{
    DexScreenerClient, DexScreenerError, DexScreenerBoostedToken,
    DexScreenerBoostedResponse, DexScreenerConfig as DexScreenerClientConfig,
};


pub use token_metadata_service::{
    TokenMetadataService, TokenMetadataError, TokenMetadata, TokenExtensions,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DexClientError {
    #[error("BirdEye API error: {0}")]
    BirdEye(#[from] BirdEyeError),
}

