use dex_client::types::BoostedToken;
use std::process::Command;

#[test]
fn test_parse_latest_boosted_tokens() {
    let output = Command::new("curl")
        .args(&["-s", "https://api.dexscreener.com/token-boosts/latest/v1?chainId=solana"])
        .output()
        .expect("Failed to execute curl command");
    
    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        match serde_json::from_str::<Vec<BoostedToken>>(&json_str) {
            Ok(tokens) => {
                println!("✅ Successfully parsed {} tokens from latest API", tokens.len());
                assert!(!tokens.is_empty(), "Should have at least one token");
                
                if let Some(first_token) = tokens.first() {
                    println!("First token: {}", first_token.token_address);
                    println!("  URL: {}", first_token.url);
                    println!("  Description: {:?}", first_token.description);
                    println!("  Open Graph: {:?}", first_token.open_graph);
                    println!("  Links: {:?}", first_token.links);
                    
                    assert!(!first_token.url.is_empty());
                    assert!(!first_token.chain_id.is_empty());
                    assert!(!first_token.token_address.is_empty());
                }
            }
            Err(e) => {
                println!("❌ Failed to parse latest tokens: {}", e);
                println!("Raw JSON (first 500 chars): {}", &json_str[..500.min(json_str.len())]);
                panic!("Failed to parse latest tokens: {}", e);
            }
        }
    } else {
        panic!("Failed to fetch latest tokens: {}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn test_parse_top_boosted_tokens() {
    let output = Command::new("curl")
        .args(&["-s", "https://api.dexscreener.com/token-boosts/top/v1?chainId=solana"])
        .output()
        .expect("Failed to execute curl command");
    
    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        match serde_json::from_str::<Vec<BoostedToken>>(&json_str) {
            Ok(tokens) => {
                println!("✅ Successfully parsed {} tokens from top API", tokens.len());
                assert!(!tokens.is_empty(), "Should have at least one token");
                
                if let Some(first_token) = tokens.first() {
                    println!("First token: {}", first_token.token_address);
                    println!("  URL: {}", first_token.url);
                    println!("  Description: {:?}", first_token.description);
                    println!("  Open Graph: {:?}", first_token.open_graph);
                    println!("  Links: {:?}", first_token.links);
                    
                    assert!(!first_token.url.is_empty());
                    assert!(!first_token.chain_id.is_empty());
                    assert!(!first_token.token_address.is_empty());
                }
            }
            Err(e) => {
                println!("❌ Failed to parse top tokens: {}", e);
                println!("Raw JSON (first 500 chars): {}", &json_str[..500.min(json_str.len())]);
                panic!("Failed to parse top tokens: {}", e);
            }
        }
    } else {
        panic!("Failed to fetch top tokens: {}", String::from_utf8_lossy(&output.stderr));
    }
}