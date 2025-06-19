#!/bin/bash

echo "=== Testing Configuration Loading ==="

export PNL__BIRDEYE__API_KEY="5ff313b239ac42e297b830b10ea1871d"

echo "Environment variable set: PNL__BIRDEYE__API_KEY='$PNL__BIRDEYE__API_KEY'"

# Create a simple Rust test
cat > temp_config_test.rs << 'EOF'
use std::env;

fn main() {
    println!("=== Environment Variables Check ===");
    
    // Check the specific API key variable
    match env::var("PNL__BIRDEYE__API_KEY") {
        Ok(val) => println!("✅ PNL__BIRDEYE__API_KEY found: '{}'", val),
        Err(_) => println!("❌ PNL__BIRDEYE__API_KEY not found"),
    }
    
    // List all PNL_ prefixed env vars
    println!("\n=== All PNL_ prefixed environment variables ===");
    for (key, value) in env::vars() {
        if key.starts_with("PNL_") {
            println!("  {}='{}'", key, value);
        }
    }
}
EOF

# Compile and run the test
rustc temp_config_test.rs --edition 2021 -o temp_config_test
./temp_config_test

# Clean up
rm -f temp_config_test.rs temp_config_test

echo
echo "=== Testing with curl to verify API key works ==="
curl -s -H "X-API-KEY: $PNL__BIRDEYE__API_KEY" \
     "https://public-api.birdeye.so/defi/token_trending?chain=solana" \
     | jq '.success // "No success field"' 2>/dev/null || echo "API call failed or jq not available"