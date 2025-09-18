// BirdEye Client - Modern API-based Trading Data Discovery
// Provides clean, high-quality trading data and wallet discovery

pub mod birdeye_client;
pub mod dexscreener_client;
pub mod history_trait_impl;
pub mod price_enricher;

// Re-export configs from config_manager
pub use config_manager::{BirdEyeConfig, DexScreenerConfig};

pub use birdeye_client::{
    BirdEyeClient,
    BirdEyeError,
    GainerLoser,
    GeneralTraderTransactionsResponse,
    NewListingToken,
    NewListingTokenFilter,
    TopTrader,
    TopTraderFilter,
    TrendingToken,
    TrendingTokenFilter,
    WalletPortfolioData,
    // Portfolio API types
    WalletPortfolioResponse,
    WalletTokenBalance,
};
pub use pnl_core::{GeneralTraderTransaction, TokenTransactionSide};

pub use dexscreener_client::{
    DexScreenerBoostedResponse, DexScreenerBoostedToken, DexScreenerClient,
    DexScreenerConfig as DexScreenerClientConfig, DexScreenerError, DexScreenerTrendingToken,
};

pub use price_enricher::{
    EnrichedBalanceChange, EnrichedTransaction, PriceEnricher, PriceStrategy,
};

use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DexClientError {
    #[error("BirdEye API error: {0}")]
    BirdEye(#[from] BirdEyeError),
}

/// Multi-chain token security check
/// Returns true if token is safe to process, false if it's a honeypot or high-risk
/// Uses Honeypot.is for Ethereum/BSC/Base and SolSniffer for Solana
pub async fn is_token_safe(token_address: &str, chain: &str) -> bool {
    match chain.to_lowercase().as_str() {
        "solana" => {
            return check_solana_token_safety(token_address).await;
        }
        "ethereum" | "eth" => 1,
        "bsc" | "binance" => 56,
        "base" => 8453,
        _ => return true, // Unknown chain, allow by default
    };

    // Map chain to Honeypot.is chain ID for non-Solana chains
    let chain_id = match chain.to_lowercase().as_str() {
        "ethereum" | "eth" => 1,
        "bsc" | "binance" => 56,
        "base" => 8453,
        _ => return true,
    };

    // Call Honeypot.is API
    let client = reqwest::Client::new();
    let url = "https://api.honeypot.is/v2/IsHoneypot";

    match client
        .get(url)
        .query(&[
            ("address", token_address),
            ("chainID", &chain_id.to_string()),
        ])
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                // Check if it's a honeypot
                if let Some(is_honeypot) = json["honeypotResult"]["isHoneypot"].as_bool() {
                    if is_honeypot {
                        tracing::warn!("🍯 Honeypot detected: {} on {}", token_address, chain);
                        return false; // It's a honeypot, not safe
                    }
                }

                // Check risk level (reject high risk)
                if let Some(risk) = json["summary"]["risk"].as_str() {
                    if risk == "honeypot" || risk == "very_high" || risk == "high" {
                        tracing::warn!(
                            "⚠️ High-risk token detected: {} on {} (risk: {})",
                            token_address,
                            chain,
                            risk
                        );
                        return false; // Too risky
                    }
                }

                // Check risk level number as backup
                if let Some(risk_level) = json["summary"]["riskLevel"].as_u64() {
                    if risk_level >= 60 {
                        // High risk threshold
                        tracing::warn!(
                            "⚠️ High-risk token detected: {} on {} (risk level: {})",
                            token_address,
                            chain,
                            risk_level
                        );
                        return false;
                    }
                }

                tracing::debug!(
                    "✅ Token passed security check: {} on {}",
                    token_address,
                    chain
                );
                true // Token passed checks
            } else {
                tracing::debug!(
                    "Failed to parse Honeypot.is response for {}, allowing token",
                    token_address
                );
                true // On parse error, allow token (don't block on API issues)
            }
        }
        Err(e) => {
            tracing::debug!(
                "Honeypot.is API error for {}: {}, allowing token",
                token_address,
                e
            );
            true // On API error, allow token (don't block discovery)
        }
    }
}

/// Check Solana token safety using SolSniffer API
/// Returns true if token is safe (both mint and freeze authorities disabled)
async fn check_solana_token_safety(token_address: &str) -> bool {
    let client = reqwest::Client::new();
    let url = "https://solsniffer.com/api/v2/tokens";

    let payload = serde_json::json!({
        "addresses": [token_address]
    });

    match client
        .post(url)
        .header("X-API-KEY", "w7axeg8gnjcq50q6b77za7gdxrwsbd")
        .header("Content-Type", "application/json")
        .json(&payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                if let Some(data) = json["data"].as_array() {
                    if let Some(token_data) = data.first() {
                        if let Some(audit_risk) = token_data["tokenData"]["auditRisk"].as_object() {
                            // Check critical security flags
                            let mint_disabled =
                                audit_risk["mintDisabled"].as_bool().unwrap_or(false);
                            let freeze_disabled =
                                audit_risk["freezeDisabled"].as_bool().unwrap_or(false);

                            // Token is unsafe if EITHER mint OR freeze authority is not disabled
                            if !mint_disabled {
                                tracing::warn!(
                                    "🚫 Solana token {} rejected: mintable (mintDisabled: false)",
                                    token_address
                                );
                                return false;
                            }
                            if !freeze_disabled {
                                tracing::warn!("🚫 Solana token {} rejected: freezable (freezeDisabled: false)", token_address);
                                return false;
                            }

                            tracing::debug!(
                                "✅ Solana token {} passed security check (mint & freeze disabled)",
                                token_address
                            );
                            return true;
                        }
                    }
                }
            }

            // If we can't parse the response properly, allow the token
            tracing::debug!(
                "Failed to parse SolSniffer response for {}, allowing token",
                token_address
            );
            true
        }
        Err(e) => {
            tracing::debug!(
                "SolSniffer API error for {}: {}, allowing token",
                token_address,
                e
            );
            true // Fail open - don't block discovery on API issues
        }
    }
}

/// Convert portfolio token balances to a map of token addresses to current USD prices
/// This enables accurate unrealized P&L calculation using real-time market prices
pub fn extract_current_prices_from_portfolio(
    portfolio: &[WalletTokenBalance],
) -> std::collections::HashMap<String, rust_decimal::Decimal> {
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;

    let mut price_map = HashMap::new();
    let mut skipped_count = 0;

    for token in portfolio {
        // Only include tokens with positive balances and valid prices
        if token.ui_amount > 0.0 {
            if token.price_usd > 0.0 {
                // Convert f64 price to Decimal for precision
                if let Ok(price_decimal) = Decimal::from_str(&token.price_usd.to_string()) {
                    price_map.insert(token.address.clone(), price_decimal);
                } else {
                    tracing::warn!(
                        "⚠️ Failed to convert price ${:.6} to Decimal for token {} ({})",
                        token.price_usd,
                        token.symbol.as_deref().unwrap_or("Unknown"),
                        token.address
                    );
                    skipped_count += 1;
                }
            } else {
                tracing::warn!("⚠️ Skipping token {} ({}) due to missing/zero price data (BirdEye API): ${:.6}",
                              token.symbol.as_deref().unwrap_or("Unknown"), token.address, token.price_usd);
                skipped_count += 1;
            }
        }
    }

    if skipped_count > 0 {
        tracing::info!("📊 Portfolio summary: {} tokens with valid prices, {} tokens skipped due to missing/invalid price data",
                       price_map.len(), skipped_count);
    } else {
        tracing::debug!(
            "📊 Extracted {} current prices from portfolio",
            price_map.len()
        );
    }
    price_map
}

/// Extract actual current balances from portfolio for balance reconciliation
/// Returns map of token address to (balance, symbol, decimals)
pub fn extract_current_balances_from_portfolio(
    portfolio: &[WalletTokenBalance],
) -> std::collections::HashMap<String, (rust_decimal::Decimal, String, u32)> {
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;

    let mut balance_map = HashMap::new();

    for token in portfolio {
        // Only include tokens with positive balances
        if token.ui_amount > 0.0 {
            // Convert f64 ui_amount to Decimal for precision
            if let Ok(balance_decimal) = Decimal::from_str(&token.ui_amount.to_string()) {
                balance_map.insert(
                    token.address.clone(),
                    (
                        balance_decimal,
                        token
                            .symbol
                            .clone()
                            .unwrap_or_else(|| "UNKNOWN".to_string()),
                        token.decimals,
                    ),
                );
            } else {
                tracing::warn!(
                    "Failed to convert balance {} to Decimal for token {}",
                    token.ui_amount,
                    token.address
                );
            }
        }
    }

    tracing::debug!(
        "Extracted {} current balances from portfolio",
        balance_map.len()
    );
    balance_map
}
