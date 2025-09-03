#!/usr/bin/env cargo +nightly -Zscript

//! ```cargo
//! [dependencies]
//! tokio = { version = "1", features = ["full"] }
//! dex_client = { path = "./dex_client" }
//! tracing = "0.1"
//! tracing-subscriber = "0.3"
//! ```

use dex_client::is_token_safe;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("Testing Honeypot.is security checks...\n");

    // Test cases: (address, chain, expected_name)
    let test_tokens = vec![
        // Known safe tokens
        ("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "ethereum", "USDC"),
        ("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "ethereum", "WETH"),
        
        // Solana token (should always pass - not supported by Honeypot.is)
        ("So11111111111111111111111111111111111111112", "solana", "SOL"),
        
        // Test a random Ethereum token (you can replace with a known honeypot)
        ("0x6982508145454Ce325dDbE47a25d4ec3d2311933", "ethereum", "PEPE"),
        
        // BSC test
        ("0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56", "bsc", "BUSD"),
    ];

    for (address, chain, name) in test_tokens {
        print!("Testing {} ({}) on {}... ", name, address, chain);
        
        let is_safe = is_token_safe(address, chain).await;
        
        if is_safe {
            println!("✅ SAFE");
        } else {
            println!("🚫 BLOCKED (honeypot or high-risk)");
        }
    }
    
    println!("\n✨ Test complete!");
}