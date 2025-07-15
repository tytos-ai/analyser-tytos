// BirdEye Client - Modern API-based Trading Data Discovery
// Provides clean, high-quality trading data and wallet discovery

pub mod birdeye_client;
pub mod helius_client;
pub mod price_fetching_service;
pub mod token_metadata_service;

// Re-export configs from config_manager
pub use config_manager::{BirdEyeConfig, HeliusConfig, DataSource};

pub use pnl_core::{GeneralTraderTransaction, TokenTransactionSide};
pub use birdeye_client::{
    BirdEyeClient, BirdEyeError, TrendingToken, TopTrader,
    GeneralTraderTransactionsResponse,
    TrendingTokenFilter, TopTraderFilter,
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
            default_max_transactions: 1000,
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

    #[test]
    fn test_helius_client_creation() {
        let config = HeliusConfig {
            api_key: "test_key".to_string(),
            api_base_url: "https://api.helius.xyz/v0".to_string(),
            request_timeout_seconds: 30,
            rate_limit_ms: 100,
            max_retry_attempts: 3,
            enabled: true,
        };
        let client = HeliusClient::new(config).unwrap();
        assert!(client.is_enabled());
    }

    #[test]
    fn test_helius_transaction_parsing() {
        // Test parsing of sample Helius transaction data
        let sample_transaction = r#"
        {
            "description": "Test swap",
            "type": "SWAP",
            "source": "ORCA",
            "fee": 5000,
            "feePayer": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
            "signature": "test_signature",
            "slot": 352554780,
            "timestamp": 1752222888,
            "tokenTransfers": [],
            "nativeTransfers": [],
            "accountData": [
                {
                    "account": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "nativeBalanceChange": -9240,
                    "tokenBalanceChanges": [
                        {
                            "userAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                            "tokenAccount": "test_token_account",
                            "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                            "rawTokenAmount": {
                                "tokenAmount": "-117529128",
                                "decimals": 6
                            }
                        }
                    ]
                }
            ],
            "transactionError": null,
            "instructions": [],
            "events": {}
        }
        "#;

        let transaction: HeliusTransaction = serde_json::from_str(sample_transaction).unwrap();
        
        assert_eq!(transaction.signature, "test_signature");
        assert_eq!(transaction.transaction_type, "SWAP");
        assert_eq!(transaction.source, "ORCA");
        assert_eq!(transaction.fee, 5000);
        assert_eq!(transaction.timestamp, 1752222888);
        
        // Test account data
        assert_eq!(transaction.account_data.len(), 1);
        let account_data = &transaction.account_data[0];
        assert_eq!(account_data.account, "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa");
        assert_eq!(account_data.native_balance_change, -9240);
        assert_eq!(account_data.token_balance_changes.len(), 1);
        
        // Test token balance change
        let token_change = &account_data.token_balance_changes[0];
        assert_eq!(token_change.user_account, "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa");
        assert_eq!(token_change.mint, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
        assert_eq!(token_change.raw_token_amount.token_amount, "-117529128");
        assert_eq!(token_change.raw_token_amount.decimals, 6);
    }

    #[test]
    fn test_helius_token_balance_extraction() {
        let config = HeliusConfig {
            api_key: "test_key".to_string(),
            api_base_url: "https://api.helius.xyz/v0".to_string(),
            request_timeout_seconds: 30,
            rate_limit_ms: 100,
            max_retry_attempts: 3,
            enabled: true,
        };
        let client = HeliusClient::new(config).unwrap();
        
        // Create a test transaction with token balance changes
        let wallet = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa";
        let test_transaction = HeliusTransaction {
            signature: "test_signature".to_string(),
            timestamp: 1752222888,
            slot: 352554780,
            transaction_type: "SWAP".to_string(),
            source: "ORCA".to_string(),
            description: "Test swap".to_string(),
            fee: 5000,
            fee_payer: wallet.to_string(),
            native_transfers: vec![],
            token_transfers: vec![],
            account_data: vec![HeliusAccountData {
                account: wallet.to_string(),
                native_balance_change: -9240,
                token_balance_changes: vec![HeliusTokenBalanceChange {
                    user_account: wallet.to_string(),
                    token_account: "test_token_account".to_string(),
                    mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                    raw_token_amount: HeliusRawTokenAmount {
                        token_amount: "-117529128".to_string(),
                        decimals: 6,
                    },
                }],
            }],
            events: None,
            transaction_error: None,
            instructions: vec![],
        };
        
        let transactions = vec![test_transaction];
        let token_changes = client.extract_token_balance_changes(&transactions, wallet);
        
        // Should have 2 changes: 1 token change + 1 SOL change
        assert_eq!(token_changes.len(), 2);
        
        // Check token change
        let token_change = token_changes.iter().find(|c| c.mint == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
        assert_eq!(token_change.operation, TokenOperation::Sell);
        assert_eq!(token_change.raw_amount, -117529128);
        assert_eq!(token_change.decimals, 6);
        
        // Check SOL change
        let sol_change = token_changes.iter().find(|c| c.mint == "So11111111111111111111111111111111111111112").unwrap();
        assert_eq!(sol_change.operation, TokenOperation::Sell);
        assert_eq!(sol_change.raw_amount, -9240);
    }

    #[test]
    fn test_data_source_configuration() {
        use DataSource::*;
        
        // Test simple configurations
        assert!(BirdEye.uses_birdeye());
        assert!(!BirdEye.uses_helius());
        
        assert!(!Helius.uses_birdeye());
        assert!(Helius.uses_helius());
        
        // Test Both configuration
        let both = Both {
            primary: Box::new(BirdEye),
            fallback: Box::new(Helius),
        };
        assert!(both.uses_birdeye());
        assert!(both.uses_helius());
        assert_eq!(both.primary(), &BirdEye);
        assert_eq!(both.fallback(), Some(&Helius));
        
        // Test validation
        assert!(BirdEye.validate().is_ok());
        assert!(Helius.validate().is_ok());
        assert!(both.validate().is_ok());
        
        // Test invalid Both configuration (same primary and fallback)
        let invalid_both = Both {
            primary: Box::new(BirdEye),
            fallback: Box::new(BirdEye),
        };
        assert!(invalid_both.validate().is_err());
    }

    #[test]
    fn test_helius_sample_data_deserialization() {
        // Test with actual sample data from the first transaction in helius_sample_swaps.json
        let sample_transaction = r#"
        {
            "description": "",
            "type": "SWAP",
            "source": "ORCA",
            "fee": 5000,
            "feePayer": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
            "signature": "28NqCytgvPztNQ44v3ohSi2vPLMEm6UV4H2QuD3EctwjkPP6p1eCJuhkw6HWntNghzkxgHDgkpc3L4NCxWeGrEbG",
            "slot": 352554780,
            "timestamp": 1752222888,
            "tokenTransfers": [
                {
                    "fromTokenAccount": "8VYWdU14V78rcDepwmNt54bb1aam5qVUMUpEtW8oCn1E",
                    "toTokenAccount": "4iznQFptuX2A2L5NNinFjSvnXzc2Vhd6b6w8a4nXCpqA",
                    "fromUserAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "toUserAccount": "AU971DrPyhhrpRnmEBp5pDTWL2ny7nofb5vYBjDJkR2E",
                    "tokenAmount": 117.529128,
                    "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                    "tokenStandard": "Fungible"
                }
            ],
            "nativeTransfers": [
                {
                    "fromUserAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "toUserAccount": "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
                    "amount": 4240
                }
            ],
            "accountData": [
                {
                    "account": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "nativeBalanceChange": -9240,
                    "tokenBalanceChanges": []
                },
                {
                    "account": "8VYWdU14V78rcDepwmNt54bb1aam5qVUMUpEtW8oCn1E",
                    "nativeBalanceChange": 0,
                    "tokenBalanceChanges": [
                        {
                            "userAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                            "tokenAccount": "8VYWdU14V78rcDepwmNt54bb1aam5qVUMUpEtW8oCn1E",
                            "rawTokenAmount": {
                                "tokenAmount": "-117529128",
                                "decimals": 6
                            },
                            "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                        }
                    ]
                },
                {
                    "account": "CFpaEA1rc3nQo5AfJPX4LzUzW9E4K9tXUDesjfwCoT8x",
                    "nativeBalanceChange": 0,
                    "tokenBalanceChanges": [
                        {
                            "userAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                            "tokenAccount": "CFpaEA1rc3nQo5AfJPX4LzUzW9E4K9tXUDesjfwCoT8x",
                            "rawTokenAmount": {
                                "tokenAmount": "3939432",
                                "decimals": 8
                            },
                            "mint": "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs"
                        }
                    ]
                }
            ],
            "transactionError": null,
            "instructions": [],
            "events": {}
        }
        "#;

        // Test deserialization
        let transaction: HeliusTransaction = serde_json::from_str(sample_transaction).unwrap();
        
        // Verify basic fields
        assert_eq!(transaction.signature, "28NqCytgvPztNQ44v3ohSi2vPLMEm6UV4H2QuD3EctwjkPP6p1eCJuhkw6HWntNghzkxgHDgkpc3L4NCxWeGrEbG");
        assert_eq!(transaction.transaction_type, "SWAP");
        assert_eq!(transaction.source, "ORCA");
        assert_eq!(transaction.fee, 5000);
        assert_eq!(transaction.timestamp, 1752222888);
        assert_eq!(transaction.slot, 352554780);
        
        // Verify token transfers
        assert_eq!(transaction.token_transfers.len(), 1);
        let token_transfer = &transaction.token_transfers[0];
        assert_eq!(token_transfer.token_amount, 117.529128);
        assert_eq!(token_transfer.mint, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
        assert_eq!(token_transfer.token_standard, "Fungible");
        
        // Verify native transfers
        assert_eq!(transaction.native_transfers.len(), 1);
        let native_transfer = &transaction.native_transfers[0];
        assert_eq!(native_transfer.amount, 4240);
        
        // Verify account data
        assert_eq!(transaction.account_data.len(), 3);
        
        // Check wallet-specific account data
        let wallet = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa";
        let wallet_account = transaction.account_data.iter()
            .find(|acc| acc.account == wallet)
            .unwrap();
        assert_eq!(wallet_account.native_balance_change, -9240);
        
        // Check token balance changes in other accounts for this wallet
        let mut total_token_changes = 0;
        for account in &transaction.account_data {
            for change in &account.token_balance_changes {
                if change.user_account == wallet {
                    total_token_changes += 1;
                }
            }
        }
        assert_eq!(total_token_changes, 2); // Should have 2 token changes for the wallet
        
        // Verify transaction error and instructions fields are present
        assert!(transaction.transaction_error.is_none());
        assert_eq!(transaction.instructions.len(), 0);
        
        println!("✅ Successfully deserialized and validated Helius sample transaction");
    }

    #[test]
    fn test_enhanced_helius_extraction() {
        // Test the enhanced extraction logic using comprehensive sample data
        let sample_transaction = r#"
        {
            "description": "Test swap with both tokenTransfers and accountData",
            "type": "SWAP",
            "source": "ORCA",
            "fee": 5000,
            "feePayer": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
            "signature": "enhanced_test_signature",
            "slot": 352554780,
            "timestamp": 1752222888,
            "tokenTransfers": [
                {
                    "fromTokenAccount": "8VYWdU14V78rcDepwmNt54bb1aam5qVUMUpEtW8oCn1E",
                    "toTokenAccount": "4iznQFptuX2A2L5NNinFjSvnXzc2Vhd6b6w8a4nXCpqA",
                    "fromUserAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "toUserAccount": "AU971DrPyhhrpRnmEBp5pDTWL2ny7nofb5vYBjDJkR2E",
                    "tokenAmount": 117.529128,
                    "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                    "tokenStandard": "Fungible"
                },
                {
                    "fromTokenAccount": "EYk1gzG24RSYMtfrcS4j1VLg4Lu9StefbxP6Nt9uFT5f",
                    "toTokenAccount": "CFpaEA1rc3nQo5AfJPX4LzUzW9E4K9tXUDesjfwCoT8x",
                    "fromUserAccount": "AU971DrPyhhrpRnmEBp5pDTWL2ny7nofb5vYBjDJkR2E",
                    "toUserAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "tokenAmount": 0.03939432,
                    "mint": "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs",
                    "tokenStandard": "Fungible"
                }
            ],
            "nativeTransfers": [
                {
                    "fromUserAccount": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "toUserAccount": "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
                    "amount": 4240
                }
            ],
            "accountData": [
                {
                    "account": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
                    "nativeBalanceChange": -9240,
                    "tokenBalanceChanges": []
                }
            ],
            "transactionError": null,
            "instructions": [],
            "events": {}
        }
        "#;

        let transaction: HeliusTransaction = serde_json::from_str(sample_transaction).unwrap();
        
        let config = HeliusConfig {
            api_key: "test_key".to_string(),
            api_base_url: "https://api.helius.xyz/v0".to_string(),
            request_timeout_seconds: 30,
            rate_limit_ms: 100,
            max_retry_attempts: 3,
            enabled: true,
        };
        let client = HeliusClient::new(config).unwrap();
        
        let wallet = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa";
        let token_changes = client.extract_token_balance_changes(&[transaction], wallet);
        
        // Should extract multiple token changes:
        // 1. USDC sell from tokenTransfers (fromUserAccount)
        // 2. ETHER buy from tokenTransfers (toUserAccount)
        // 3. SOL change from accountData
        assert!(token_changes.len() >= 3, "Should extract at least 3 token changes, got {}", token_changes.len());
        
        // Check for USDC sell
        let usdc_sell = token_changes.iter().find(|c| 
            c.mint == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" 
            && c.operation == TokenOperation::Sell
        );
        assert!(usdc_sell.is_some(), "Should find USDC sell transaction");
        
        // Check for ETHER buy  
        let ether_buy = token_changes.iter().find(|c| 
            c.mint == "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs" 
            && c.operation == TokenOperation::Buy
        );
        assert!(ether_buy.is_some(), "Should find ETHER buy transaction");
        
        // Check for SOL change
        let sol_change = token_changes.iter().find(|c| 
            c.mint == "So11111111111111111111111111111111111111112"
        );
        assert!(sol_change.is_some(), "Should find SOL balance change");
        
        println!("✅ Enhanced extraction test passed: extracted {} token changes", token_changes.len());
    }

    #[tokio::test]
    async fn test_jupiter_v3_integration() {
        // Test that Helius client can be configured with price fetching service
        let helius_config = HeliusConfig {
            api_key: "test_key".to_string(),
            api_base_url: "https://api.helius.xyz/v0".to_string(),
            request_timeout_seconds: 30,
            rate_limit_ms: 100,
            max_retry_attempts: 3,
            enabled: true,
        };
        
        // Create price fetching config
        let price_config = config_manager::PriceFetchingConfig {
            primary_source: "jupiter".to_string(),
            fallback_enabled: false,
            fallback_source: "jupiter".to_string(),
            jupiter_api_url: "https://lite-api.jup.ag/price/v2".to_string(),
            birdeye_api_url: "https://public-api.birdeye.so".to_string(),
            request_timeout_seconds: 30,
            price_cache_ttl_seconds: 300,
            enable_caching: false,
        };
        
        let birdeye_config = config_manager::BirdEyeConfig {
            api_key: "test_birdeye_key".to_string(),
            api_base_url: "https://public-api.birdeye.so".to_string(),
            request_timeout_seconds: 30,
            price_cache_ttl_seconds: 300,
            rate_limit_per_second: 10,
            max_traders_per_token: 100,
            max_transactions_per_trader: 50,
            default_max_transactions: 1000,
            max_token_rank: 1000,
        };
        
        // Create price fetching service
        let price_service = PriceFetchingService::new(price_config, Some(birdeye_config)).unwrap();
        
        // Create Helius client with price service
        let client = HeliusClient::new(helius_config)
            .unwrap()
            .with_price_fetching_service(price_service);
        
        // Test data: mints from our sample transaction
        let test_mints = vec![
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC
            "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs".to_string(), // ETHER
            "So11111111111111111111111111111111111111112".to_string(),   // SOL
        ];
        
        // This test will make actual API calls, so we expect it might fail in CI
        // The important thing is that the integration is properly set up
        match client.fetch_token_prices(test_mints).await {
            Ok(prices) => {
                println!("✅ Successfully fetched {} token prices", prices.len());
                // Verify we got some prices back
                assert!(!prices.is_empty(), "Should receive at least some token prices");
            }
            Err(e) => {
                // This is expected to fail in test environment without proper API keys
                println!("⚠️  Price fetching failed as expected in test environment: {}", e);
            }
        }
        
        println!("✅ Helius price integration test completed");
    }
}