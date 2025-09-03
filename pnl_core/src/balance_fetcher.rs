use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rust_decimal::Decimal;
use anyhow::{Result, anyhow};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub address: String,
    pub balance: Decimal,
    pub decimals: u8,
    pub symbol: String,
    pub name: String,
    pub ui_amount: Decimal,
    pub price_usd: Option<Decimal>,
    pub value_usd: Option<Decimal>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BirdeyeBalanceResponse {
    pub success: bool,
    pub data: BirdeyeBalanceData,
}

#[derive(Debug, Serialize, Deserialize)]
struct BirdeyeBalanceData {
    pub wallet: String,
    #[serde(rename = "totalUsd")]
    pub total_usd: Decimal,
    pub items: Vec<BirdeyeBalanceItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BirdeyeBalanceItem {
    pub address: String,
    pub balance: u64,
    pub decimals: u8,
    pub symbol: String,
    pub name: String,
    #[serde(rename = "uiAmount")]
    pub ui_amount: f64,
    #[serde(rename = "priceUsd")]
    pub price_usd: Option<f64>,
    #[serde(rename = "valueUsd")]
    pub value_usd: Option<f64>,
    #[serde(rename = "chainId")]
    pub chain_id: String,
    pub icon: Option<String>,
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
}

pub struct BalanceFetcher {
    client: Client,
    api_key: String,
    base_url: String,
}

impl BalanceFetcher {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        let base_url = base_url.unwrap_or_else(|| "https://public-api.birdeye.so".to_string());
        
        Self {
            client,
            api_key,
            base_url,
        }
    }

    /// Fetch wallet balances from Birdeye API
    pub async fn fetch_wallet_balances(&self, wallet_address: &str) -> Result<HashMap<String, TokenBalance>> {
        let url = format!("{}/v1/wallet/token_list", self.base_url);
        
        debug!("Fetching wallet balances for: {}", wallet_address);
        
        let response = self.client
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .header("x-chain", "solana")
            .header("accept", "application/json")
            .query(&[("wallet", wallet_address), ("ui_amount_mode", "scaled")])
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send request to Birdeye API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Birdeye API error ({}): {}", status, error_text));
        }

        let balance_response: BirdeyeBalanceResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Birdeye balance response: {}", e))?;

        if !balance_response.success {
            return Err(anyhow!("Birdeye API returned success=false"));
        }

        let mut balances = HashMap::new();
        
        for item in balance_response.data.items {
            let balance = Decimal::from(item.balance);
            let ui_amount = Decimal::try_from(item.ui_amount)
                .map_err(|e| anyhow!("Failed to convert ui_amount '{}': {}", item.ui_amount, e))?;
            
            let price_usd = if let Some(price_f64) = item.price_usd {
                Some(Decimal::try_from(price_f64)
                    .map_err(|e| anyhow!("Failed to convert price_usd '{}': {}", price_f64, e))?)
            } else {
                None
            };
            
            let value_usd = if let Some(value_f64) = item.value_usd {
                Some(Decimal::try_from(value_f64)
                    .map_err(|e| anyhow!("Failed to convert value_usd '{}': {}", value_f64, e))?)
            } else {
                None
            };

            let token_balance = TokenBalance {
                address: item.address.clone(),
                balance,
                decimals: item.decimals,
                symbol: item.symbol,
                name: item.name,
                ui_amount,
                price_usd,
                value_usd,
            };

            balances.insert(item.address, token_balance);
        }

        debug!("Retrieved {} token balances for wallet {}", balances.len(), wallet_address);
        Ok(balances)
    }

    /// Get balance for a specific token in the wallet
    pub async fn get_token_balance(&self, wallet_address: &str, token_address: &str) -> Result<Option<TokenBalance>> {
        let balances = self.fetch_wallet_balances(wallet_address).await?;
        Ok(balances.get(token_address).cloned())
    }

    /// Get the current balance for a token, returning zero if not found
    pub async fn get_token_ui_amount(&self, wallet_address: &str, token_address: &str) -> Result<Decimal> {
        match self.get_token_balance(wallet_address, token_address).await? {
            Some(balance) => Ok(balance.ui_amount),
            None => {
                debug!("Token {} not found in wallet {}, returning zero balance", token_address, wallet_address);
                Ok(Decimal::ZERO)
            }
        }
    }

    /// Get balances for multiple tokens at once
    pub async fn get_multiple_token_balances(&self, wallet_address: &str, token_addresses: &[String]) -> Result<HashMap<String, Decimal>> {
        let all_balances = self.fetch_wallet_balances(wallet_address).await?;
        let mut result = HashMap::new();
        
        for token_address in token_addresses {
            let balance = all_balances
                .get(token_address)
                .map(|b| b.ui_amount)
                .unwrap_or(Decimal::ZERO);
            result.insert(token_address.clone(), balance);
        }
        
        Ok(result)
    }

    /// Check if wallet has any significant balances (> $0.01 USD)
    pub async fn has_significant_balances(&self, wallet_address: &str, min_usd_value: Option<Decimal>) -> Result<bool> {
        let balances = self.fetch_wallet_balances(wallet_address).await?;
        let threshold = min_usd_value.unwrap_or(Decimal::from_str_exact("0.01")?);
        
        for balance in balances.values() {
            if let Some(value_usd) = balance.value_usd {
                if value_usd >= threshold {
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }

    /// Get total USD value of all balances in wallet
    pub async fn get_total_wallet_value_usd(&self, wallet_address: &str) -> Result<Decimal> {
        let balances = self.fetch_wallet_balances(wallet_address).await?;
        
        let mut total = Decimal::ZERO;
        for balance in balances.values() {
            if let Some(value_usd) = balance.value_usd {
                total += value_usd;
            }
        }
        
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_balance_fetcher_creation() {
        let fetcher = BalanceFetcher::new("test_key".to_string(), None);
        assert_eq!(fetcher.api_key, "test_key");
        assert_eq!(fetcher.base_url, "https://public-api.birdeye.so");
    }

    #[tokio::test]
    async fn test_balance_fetcher_with_custom_url() {
        let fetcher = BalanceFetcher::new("test_key".to_string(), Some("https://custom.api.com".to_string()));
        assert_eq!(fetcher.base_url, "https://custom.api.com");
    }
}