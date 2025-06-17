use dex_client::types::TokenPair;
use std::process::Command;

#[test]
fn test_parse_token_pairs_with_socials() {
    let output = Command::new("curl")
        .args(&["-s", "https://api.dexscreener.com/token-pairs/v1/solana/4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R"])
        .output()
        .expect("Failed to execute curl command");
    
    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        println!("Raw API response (first 1000 chars): {}", &json_str[..1000.min(json_str.len())]);
        
        match serde_json::from_str::<Vec<TokenPair>>(&json_str) {
            Ok(pairs) => {
                println!("✅ Successfully parsed {} token pairs with social info", pairs.len());
                assert!(!pairs.is_empty(), "Should have at least one token pair");
                
                if let Some(first_pair) = pairs.first() {
                    println!("First pair: {}", first_pair.pair_address);
                    println!("  DEX ID: {}", first_pair.dex_id);
                    println!("  Chain ID: {}", first_pair.chain_id);
                    
                    if let Some(ref info) = first_pair.info {
                        println!("  Has info: {:?}", info.image_url);
                        
                        if let Some(ref socials) = info.socials {
                            println!("  Socials ({}):", socials.len());
                            for social in socials {
                                println!("    - Type: {}, URL: {}", social.social_type, social.url);
                            }
                        } else {
                            println!("  No socials");
                        }
                        
                        if let Some(ref websites) = info.websites {
                            println!("  Websites: {}", websites.len());
                        }
                    } else {
                        println!("  No info");
                    }
                    
                    assert!(!first_pair.pair_address.is_empty());
                    assert!(!first_pair.dex_id.is_empty());
                    assert!(!first_pair.chain_id.is_empty());
                }
            }
            Err(e) => {
                println!("❌ Failed to parse token pairs with socials: {}", e);
                println!("Raw JSON (first 1000 chars): {}", &json_str[..1000.min(json_str.len())]);
                panic!("Failed to parse token pairs with socials: {}", e);
            }
        }
    } else {
        panic!("Failed to fetch token pairs: {}", String::from_utf8_lossy(&output.stderr));
    }
}