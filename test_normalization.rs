use config_manager::normalize_chain_for_zerion;

fn main() {
    // Test various chain name inputs
    let test_cases = vec![
        ("solana", "solana"),
        ("sol", "solana"),
        ("ethereum", "ethereum"),
        ("eth", "ethereum"),
        ("base", "base"),
        ("binance", "binance-smart-chain"),
        ("bsc", "binance-smart-chain"),
        ("binance-smart-chain", "binance-smart-chain"),
        ("bnb", "binance-smart-chain"),
        ("Binance Smart Chain", "binance-smart-chain"),
        ("  BSC  ", "binance-smart-chain"), // Test with whitespace
    ];

    println!("Testing chain normalization:");
    println!("============================");

    for (input, expected) in test_cases {
        match normalize_chain_for_zerion(input) {
            Ok(result) => {
                let status = if result == expected { "✅ PASS" } else { "❌ FAIL" };
                println!("{} '{}' -> '{}' (expected: '{}')", status, input, result, expected);
            }
            Err(e) => {
                println!("❌ ERROR '{}' -> Error: {}", input, e);
            }
        }
    }

    // Test invalid chain
    println!("\nTesting invalid chains:");
    println!("======================");
    let invalid_chains = vec!["polygon", "avalanche", "unknown"];

    for invalid in invalid_chains {
        match normalize_chain_for_zerion(invalid) {
            Ok(result) => {
                println!("❌ UNEXPECTED SUCCESS '{}' -> '{}'", invalid, result);
            }
            Err(e) => {
                println!("✅ EXPECTED ERROR '{}' -> {}", invalid, e);
            }
        }
    }
}