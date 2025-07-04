#!/usr/bin/env python3
"""
Comprehensive analysis of BirdEye data structure to understand:
1. When SOL price is available in transaction data
2. When we need external SOL price fetching
3. How to properly handle token-to-token scenarios
"""

import json

def analyze_birdeye_data_structure():
    """Analyze the actual BirdEye data structure"""
    
    print("🔍 BIRDEYE DATA STRUCTURE ANALYSIS")
    print("=" * 50)
    
    with open('manual_verification_transactions.json', 'r') as f:
        data = json.load(f)
    
    transactions = data['data']['items']
    sol_mint = "So11111111111111111111111111111111111111112"
    
    # Analyze patterns
    patterns = {
        'sol_to_token': 0,
        'token_to_sol': 0, 
        'token_to_token': 0
    }
    
    sol_prices_found = []
    
    for i, tx in enumerate(transactions):
        quote_addr = tx['quote'].get('address', '')
        base_addr = tx['base'].get('address', '')
        quote_symbol = tx['quote'].get('symbol', 'Unknown')
        base_symbol = tx['base'].get('symbol', 'Unknown')
        
        # Classify transaction type
        if quote_addr == sol_mint and base_addr != sol_mint:
            patterns['sol_to_token'] += 1
            sol_price = tx.get('quote_price')
            if sol_price:
                sol_prices_found.append(sol_price)
        elif quote_addr != sol_mint and base_addr == sol_mint:
            patterns['token_to_sol'] += 1
            sol_price = tx.get('base_price') 
            if sol_price:
                sol_prices_found.append(sol_price)
        elif quote_addr != sol_mint and base_addr != sol_mint:
            patterns['token_to_token'] += 1
            print(f"  🚨 TOKEN-TO-TOKEN FOUND: {quote_symbol} → {base_symbol}")
            print(f"    quote_price: ${tx.get('quote_price')} (price of {quote_symbol})")
            print(f"    base_price: ${tx.get('base_price')} (price of {base_symbol})")
            print(f"    ❌ NO SOL PRICE AVAILABLE in transaction data!")
    
    print(f"\n📊 TRANSACTION PATTERN SUMMARY:")
    print(f"  SOL → Token: {patterns['sol_to_token']}")
    print(f"  Token → SOL: {patterns['token_to_sol']}")
    print(f"  Token → Token: {patterns['token_to_token']}")
    
    print(f"\n💰 SOL PRICE AVAILABILITY:")
    if sol_prices_found:
        print(f"  Found {len(sol_prices_found)} SOL prices")
        print(f"  Range: ${min(sol_prices_found):.2f} - ${max(sol_prices_found):.2f}")
        print(f"  Average: ${sum(sol_prices_found)/len(sol_prices_found):.2f}")
    
    print(f"\n🎯 KEY INSIGHTS:")
    print(f"  1. ✅ SOL → Token: SOL price in quote_price")
    print(f"  2. ✅ Token → SOL: SOL price in base_price") 
    print(f"  3. ❌ Token → Token: NO SOL price in transaction data")
    print(f"  4. 🚨 Current dataset: Only SOL ↔ BNSOL (no token-to-token)")
    
    return patterns

def demonstrate_gemini_bug():
    """Demonstrate the bug Gemini identified"""
    
    print(f"\n🚨 DEMONSTRATING GEMINI'S IDENTIFIED BUG")
    print("=" * 45)
    
    # Mock token-to-token transaction
    mock_usdc_to_render = {
        "quote": {
            "symbol": "USDC",
            "address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "price": 1.0,  # $1 per USDC
            "ui_change_amount": -1000.0
        },
        "base": {
            "symbol": "RENDER", 
            "address": "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof",
            "price": 20.0,  # $20 per RENDER
            "ui_change_amount": 50.0
        },
        "quote_price": 1.0,   # Price of USDC, NOT SOL!
        "base_price": 20.0    # Price of RENDER, NOT SOL!
    }
    
    sol_mint = "So11111111111111111111111111111111111111112"
    
    print("📋 MOCK USDC → RENDER TRANSACTION:")
    print(f"  Quote: {mock_usdc_to_render['quote']['symbol']} (${mock_usdc_to_render['quote']['price']})")
    print(f"  Base: {mock_usdc_to_render['base']['symbol']} (${mock_usdc_to_render['base']['price']})")
    print(f"  quote_price: ${mock_usdc_to_render['quote_price']} (USDC price)")
    print(f"  base_price: ${mock_usdc_to_render['base_price']} (RENDER price)")
    
    # Calculate USD value
    amount_out = 50.0  # RENDER received
    token_price = 20.0  # RENDER price
    usd_value = amount_out * token_price  # $1000
    
    print(f"\n💰 VALUE CALCULATION:")
    print(f"  USD value: {amount_out} RENDER × ${token_price} = ${usd_value}")
    
    # Demonstrate our BUGGY logic
    print(f"\n❌ OUR BUGGY LOGIC:")
    quote_addr = mock_usdc_to_render['quote']['address']
    base_addr = mock_usdc_to_render['base']['address']
    
    if quote_addr == sol_mint:
        sol_price = mock_usdc_to_render['quote_price']
        print(f"  SOL price from quote: ${sol_price}")
    elif base_addr == sol_mint:
        sol_price = mock_usdc_to_render['base_price']
        print(f"  SOL price from base: ${sol_price}")
    else:
        # This is the BUG!
        sol_price = mock_usdc_to_render['quote_price']  # Using USDC price as SOL price!
        print(f"  🚨 BUG: Using quote_price as SOL price: ${sol_price}")
        print(f"  🚨 This is USDC price, NOT SOL price!")
    
    # Wrong calculation
    wrong_sol_equivalent = usd_value / sol_price
    print(f"  Wrong calculation: ${usd_value} ÷ ${sol_price} = {wrong_sol_equivalent} 'SOL'")
    print(f"  🚨 RESULT: 1000 'SOL' (should be ~6.67 SOL at $150/SOL)")
    
    # Correct calculation
    print(f"\n✅ CORRECT LOGIC:")
    actual_sol_price = 150.0  # Real SOL price
    correct_sol_equivalent = usd_value / actual_sol_price
    print(f"  Fetch actual SOL price: ${actual_sol_price}")
    print(f"  Correct calculation: ${usd_value} ÷ ${actual_sol_price} = {correct_sol_equivalent:.2f} SOL")
    print(f"  ✅ RESULT: {correct_sol_equivalent:.2f} SOL (mathematically correct)")

def propose_fix():
    """Propose the correct fix for the SOL price bug"""
    
    print(f"\n🔧 PROPOSED FIX")
    print("=" * 20)
    
    print("Current BUGGY code:")
    print("""
} else {
    // ❌ BUG: Using quote_price as SOL price when neither token is SOL
    let sol_price_usd = Decimal::try_from(first_tx.quote_price)
        .unwrap_or(Decimal::from(150));
    usd_value / sol_price_usd  // Wrong! quote_price is not SOL price
};
""")
    
    print("Fixed code:")
    print("""
} else {
    // ✅ FIXED: Fetch actual SOL price externally
    let sol_price_usd = self.fetch_sol_price_at_timestamp(timestamp).await
        .map_err(|e| warn!("Failed to fetch SOL price: {}", e))
        .unwrap_or(Decimal::from(150)); // Fallback only if fetch fails
    usd_value / sol_price_usd  // Correct! Using actual SOL price
};
""")
    
    print("\n📋 IMPLEMENTATION REQUIREMENTS:")
    print("1. Add async SOL price fetching method")
    print("2. Use BirdEye historical price API for accuracy") 
    print("3. Implement proper error handling and fallbacks")
    print("4. Cache prices to avoid repeated API calls")

def main():
    patterns = analyze_birdeye_data_structure()
    demonstrate_gemini_bug()
    propose_fix()
    
    print(f"\n🎯 CONCLUSION:")
    print("✅ Gemini correctly identified a critical data misunderstanding bug")
    print("✅ Our current dataset (SOL ↔ BNSOL) masks this issue")
    print("🚨 Token-to-token swaps would fail catastrophically with current logic")
    print("🔧 Must implement external SOL price fetching for token-to-token scenarios")

if __name__ == "__main__":
    main()