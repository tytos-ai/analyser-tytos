use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Token balance information (compatible with existing P&L engine interface)
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

/// Zerion API response structures
#[derive(Debug, Deserialize)]
struct ZerionPositionsResponse {
    data: Vec<ZerionPosition>,
}

#[derive(Debug, Deserialize)]
struct ZerionPosition {
    #[serde(rename = "type")]
    _position_type: Option<String>,
    id: Option<String>,
    attributes: ZerionPositionAttributes,
}

#[derive(Debug, Deserialize)]
struct ZerionPositionAttributes {
    quantity: ZerionQuantity,
    value: Option<f64>,
    price: Option<f64>,
    fungible_info: ZerionFungibleInfo,
}

#[derive(Debug, Deserialize)]
struct ZerionQuantity {
    int: String,
    decimals: u8,
    float: f64,
    _numeric: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZerionFungibleInfo {
    name: String,
    symbol: String,
    implementations: Vec<ZerionImplementation>,
}

#[derive(Debug, Deserialize)]
struct ZerionImplementation {
    chain_id: String,
    address: String,
    _decimals: Option<u8>,
}

/// Zerion-based balance fetcher that maintains compatibility with existing P&L engine interface
#[derive(Clone)]
pub struct ZerionBalanceFetcher {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ZerionBalanceFetcher {
    /// Create a new Zerion balance fetcher
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = base_url.unwrap_or_else(|| "https://api.zerion.io".to_string());

        Self {
            client,
            api_key,
            base_url,
        }
    }

    /// Generate Basic Auth header for Zerion API
    fn generate_auth_header(&self) -> String {
        // Zerion expects "Basic base64(api_key:)" format
        let auth_string = format!("{}:", self.api_key);
        let encoded = STANDARD.encode(auth_string.as_bytes());
        format!("Basic {}", encoded)
    }

    /// Determine possible chain IDs from wallet address format
    /// Returns all chains that could support this address format
    fn detect_possible_chains(wallet_address: &str) -> Vec<&'static str> {
        if Self::is_evm_address(wallet_address) {
            // EVM-compatible chains (Ethereum, BSC, Base all use same address format)
            vec!["ethereum", "binance-smart-chain", "base"]
        } else if Self::is_solana_address(wallet_address) {
            vec!["solana"]
        } else {
            // Unknown format, try all supported chains
            vec!["solana", "ethereum", "binance-smart-chain", "base"]
        }
    }

    /// Determine primary chain ID from wallet address format (legacy compatibility)
    fn detect_chain(wallet_address: &str) -> &'static str {
        // Return the first/primary chain for backward compatibility
        Self::detect_possible_chains(wallet_address).first().unwrap_or(&"solana")
    }

    /// Check if address is a valid EVM address (Ethereum, BSC, Base)
    fn is_evm_address(address: &str) -> bool {
        if !address.starts_with("0x") && !address.starts_with("0X") {
            return false;
        }

        // Must be exactly 42 characters (0x + 40 hex characters)
        if address.len() != 42 {
            return false;
        }

        // Check if all characters after 0x are valid hex
        address[2..].chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Check if address is a valid Solana address
    fn is_solana_address(address: &str) -> bool {
        // Solana addresses are typically 32-44 characters, base58 encoded
        if address.len() < 32 || address.len() > 44 {
            return false;
        }

        // Check if all characters are valid base58 (Bitcoin alphabet)
        // Base58 alphabet: 123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz
        // (excludes 0, O, I, l to avoid confusion)
        address.chars().all(|c| {
            matches!(c,
                '1'..='9' | 'A'..='H' | 'J'..='N' | 'P'..='Z' |
                'a'..='k' | 'm'..='z'
            )
        })
    }

    /// Validate address format for a specific chain
    pub fn validate_address_for_chain(address: &str, chain_id: &str) -> bool {
        match chain_id {
            "ethereum" | "binance-smart-chain" | "base" => Self::is_evm_address(address),
            "solana" => Self::is_solana_address(address),
            _ => false, // Unknown chain
        }
    }

    /// Fetch wallet balances from Zerion API for a specific chain
    pub async fn fetch_wallet_balances_for_chain(
        &self,
        wallet_address: &str,
        chain_id: &str,
    ) -> Result<HashMap<String, TokenBalance>> {
        let url = format!("{}/v1/wallets/{}/positions/", self.base_url, wallet_address);

        debug!("Fetching wallet balances from Zerion for: {} (chain: {})", wallet_address, chain_id);

        let response = self
            .client
            .get(&url)
            .header("accept", "application/json")
            .header("authorization", self.generate_auth_header())
            .query(&[
                ("filter[positions]", "only_simple"),
                ("currency", "usd"),
                ("filter[trash]", "only_non_trash"),
                ("filter[chain_ids]", chain_id),
                ("sort", "value"),
            ])
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send request to Zerion API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Zerion API error ({}): {}", status, error_text));
        }

        let positions_response: ZerionPositionsResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Zerion positions response: {}", e))?;

        let mut balances = HashMap::new();

        for position in positions_response.data {
            // Extract token implementation for the specified chain
            let implementation = position.attributes.fungible_info.implementations
                .iter()
                .find(|impl_| impl_.chain_id == chain_id)
                .ok_or_else(|| anyhow!(
                    "No {} implementation found for token {}",
                    chain_id,
                    position.attributes.fungible_info.symbol
                ))?;

            // Parse quantity from string representation
            let balance_raw = position.attributes.quantity.int
                .parse::<u128>()
                .map_err(|e| anyhow!("Failed to parse balance '{}': {}", position.attributes.quantity.int, e))?;

            // Convert to Decimal with proper decimal places
            let decimals = position.attributes.quantity.decimals;
            let divisor = 10u128.pow(decimals as u32);
            let balance = Decimal::from(balance_raw) / Decimal::from(divisor);

            // Use the float value directly as ui_amount for consistency
            let ui_amount = Decimal::try_from(position.attributes.quantity.float)
                .unwrap_or(balance);

            // Convert price and value from f64 to Decimal
            let price_usd = position.attributes.price
                .and_then(|p| Decimal::try_from(p).ok());

            let value_usd = position.attributes.value
                .and_then(|v| Decimal::try_from(v).ok());

            let token_balance = TokenBalance {
                address: implementation.address.clone(),
                balance,
                decimals,
                symbol: position.attributes.fungible_info.symbol.clone(),
                name: position.attributes.fungible_info.name.clone(),
                ui_amount,
                price_usd,
                value_usd,
            };

            // Use the token address as the key for HashMap lookup consistency
            balances.insert(implementation.address.clone(), token_balance);
        }

        debug!(
            "Retrieved {} token balances from Zerion for wallet {} on chain {}",
            balances.len(),
            wallet_address,
            chain_id
        );
        Ok(balances)
    }

    /// Fetch wallet balances from Zerion API (auto-detect chain)
    pub async fn fetch_wallet_balances(
        &self,
        wallet_address: &str,
    ) -> Result<HashMap<String, TokenBalance>> {
        let chain_id = Self::detect_chain(wallet_address);
        self.fetch_wallet_balances_for_chain(wallet_address, chain_id).await
    }

    /// Get balance for a specific token in the wallet
    pub async fn get_token_balance(
        &self,
        wallet_address: &str,
        token_address: &str,
    ) -> Result<Option<TokenBalance>> {
        let balances = self.fetch_wallet_balances(wallet_address).await?;
        Ok(balances.get(token_address).cloned())
    }

    /// Get the current balance for a token, returning zero if not found
    pub async fn get_token_ui_amount(
        &self,
        wallet_address: &str,
        token_address: &str,
    ) -> Result<Decimal> {
        match self
            .get_token_balance(wallet_address, token_address)
            .await?
        {
            Some(balance) => Ok(balance.ui_amount),
            None => {
                debug!(
                    "Token {} not found in wallet {} (Zerion), returning zero balance",
                    token_address, wallet_address
                );
                Ok(Decimal::ZERO)
            }
        }
    }

    /// Get balances for multiple tokens at once
    pub async fn get_multiple_token_balances(
        &self,
        wallet_address: &str,
        token_addresses: &[String],
    ) -> Result<HashMap<String, Decimal>> {
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
    pub async fn has_significant_balances(
        &self,
        wallet_address: &str,
        min_usd_value: Option<Decimal>,
    ) -> Result<bool> {
        let balances = self.fetch_wallet_balances(wallet_address).await?;
        let threshold = min_usd_value.unwrap_or_else(|| Decimal::new(1, 2)); // 0.01

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

