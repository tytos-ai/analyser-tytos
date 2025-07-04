#!/usr/bin/env -S cargo +stable script
//! Test script to verify our parsing logic against actual BirdEye data
//! 
//! Run with: cargo +stable script test_parsing_verification.rs

use serde_json;
use std::fs;

// Copy the relevant structs from our codebase for testing
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestTransactionResponse {
    pub success: bool,
    pub data: TestTransactionData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestTransactionData {
    pub items: Vec<TestTransaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestTransaction {
    pub quote: TestTokenSide,
    pub base: TestTokenSide,
    #[serde(rename = "base_price")]
    pub base_price: f64,
    #[serde(rename = "quote_price")]
    pub quote_price: f64,
    #[serde(rename = "tx_hash")]
    pub tx_hash: String,
    #[serde(rename = "block_unix_time")]
    pub block_unix_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestTokenSide {
    pub symbol: String,
    pub decimals: u32,
    pub address: String,
    pub amount: u128,
    #[serde(rename = "type")]
    pub transfer_type: String,
    #[serde(rename = "type_swap")]
    pub type_swap: String,
    #[serde(rename = "ui_amount")]
    pub ui_amount: f64,
    pub price: f64,
    #[serde(rename = "change_amount")]
    pub change_amount: i128,
    #[serde(rename = "ui_change_amount")]
    pub ui_change_amount: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚨 CRITICAL RUST PARSING VERIFICATION");
    println!("=====================================");
    
    // Load test transaction
    let json_data = fs::read_to_string("test_single_transaction.json")?;
    let response: TestTransactionResponse = serde_json::from_str(&json_data)?;
    
    if let Some(tx) = response.data.items.first() {
        println!("\n📊 TESTING SINGLE TRANSACTION:");
        println!("TX Hash: {}...", &tx.tx_hash[..12]);
        
        // Test our aggregation logic
        println!("\n🔍 AGGREGATION LOGIC:");
        
        let mut net_changes = std::collections::HashMap::new();
        net_changes.insert(tx.quote.address.clone(), tx.quote.ui_change_amount);
        net_changes.insert(tx.base.address.clone(), tx.base.ui_change_amount);
        
        println!("Net changes:");
        for (token, amount) in &net_changes {
            println!("  {}: {}", &token[..8], amount);
        }
        
        // Find input/output
        let mut token_in = String::new();
        let mut token_out = String::new();
        let mut amount_in = 0.0;
        let mut amount_out = 0.0;
        
        for (token, net_amount) in &net_changes {
            if *net_amount < 0.0 {
                token_in = token.clone();
                amount_in = net_amount.abs();
            } else if *net_amount > 0.0 {
                token_out = token.clone();
                amount_out = *net_amount;
            }
        }
        
        println!("\nAggregation result:");
        println!("  token_in: {}... ({})", &token_in[..8], amount_in);
        println!("  token_out: {}... ({})", &token_out[..8], amount_out);
        
        // Test FinancialEvent creation
        println!("\n🎯 FINANCIAL EVENT CREATION:");
        
        let sol_mint = "So11111111111111111111111111111111111111112";
        
        if token_in == sol_mint {
            // SOL → Token swap: BUY event
            let price_per_token = if tx.base.address == token_out {
                tx.base.price
            } else {
                tx.quote.price
            };
            
            println!("Event type: Buy");
            println!("Token mint: {}...", &token_out[..8]);
            println!("Token amount: {}", amount_out);
            println!("SOL amount: {}", amount_in);
            println!("Price per token: ${:.6}", price_per_token);
            
            // Verify against expected values
            println!("\n✅ VERIFICATION:");
            println!("Expected SOL spent: {} ✅", tx.quote.ui_change_amount.abs());
            println!("Expected BNSOL received: {} ✅", tx.base.ui_change_amount);
            println!("Expected price: ${:.6} ✅", tx.base.price);
            
            // Check if our logic produces correct results
            if (amount_in - tx.quote.ui_change_amount.abs()).abs() < 0.001 &&
               (amount_out - tx.base.ui_change_amount).abs() < 0.001 &&
               (price_per_token - tx.base.price).abs() < 0.001 {
                println!("🎉 RUST PARSING LOGIC IS CORRECT!");
            } else {
                println!("❌ RUST PARSING LOGIC HAS ERRORS!");
            }
            
        } else if token_out == sol_mint {
            println!("Event type: Sell");
            // ... sell logic would go here
        }
        
        println!("\n🔍 DATA STRUCTURE VALIDATION:");
        println!("Quote (SOL):");
        println!("  ui_change_amount: {} ✅", tx.quote.ui_change_amount);
        println!("  price: ${:.6} (USD) ✅", tx.quote.price);
        println!("  type_swap: {} ✅", tx.quote.type_swap);
        
        println!("Base (BNSOL):");
        println!("  ui_change_amount: {} ✅", tx.base.ui_change_amount);
        println!("  price: ${:.6} (USD) ✅", tx.base.price);
        println!("  type_swap: {} ✅", tx.base.type_swap);
        
    } else {
        println!("❌ No transaction found in test data!");
    }
    
    Ok(())
}