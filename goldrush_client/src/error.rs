use thiserror::Error;

#[derive(Error, Debug)]
pub enum GoldRushError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON parsing failed: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("API error: {message}")]
    ApiError { message: String },

    #[error("Parse error: {message}")]
    ParseError { message: String },

    #[error("Invalid chain: {chain}")]
    InvalidChain { chain: String },

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Authentication failed")]
    AuthError,

    #[error("Invalid wallet address: {address}")]
    InvalidAddress { address: String },
}