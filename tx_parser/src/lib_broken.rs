pub mod solana_tx_processor;

use chrono::{DateTime, TimeZone, Utc};
use pnl_core::{EventMetadata, EventType, FinancialEvent};
use rust_decimal::Decimal;
use serde_json::Value;
use solana_client::{SignatureInfo, SolanaClientError};
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, UiInnerInstructions, UiInstruction,
    UiParsedInstruction, UiTokenAmount, UiTransaction, UiTransactionStatusMeta,
};
use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid transaction data: {0}")]
    InvalidTransaction(String),
    #[error("Unsupported instruction: {0}")]
    UnsupportedInstruction(String),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Decimal parsing error: {0}")]
    Decimal(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Invalid pubkey: {0}")]
    InvalidPubkey(String),
    #[error("Solana client error: {0}")]
    SolanaClient(#[from] SolanaClientError),
}

pub type Result<T> = std::result::Result<T, ParseError>;

/// Known Solana program IDs
pub mod program_ids {
    pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111112";
    pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
    pub const RAYDIUM_AMM_V4: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
    pub const RAYDIUM_AMM_V5: &str = "5quBtoiQqxF9Jv6KYKctB59NT3gtJD2Y65kdnB1Uev3h";
    pub const ORCA_SWAP: &str = "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP";
    pub const JUPITER_AGGREGATOR: &str = "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB";
    pub const METEORA: &str = "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB";
    pub const PUMP_FUN: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
}

/// Configuration for transaction parsing
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Minimum SOL amount to consider for events (to filter dust)
    pub min_sol_amount: Decimal,
    
    /// Minimum token amount to consider for events
    pub min_token_amount: Decimal,
    
    /// Include transfer events (in/out)
    pub include_transfers: bool,
    
    /// Include fee events
    pub include_fees: bool,
    
    /// Known stable coins to handle differently
    pub stable_coins: Vec<String>,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            min_sol_amount: Decimal::new(1, 6), // 0.000001 SOL
            min_token_amount: Decimal::new(1, 9), // 0.000000001 tokens
            include_transfers: true,
            include_fees: true,
            stable_coins: vec![
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC
                "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(), // USDT
            ],
        }
    }
}

/// Main transaction parser
pub struct TransactionParser {
    config: ParserConfig,
}

impl TransactionParser {
    pub fn new(config: ParserConfig) -> Self {
        Self { config }
    }

    /// Parse a single transaction into financial events
    pub fn parse_transaction(
        &self,
        signature_info: &SignatureInfo,
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
        target_wallet: &str,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        // Get transaction timestamp
        let timestamp = signature_info.timestamp().unwrap_or_else(Utc::now);

        // Extract transaction fee
        let fee = self.extract_transaction_fee(transaction)?;

        // Parse the transaction based on its structure
        if let Some(ref ui_transaction) = transaction.transaction {
            // Parse all instructions in the transaction
            let instruction_events = self.parse_instructions(
                ui_transaction,
                target_wallet,
                &signature_info.signature,
                timestamp,
                fee,
            )?;
            
            events.extend(instruction_events);

            // Parse inner instructions (from program invocations)
            if let Some(ref meta) = transaction.meta {
                if let Some(ref inner_instructions) = meta.inner_instructions {
                    let inner_events = self.parse_inner_instructions(
                        inner_instructions,
                        target_wallet,
                        &signature_info.signature,
                        timestamp,
                        fee,
                    )?;
                    
                    events.extend(inner_events);
                }
            }
        }

        // Include a fee event if enabled
        if self.config.include_fees && fee > Decimal::ZERO {
            events.push(FinancialEvent {
                id: Uuid::new_v4(),
                transaction_id: signature_info.signature.clone(),
                wallet_address: target_wallet.to_string(),
                event_type: EventType::Fee,
                token_mint: program_ids::SOL_MINT.to_string(),
                token_amount: Decimal::ZERO,
                sol_amount: fee,
                timestamp,
                transaction_fee: fee,
                metadata: EventMetadata {
                    program_id: Some(program_ids::SYSTEM_PROGRAM.to_string()),
                    ..Default::default()
                },
            });
        }

        debug!("Parsed {} events from transaction {}", events.len(), signature_info.signature);
        Ok(events)
    }

    /// Parse multiple transactions into financial events
    pub fn parse_transactions(
        &self,
        transactions: &[(SignatureInfo, Option<EncodedConfirmedTransactionWithStatusMeta>)],
        target_wallet: &str,
    ) -> Result<Vec<FinancialEvent>> {
        let mut all_events = Vec::new();

        for (signature_info, transaction_opt) in transactions {
            if let Some(transaction) = transaction_opt {
                match self.parse_transaction(signature_info, transaction, target_wallet) {
                    Ok(events) => {
                        all_events.extend(events);
                    }
                    Err(e) => {
                        warn!("Failed to parse transaction {}: {}", signature_info.signature, e);
                    }
                }
            }
        }

        debug!("Parsed total of {} events from {} transactions", all_events.len(), transactions.len());
        Ok(all_events)
    }

    /// Extract transaction fee
    fn extract_transaction_fee(
        &self,
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> Result<Decimal> {
        if let Some(ref meta) = transaction.meta {
            if let Some(fee) = meta.fee {
                return Ok(Decimal::new(fee as i64, 9)); // Convert lamports to SOL
            }
        }
        Ok(Decimal::ZERO)
    }

    /// Parse main instructions
    fn parse_instructions(
        &self,
        ui_transaction: &UiTransaction,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        match ui_transaction {
            UiTransaction::Json(ref parsed_tx) => {
                if let Some(ref message) = &parsed_tx.message {
                    if let Some(ref instructions) = message.instructions {
                        for (index, instruction) in instructions.iter().enumerate() {
                            let instruction_events = self.parse_single_instruction(
                                instruction,
                                target_wallet,
                                transaction_id,
                                timestamp,
                                transaction_fee,
                                Some(index as u32),
                            )?;
                            
                            events.extend(instruction_events);
                        }
                    }
                }
            }
            _ => {
                // Handle other UiTransaction variants (Encoded, etc.)
                // For now, skip parsing of non-JSON transactions
            }
        }

        Ok(events)
    }

    /// Parse inner instructions
    fn parse_inner_instructions(
        &self,
        inner_instructions: &[UiInnerInstructions],
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        for inner_instruction in inner_instructions {
            for instruction in &inner_instruction.instructions {
                let instruction_events = self.parse_single_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    None,
                )?;
                
                events.extend(instruction_events);
            }
        }

        Ok(events)
    }

    /// Parse a single instruction
    fn parse_single_instruction(
        &self,
        instruction: &UiInstruction,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        instruction_index: Option<u32>,
    ) -> Result<Vec<FinancialEvent>> {
        match instruction {
            UiInstruction::Parsed(parsed) => {
                self.parse_parsed_instruction(
                    parsed,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    instruction_index,
                )
            }
            UiInstruction::Compiled(_) => {
                // For now, skip compiled instructions
                // Could be extended to handle raw instruction data
                Ok(Vec::new())
            }
        }
    }

    /// Parse a parsed instruction
    fn parse_parsed_instruction(
        &self,
        instruction: &UiParsedInstruction,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        instruction_index: Option<u32>,
    ) -> Result<Vec<FinancialEvent>> {
        let program_id = &instruction.program_id;
        
        // Create base metadata
        let mut metadata = EventMetadata {
            program_id: Some(program_id.clone()),
            instruction_index,
            ..Default::default()
        };

        // Parse based on program type
        match program_id.as_str() {
            program_ids::TOKEN_PROGRAM => {
                self.parse_token_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
            program_ids::SYSTEM_PROGRAM => {
                self.parse_system_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
            program_ids::RAYDIUM_AMM_V4 | program_ids::RAYDIUM_AMM_V5 => {
                metadata.exchange = Some("Raydium".to_string());
                self.parse_dex_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
            program_ids::ORCA_SWAP => {
                metadata.exchange = Some("Orca".to_string());
                self.parse_dex_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
            program_ids::JUPITER_AGGREGATOR => {
                metadata.exchange = Some("Jupiter".to_string());
                self.parse_dex_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
            program_ids::PUMP_FUN => {
                metadata.exchange = Some("Pump.fun".to_string());
                self.parse_dex_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
            _ => {
                // Unknown program, try to parse generically
                self.parse_generic_instruction(
                    instruction,
                    target_wallet,
                    transaction_id,
                    timestamp,
                    transaction_fee,
                    metadata,
                )
            }
        }
    }

    /// Parse token program instructions
    fn parse_token_instruction(
        &self,
        instruction: &UiParsedInstruction,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        if let Some(ref parsed) = instruction.parsed {
            if let Some(instruction_type) = parsed.get("type").and_then(|v| v.as_str()) {
                match instruction_type {
                    "transfer" | "transferChecked" => {
                        self.parse_token_transfer(
                            parsed,
                            target_wallet,
                            transaction_id,
                            timestamp,
                            transaction_fee,
                            metadata,
                        )
                    }
                    "mintTo" => {
                        self.parse_token_mint(
                            parsed,
                            target_wallet,
                            transaction_id,
                            timestamp,
                            transaction_fee,
                            metadata,
                        )
                    }
                    "burn" => {
                        self.parse_token_burn(
                            parsed,
                            target_wallet,
                            transaction_id,
                            timestamp,
                            transaction_fee,
                            metadata,
                        )
                    }
                    _ => Ok(Vec::new()),
                }
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse system program instructions
    fn parse_system_instruction(
        &self,
        instruction: &UiParsedInstruction,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        if let Some(ref parsed) = instruction.parsed {
            if let Some(instruction_type) = parsed.get("type").and_then(|v| v.as_str()) {
                match instruction_type {
                    "transfer" => {
                        self.parse_sol_transfer(
                            parsed,
                            target_wallet,
                            transaction_id,
                            timestamp,
                            transaction_fee,
                            metadata,
                        )
                    }
                    _ => Ok(Vec::new()),
                }
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse DEX-related instructions (generic)
    fn parse_dex_instruction(
        &self,
        instruction: &UiParsedInstruction,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        // For DEX instructions, we often need to look at the associated token transfers
        // This is a simplified implementation - could be enhanced to parse specific DEX data
        Ok(Vec::new())
    }

    /// Parse generic instructions
    fn parse_generic_instruction(
        &self,
        _instruction: &UiParsedInstruction,
        _target_wallet: &str,
        _transaction_id: &str,
        _timestamp: DateTime<Utc>,
        _transaction_fee: Decimal,
        _metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        // Generic parsing for unknown programs
        Ok(Vec::new())
    }

    /// Parse token transfer
    fn parse_token_transfer(
        &self,
        parsed: &Value,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        if let Some(info) = parsed.get("info") {
            let source = info.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let destination = info.get("destination").and_then(|v| v.as_str()).unwrap_or("");
            let authority = info.get("authority").and_then(|v| v.as_str()).unwrap_or("");
            
            // Get token mint
            let mint = info.get("mint")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            
            // Get amount
            let amount = if let Some(token_amount) = info.get("tokenAmount") {
                self.parse_token_amount(token_amount)?
            } else if let Some(amount_str) = info.get("amount").and_then(|v| v.as_str()) {
                Decimal::from_str(amount_str).map_err(|e| ParseError::Decimal(e.to_string()))?
            } else {
                return Ok(events);
            };

            // Check if this transfer involves our target wallet
            if authority == target_wallet || source == target_wallet || destination == target_wallet {
                // Determine event type
                let event_type = if authority == target_wallet || source == target_wallet {
                    if destination == target_wallet {
                        // Self-transfer, skip
                        return Ok(events);
                    } else {
                        EventType::TransferOut
                    }
                } else {
                    EventType::TransferIn
                };

                if amount >= self.config.min_token_amount {
                    events.push(FinancialEvent {
                        id: Uuid::new_v4(),
                        transaction_id: transaction_id.to_string(),
                        wallet_address: target_wallet.to_string(),
                        event_type,
                        token_mint: mint.to_string(),
                        token_amount: amount,
                        sol_amount: Decimal::ZERO,
                        timestamp,
                        transaction_fee,
                        metadata,
                    });
                }
            }
        }

        Ok(events)
    }

    /// Parse token mint event
    fn parse_token_mint(
        &self,
        parsed: &Value,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        if let Some(info) = parsed.get("info") {
            let account = info.get("account").and_then(|v| v.as_str()).unwrap_or("");
            let mint = info.get("mint").and_then(|v| v.as_str()).unwrap_or("unknown");
            
            // Get amount
            let amount = if let Some(token_amount) = info.get("tokenAmount") {
                self.parse_token_amount(token_amount)?
            } else if let Some(amount_str) = info.get("amount").and_then(|v| v.as_str()) {
                Decimal::from_str(amount_str).map_err(|e| ParseError::Decimal(e.to_string()))?
            } else {
                return Ok(events);
            };

            // Check if this involves our target wallet (as recipient)
            if account == target_wallet && amount >= self.config.min_token_amount {
                events.push(FinancialEvent {
                    id: Uuid::new_v4(),
                    transaction_id: transaction_id.to_string(),
                    wallet_address: target_wallet.to_string(),
                    event_type: EventType::TransferIn,
                    token_mint: mint.to_string(),
                    token_amount: amount,
                    sol_amount: Decimal::ZERO,
                    timestamp,
                    transaction_fee,
                    metadata,
                });
            }
        }

        Ok(events)
    }

    /// Parse token burn event
    fn parse_token_burn(
        &self,
        parsed: &Value,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        if let Some(info) = parsed.get("info") {
            let account = info.get("account").and_then(|v| v.as_str()).unwrap_or("");
            let mint = info.get("mint").and_then(|v| v.as_str()).unwrap_or("unknown");
            
            // Get amount
            let amount = if let Some(token_amount) = info.get("tokenAmount") {
                self.parse_token_amount(token_amount)?
            } else if let Some(amount_str) = info.get("amount").and_then(|v| v.as_str()) {
                Decimal::from_str(amount_str).map_err(|e| ParseError::Decimal(e.to_string()))?
            } else {
                return Ok(events);
            };

            // Check if this involves our target wallet
            if account == target_wallet && amount >= self.config.min_token_amount {
                events.push(FinancialEvent {
                    id: Uuid::new_v4(),
                    transaction_id: transaction_id.to_string(),
                    wallet_address: target_wallet.to_string(),
                    event_type: EventType::TransferOut,
                    token_mint: mint.to_string(),
                    token_amount: amount,
                    sol_amount: Decimal::ZERO,
                    timestamp,
                    transaction_fee,
                    metadata,
                });
            }
        }

        Ok(events)
    }

    /// Parse SOL transfer
    fn parse_sol_transfer(
        &self,
        parsed: &Value,
        target_wallet: &str,
        transaction_id: &str,
        timestamp: DateTime<Utc>,
        transaction_fee: Decimal,
        metadata: EventMetadata,
    ) -> Result<Vec<FinancialEvent>> {
        let mut events = Vec::new();

        if let Some(info) = parsed.get("info") {
            let source = info.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let destination = info.get("destination").and_then(|v| v.as_str()).unwrap_or("");
            
            // Get amount in lamports and convert to SOL
            let lamports = info.get("lamports")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            
            let sol_amount = Decimal::new(lamports as i64, 9); // Convert lamports to SOL

            // Check if this transfer involves our target wallet
            if (source == target_wallet || destination == target_wallet) && sol_amount >= self.config.min_sol_amount {
                let event_type = if source == target_wallet {
                    if destination == target_wallet {
                        // Self-transfer, skip
                        return Ok(events);
                    } else {
                        EventType::TransferOut
                    }
                } else {
                    EventType::TransferIn
                };

                events.push(FinancialEvent {
                    id: Uuid::new_v4(),
                    transaction_id: transaction_id.to_string(),
                    wallet_address: target_wallet.to_string(),
                    event_type,
                    token_mint: program_ids::SOL_MINT.to_string(),
                    token_amount: sol_amount,
                    sol_amount,
                    timestamp,
                    transaction_fee,
                    metadata,
                });
            }
        }

        Ok(events)
    }

    /// Parse token amount from UiTokenAmount structure
    fn parse_token_amount(&self, token_amount: &Value) -> Result<Decimal> {
        if let Some(amount_str) = token_amount.get("uiAmountString").and_then(|v| v.as_str()) {
            Decimal::from_str(amount_str).map_err(|e| ParseError::Decimal(e.to_string()))
        } else if let Some(amount) = token_amount.get("uiAmount").and_then(|v| v.as_f64()) {
            Ok(Decimal::try_from(amount).map_err(|e| ParseError::Decimal(e.to_string()))?)
        } else {
            Err(ParseError::MissingField("token amount".to_string()))
        }
    }
}

/// Utility functions for transaction parsing
pub mod utils {
    use super::*;

    /// Check if a program ID represents a DEX
    pub fn is_dex_program(program_id: &str) -> bool {
        matches!(
            program_id,
            program_ids::RAYDIUM_AMM_V4
                | program_ids::RAYDIUM_AMM_V5
                | program_ids::ORCA_SWAP
                | program_ids::JUPITER_AGGREGATOR
                | program_ids::METEORA
                | program_ids::PUMP_FUN
        )
    }

    /// Check if a token is a stablecoin
    pub fn is_stablecoin(token_mint: &str, config: &ParserConfig) -> bool {
        config.stable_coins.contains(&token_mint.to_string())
    }

    /// Extract wallet addresses from a transaction
    pub fn extract_wallet_addresses(
        transaction: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> Vec<String> {
        let _addresses = Vec::new();
        // TODO: Fix Solana API structure changes
        // This function needs to be updated for the current Solana SDK version
        vec![]
    }

    /// Classify event as buy/sell based on context
    pub fn classify_trade_event(
        events: &[FinancialEvent],
        target_wallet: &str,
    ) -> Vec<FinancialEvent> {
        // This is a simplified implementation
        // In practice, you'd analyze the combination of token transfers to determine buy/sell
        let mut classified_events = Vec::new();

        for event in events {
            let mut new_event = event.clone();
            
            // Simple heuristic: if SOL is going out and tokens coming in, it's a buy
            // If tokens going out and SOL coming in, it's a sell
            // This would need more sophisticated logic for real implementation
            
            classified_events.push(new_event);
        }

        classified_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_config_default() {
        let config = ParserConfig::default();
        assert_eq!(config.min_sol_amount, Decimal::new(1, 6));
        assert!(config.include_transfers);
        assert!(config.include_fees);
    }

    #[test]
    fn test_program_id_validation() {
        assert!(utils::is_dex_program(program_ids::RAYDIUM_AMM_V4));
        assert!(utils::is_dex_program(program_ids::ORCA_SWAP));
        assert!(!utils::is_dex_program(program_ids::TOKEN_PROGRAM));
    }

    #[test]
    fn test_stablecoin_detection() {
        let config = ParserConfig::default();
        assert!(utils::is_stablecoin("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", &config)); // USDC
        assert!(!utils::is_stablecoin("unknown_token", &config));
    }
}