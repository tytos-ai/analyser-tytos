// BirdEye Price Client - Modern API-based Token Pricing
// Provides clean, high-quality token pricing via BirdEye API

pub mod birdeye_price_fetcher;

pub use birdeye_price_fetcher::{BirdEyePriceFetcher, BirdEyePriceError};

// Legacy compatibility exports - all functionality moved to BirdEye
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PriceClientError {
    #[error("BirdEye price error: {0}")]
    BirdEye(#[from] BirdEyePriceError),
}