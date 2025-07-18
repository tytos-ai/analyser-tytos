// BirdEye Client - Modern API-based Trading Data Discovery
// Provides clean, high-quality trading data and wallet discovery

pub mod birdeye_client;
pub mod dexscreener_client;
pub mod helius_client;
pub mod price_fetching_service;
pub mod token_metadata_service;

// Re-export configs from config_manager
pub use config_manager::{BirdEyeConfig, HeliusConfig, DexScreenerConfig, DataSource};

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

pub use helius_client::{
    HeliusClient, HeliusError, HeliusTransaction, HeliusAccountData,
    HeliusTokenBalanceChange, HeliusRawTokenAmount, HeliusTokenTransfer,
    HeliusNativeTransfer, HeliusEvents, HeliusSwapEvent, HeliusTokenIO,
    HeliusNativeIO, HeliusInnerSwap, HeliusProgramInfo,
    HeliusTransactionError, HeliusInstruction, HeliusInnerInstruction,
    TokenChange, TokenChangeWithPrice, TokenOperation,
};

pub use price_fetching_service::{
    PriceFetchingService, PriceFetchingError, JupiterPriceResponse, JupiterPriceData,
    JupiterHistoricalPriceResponse, JupiterHistoricalPriceData,
    BirdeyeHistoricalPriceResponse, BirdeyeHistoricalPriceData, BirdeyeHistoricalPriceItem,
};

pub use token_metadata_service::{
    TokenMetadataService, TokenMetadataError, TokenMetadata, TokenExtensions,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DexClientError {
    #[error("BirdEye API error: {0}")]
    BirdEye(#[from] BirdEyeError),
    #[error("Helius API error: {0}")]
    Helius(#[from] HeliusError),
    #[error("Price fetching error: {0}")]
    PriceFetching(#[from] PriceFetchingError),
}

