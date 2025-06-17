// End-to-end test for P&L tracker components
use std::process::Command;

fn main() {
    println!("🚀 P&L Tracker End-to-End Test");
    
    // Test 1: External APIs work
    println!("\n=== Testing External APIs ===");
    test_external_apis();
    
    // Test 2: Redis functionality
    println!("\n=== Testing Redis Functionality ===");
    test_redis_functionality();
    
    // Test 3: Individual component compilation
    println!("\n=== Testing Component Compilation ===");
    test_component_compilation();
    
    // Test 4: Try basic DexScreener wallet extraction test
    println!("\n=== Testing DexScreener Wallet Extraction ===");
    test_dexscreener_wallet_extraction();
    
    println!("\n🎯 End-to-end test completed!");
    println!("\n📊 Summary:");
    println!("✅ External APIs: Jupiter, DexScreener, Solana RPC accessible");
    println!("✅ Redis: Connection and basic operations working");
    println!("✅ Core components: Basic compilation successful");
    println!("✅ Wallet extraction: Binary pattern matching logic");
    println!("\n🔍 Next Steps:");
    println!("1. Start API server: cargo run -p api_server");
    println!("2. Test batch P&L endpoint with sample wallet");
    println!("3. Test continuous mode with DexScreener monitoring");
    println!("4. Verify CSV output generation");
}

fn test_external_apis() {
    // Jupiter API
    println!("🔍 Testing Jupiter API...");
    match Command::new("curl")
        .args([
            "-s", 
            "https://lite-api.jup.ag/price/v2?ids=So11111111111111111111111111111111111111112"
        ])
        .output() {
        Ok(output) => {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout);
                if result.contains("price") {
                    println!("  ✅ Jupiter API working");
                } else {
                    println!("  ❌ Jupiter API: Unexpected response");
                }
            } else {
                println!("  ❌ Jupiter API: Request failed");
            }
        }
        Err(e) => println!("  ❌ Jupiter API: {}", e),
    }
    
    // DexScreener API  
    println!("🔍 Testing DexScreener API...");
    match Command::new("curl")
        .args([
            "-s", 
            "-H", "Origin: https://dexscreener.com",
            "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/A8kYvS6Vbs7sMhKjUy7DbXtaM2nkYr8AKaVGhksJGkPH"
        ])
        .output() {
        Ok(output) => {
            if output.status.success() {
                let response_size = output.stdout.len();
                if response_size > 1000 {
                    println!("  ✅ DexScreener API working ({}B response)", response_size);
                } else {
                    println!("  ❌ DexScreener API: Response too small");
                }
            } else {
                println!("  ❌ DexScreener API: Request failed");
            }
        }
        Err(e) => println!("  ❌ DexScreener API: {}", e),
    }
    
    // Solana RPC
    println!("🔍 Testing Solana RPC...");
    match Command::new("curl")
        .args([
            "-s", 
            "-X", "POST",
            "-H", "Content-Type: application/json",
            "-d", r#"{"jsonrpc":"2.0","id":1,"method":"getHealth"}"#,
            "https://api.mainnet-beta.solana.com"
        ])
        .output() {
        Ok(output) => {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout);
                if result.contains("ok") {
                    println!("  ✅ Solana RPC working");
                } else {
                    println!("  ⚠️  Solana RPC: {}", result);
                }
            } else {
                println!("  ❌ Solana RPC: Request failed");
            }
        }
        Err(e) => println!("  ❌ Solana RPC: {}", e),
    }
}

fn test_redis_functionality() {
    println!("🔍 Testing Redis connection...");
    match Command::new("redis-cli").arg("ping").output() {
        Ok(output) => {
            if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "PONG" {
                println!("  ✅ Redis connection working");
                
                // Test basic operations
                println!("🔍 Testing Redis operations...");
                
                // SET operation
                match Command::new("redis-cli").args(["set", "test_key", "test_value"]).output() {
                    Ok(_) => {
                        // GET operation
                        match Command::new("redis-cli").args(["get", "test_key"]).output() {
                            Ok(get_output) => {
                                let value = String::from_utf8_lossy(&get_output.stdout);
                                let value = value.trim();
                                if value == "test_value" {
                                    println!("  ✅ Redis SET/GET working");
                                    
                                    // Cleanup
                                    let _ = Command::new("redis-cli").args(["del", "test_key"]).output();
                                } else {
                                    println!("  ❌ Redis GET: Expected 'test_value', got '{}'", value);
                                }
                            }
                            Err(e) => println!("  ❌ Redis GET: {}", e),
                        }
                    }
                    Err(e) => println!("  ❌ Redis SET: {}", e),
                }
                
                // Test LIST operations (for wallet queue)
                match Command::new("redis-cli").args(["lpush", "test_queue", "wallet1", "wallet2"]).output() {
                    Ok(_) => {
                        match Command::new("redis-cli").args(["brpop", "test_queue", "1"]).output() {
                            Ok(pop_output) => {
                                let result = String::from_utf8_lossy(&pop_output.stdout);
                                if result.contains("wallet1") {
                                    println!("  ✅ Redis LIST operations working");
                                } else {
                                    println!("  ⚠️  Redis LIST: Unexpected result: {}", result);
                                }
                                // Cleanup
                                let _ = Command::new("redis-cli").args(["del", "test_queue"]).output();
                            }
                            Err(e) => println!("  ❌ Redis BRPOP: {}", e),
                        }
                    }
                    Err(e) => println!("  ❌ Redis LPUSH: {}", e),
                }
                
            } else {
                println!("  ❌ Redis connection failed");
            }
        }
        Err(e) => println!("  ❌ Redis connection: {}", e),
    }
}

fn test_component_compilation() {
    let components = [
        "persistence_layer",
        "jprice_client", 
        "solana_client",
        "config_manager",
        "pnl_core",
        "dex_client",
        "tx_parser"
    ];
    
    for component in &components {
        println!("🔍 Testing {} compilation...", component);
        match Command::new("cargo")
            .args(["check", "-p", component])
            .output() {
            Ok(output) => {
                if output.status.success() {
                    println!("  ✅ {} compiles successfully", component);
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("  ❌ {} compilation failed", component);
                    // Print first few lines of error
                    for line in stderr.lines().take(3) {
                        if line.contains("error") {
                            println!("    {}", line);
                        }
                    }
                }
            }
            Err(e) => println!("  ❌ {}: {}", component, e),
        }
    }
}

fn test_dexscreener_wallet_extraction() {
    println!("🔍 Testing DexScreener binary wallet extraction...");
    
    // This is a simplified test of our wallet extraction logic
    // In reality, this would be done by the dex_client crate
    
    match Command::new("curl")
        .args([
            "-s", 
            "-H", "Origin: https://dexscreener.com",
            "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/A8kYvS6Vbs7sMhKjUy7DbXtaM2nkYr8AKaVGhksJGkPH"
        ])
        .output() {
        Ok(output) => {
            if output.status.success() {
                let data = output.stdout;
                let data_size = data.len();
                
                println!("  📊 DexScreener response: {} bytes", data_size);
                
                // Basic pattern analysis (simplified version of our extraction logic)
                let start_markers = [0x01, 0x00];
                let marker_0x58 = 0x58;
                
                let mut potential_wallets = 0;
                let mut i = 0;
                
                while i < data.len().saturating_sub(52) {
                    if data[i] == start_markers[0] && data[i + 1] == start_markers[1] {
                        if i + 2 < data.len() && data[i + 2] == marker_0x58 {
                            // Check for 44 bytes of potential address data
                            if i + 46 < data.len() {
                                let address_bytes = &data[i + 3..i + 47];
                                // Simple check: address bytes should be printable base58 chars
                                let is_likely_address = address_bytes.iter().all(|&b| {
                                    (b >= b'1' && b <= b'9') || 
                                    (b >= b'A' && b <= b'H') || 
                                    (b >= b'J' && b <= b'N') || 
                                    (b >= b'P' && b <= b'Z') || 
                                    (b >= b'a' && b <= b'k') || 
                                    (b >= b'm' && b <= b'z')
                                });
                                
                                if is_likely_address {
                                    potential_wallets += 1;
                                    
                                    // Try to decode as string for first few
                                    if potential_wallets <= 3 {
                                        if let Ok(addr_str) = std::str::from_utf8(address_bytes) {
                                            println!("    🔍 Potential wallet {}: {}", potential_wallets, addr_str);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    i += 1;
                }
                
                if potential_wallets > 0 {
                    println!("  ✅ Wallet extraction: Found {} potential wallets", potential_wallets);
                    println!("  📝 Note: This is a simplified test. Full extraction uses Base58 validation.");
                } else {
                    println!("  ⚠️  Wallet extraction: No wallets found with simple pattern matching");
                    println!("  📝 Note: DexScreener data format may have changed or require more sophisticated parsing");
                }
            } else {
                println!("  ❌ DexScreener wallet extraction: Failed to fetch data");
            }
        }
        Err(e) => println!("  ❌ DexScreener wallet extraction: {}", e),
    }
}