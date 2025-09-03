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

    println!("🔐 Testing Multi-Chain Security Checks\n");

    // Test cases: (address, chain, expected_name, expected_safe)
    let test_tokens = vec![
        // Ethereum tokens - should use Honeypot.is
        ("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "ethereum", "USDC", true),
        ("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "ethereum", "WETH", true),
        
        // BSC token - should use Honeypot.is
        ("0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56", "bsc", "BUSD", true),
        
        // Solana tokens - should use SolSniffer
        ("So11111111111111111111111111111111111111112", "solana", "SOL", false), // mintable + freezable
        ("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "solana", "USDC", false), // mintable
        ("2FPyTwcZLUg1MDrwsyoP4D6s1tM7hAkHYRjkNb5w6Pxk", "solana", "ELON", false), // freezable
        ("MEW1gQWJ3nEXg2qgERiKu7FAFj79PHvQVREQUzScPP5", "solana", "MEW", true), // both disabled
    ];

    for (address, chain, name, expected_safe) in test_tokens {
        print!("Testing {} ({}) on {}... ", name, address, chain);
        
        let is_safe = is_token_safe(address, chain).await;
        
        let status = if is_safe { "✅ SAFE" } else { "🚫 BLOCKED" };
        let prediction = if expected_safe == is_safe { "✓" } else { "✗ UNEXPECTED" };
        
        println!("{} {}", status, prediction);
    }
    
    println!("\n✨ Multi-chain security test complete!");
    println!("🔍 Check logs above for detailed API responses");
}