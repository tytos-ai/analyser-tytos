// Quick integration test without dependencies
use std::process::Command;

fn main() {
    println!("🔧 P&L Tracker Quick Integration Test");
    
    // Test 1: Redis connection
    println!("\n📋 Testing Redis connection...");
    match Command::new("redis-cli").arg("ping").output() {
        Ok(output) => {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout);
                if result.trim() == "PONG" {
                    println!("✅ Redis connection: PASSED");
                } else {
                    println!("❌ Redis connection: FAILED - Unexpected response: {}", result);
                }
            } else {
                println!("❌ Redis connection: FAILED - Command failed");
            }
        }
        Err(e) => println!("❌ Redis connection: FAILED - {}", e),
    }
    
    // Test 2: Jupiter API
    println!("\n💰 Testing Jupiter API...");
    match Command::new("curl")
        .args([
            "-s", 
            "https://lite-api.jup.ag/price/v2?ids=So11111111111111111111111111111111111111112"
        ])
        .output() {
        Ok(output) => {
            if output.status.success() {
                let result = String::from_utf8_lossy(&output.stdout);
                if result.contains("So11111111111111111111111111111111111111112") && result.contains("price") {
                    println!("✅ Jupiter API: PASSED");
                } else {
                    println!("❌ Jupiter API: FAILED - Unexpected response");
                }
            } else {
                println!("❌ Jupiter API: FAILED - Command failed");
            }
        }
        Err(e) => println!("❌ Jupiter API: FAILED - {}", e),
    }
    
    // Test 3: DexScreener API
    println!("\n🦎 Testing DexScreener API...");
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
                    println!("✅ DexScreener API: PASSED (Response size: {} bytes)", response_size);
                } else {
                    println!("❌ DexScreener API: FAILED - Response too small: {} bytes", response_size);
                }
            } else {
                println!("❌ DexScreener API: FAILED - Command failed");
            }
        }
        Err(e) => println!("❌ DexScreener API: FAILED - {}", e),
    }
    
    // Test 4: Workspace compilation check
    println!("\n🔧 Testing workspace compilation...");
    match Command::new("cargo").args(["check", "--workspace"]).output() {
        Ok(output) => {
            if output.status.success() {
                println!("✅ Workspace compilation: PASSED");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("❌ Workspace compilation: FAILED");
                println!("Error: {}", stderr);
            }
        }
        Err(e) => println!("❌ Workspace compilation: FAILED - {}", e),
    }
    
    println!("\n🎯 Quick integration test completed!");
}