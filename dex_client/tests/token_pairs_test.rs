use dex_client::types::TokenPair;
use std::process::Command;

#[test]
fn test_parse_token_pairs() {
    let output = Command::new("curl")
        .args(&["-s", "https://api.dexscreener.com/token-pairs/v1/solana/So11111111111111111111111111111111111111112"])
        .output()
        .expect("Failed to execute curl command");
    
    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        println!("Raw API response (first 1000 chars): {}", &json_str[..1000.min(json_str.len())]);
        
        match serde_json::from_str::<Vec<TokenPair>>(&json_str) {
            Ok(pairs) => {
                println!("✅ Successfully parsed {} token pairs", pairs.len());
                assert!(!pairs.is_empty(), "Should have at least one token pair");
                
                if let Some(first_pair) = pairs.first() {
                    println!("First pair: {}", first_pair.pair_address);
                    println!("  DEX ID: {}", first_pair.dex_id);
                    println!("  Chain ID: {}", first_pair.chain_id);
                    println!("  Base Token: {} ({})", first_pair.base_token.symbol, first_pair.base_token.name);
                    println!("  Quote Token: {} ({})", first_pair.quote_token.symbol, first_pair.quote_token.name);
                    println!("  Price USD: {:?}", first_pair.price_usd);
                    println!("  Volume: {:?}", first_pair.volume);
                    println!("  Liquidity: {:?}", first_pair.liquidity);
                    
                    assert!(!first_pair.pair_address.is_empty());
                    assert!(!first_pair.dex_id.is_empty());
                    assert!(!first_pair.chain_id.is_empty());
                }
            }
            Err(e) => {
                println!("❌ Failed to parse token pairs: {}", e);
                println!("Raw JSON (first 1000 chars): {}", &json_str[..1000.min(json_str.len())]);
                panic!("Failed to parse token pairs: {}", e);
            }
        }
    } else {
        panic!("Failed to fetch token pairs: {}", String::from_utf8_lossy(&output.stderr));
    }
}