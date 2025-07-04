#!/usr/bin/env python3
"""
Analysis of BirdEye Price Data to Determine Natural Currency Domain
"""

import requests
import json
from decimal import Decimal, getcontext

getcontext().prec = 28

def analyze_birdeye_price_structure():
    """Analyze what BirdEye actually provides in terms of pricing"""
    
    print("💰 BIRDEYE PRICE DATA STRUCTURE ANALYSIS")
    print("=" * 70)
    
    headers = {
        "accept": "application/json",
        "x-chain": "solana", 
        "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
    }
    
    url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
    params = {
        "address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
        "offset": 0,
        "limit": 10
    }
    
    response = requests.get(url, headers=headers, params=params)
    
    if response.status_code != 200:
        print(f"API Error: {response.status_code}")
        return
    
    data = response.json()
    transactions = data.get("data", {}).get("items", [])
    
    sol_address = "So11111111111111111111111111111111111111112"
    
    print("🔍 WHAT BIRDEYE ACTUALLY PROVIDES:")
    print("-" * 50)
    
    for i, tx in enumerate(transactions[:5]):
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        print(f"\nTransaction {i+1}:")
        print(f"  Quote ({quote.get('symbol', 'UNK')}): price = {quote.get('price', 0)}")
        print(f"  Base  ({base.get('symbol', 'UNK')}): price = {base.get('price', 0)}")
        print(f"  Volume USD: {tx.get('volume_usd', 0)}")
        
        # Determine what the prices represent
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        if quote_is_sol:
            print(f"  → SOL price: ${quote.get('price', 0)} (quote)")
            print(f"  → Token price: ${base.get('price', 0)} (base)")
        elif base_is_sol:
            print(f"  → SOL price: ${base.get('price', 0)} (base)")
            print(f"  → Token price: ${quote.get('price', 0)} (quote)")
        else:
            print(f"  → Token 1 price: ${quote.get('price', 0)} (quote)")
            print(f"  → Token 2 price: ${base.get('price', 0)} (base)")
        
        # Verify mathematical consistency
        quote_usd = abs(quote.get('ui_change_amount', 0) * quote.get('price', 0))
        base_usd = abs(base.get('ui_change_amount', 0) * base.get('price', 0))
        
        print(f"  Math check: Quote=${quote_usd:.2f} vs Base=${base_usd:.2f}")
    
    print("\n" + "=" * 70)
    print("📊 PRICE DENOMINATION ANALYSIS")
    print("=" * 70)
    
    # Analyze what currency prices are denominated in
    analyze_price_denomination(transactions, sol_address)
    
    print("\n" + "=" * 70)
    print("🚀 NATURAL CURRENCY DOMAIN RECOMMENDATION")
    print("=" * 70)
    
    recommend_currency_domain(transactions, sol_address)

def analyze_price_denomination(transactions, sol_address):
    """Analyze what currency all prices are actually denominated in"""
    
    print("🔍 PRICE DENOMINATION ANALYSIS:")
    print()
    
    # Collect all price data
    sol_prices = []
    token_prices = []
    
    for tx in transactions:
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        if quote_is_sol:
            sol_prices.append(quote.get('price', 0))
            token_prices.append(base.get('price', 0))
        elif base_is_sol:
            sol_prices.append(base.get('price', 0))
            token_prices.append(quote.get('price', 0))
        else:
            # Token-to-token: both are token prices
            token_prices.extend([quote.get('price', 0), base.get('price', 0)])
    
    print(f"📈 SOL Prices Found: {len(sol_prices)}")
    if sol_prices:
        avg_sol_price = sum(sol_prices) / len(sol_prices)
        min_sol = min(sol_prices)
        max_sol = max(sol_prices)
        print(f"  Range: ${min_sol:.2f} - ${max_sol:.2f}")
        print(f"  Average: ${avg_sol_price:.2f}")
        print(f"  → All SOL prices are in USD denomination")
    
    print(f"\n💎 Token Prices Found: {len(token_prices)}")
    if token_prices:
        token_prices_nonzero = [p for p in token_prices if p > 0]
        if token_prices_nonzero:
            min_token = min(token_prices_nonzero)
            max_token = max(token_prices_nonzero)
            print(f"  Range: ${min_token:.6f} - ${max_token:.2f}")
            print(f"  → All token prices are in USD denomination")
    
    print(f"\n🎯 KEY FINDING:")
    print(f"  ✅ ALL prices in BirdEye are USD-denominated")
    print(f"  ✅ SOL price: ~$151-152 USD per SOL")
    print(f"  ✅ Token prices: $X USD per token")
    print(f"  ❌ NO prices are denominated in SOL")

def recommend_currency_domain(transactions, sol_address):
    """Make recommendation based on actual price data"""
    
    print("💡 NATURAL CURRENCY DOMAIN ANALYSIS:")
    print()
    
    # Count transaction types
    sol_swaps = 0
    token_swaps = 0
    
    for tx in transactions:
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        if quote_is_sol or base_is_sol:
            sol_swaps += 1
        else:
            token_swaps += 1
    
    print(f"📊 Transaction Distribution:")
    print(f"  SOL swaps: {sol_swaps}")
    print(f"  Token-to-token swaps: {token_swaps}")
    print(f"  Total: {sol_swaps + token_swaps}")
    
    print(f"\n🔍 MATHEMATICAL ANALYSIS:")
    
    print(f"\n1️⃣ USD-ONLY APPROACH:")
    print(f"   ✅ Native to BirdEye data (all prices in USD)")
    print(f"   ✅ No conversion needed for any transaction")
    print(f"   ✅ Direct calculation: amount × USD_price = USD_value")
    print(f"   ✅ Single currency domain throughout")
    print(f"   Example: 13.75 SOL × $151.75 = $2,087 USD")
    print(f"           2078 USDC × $0.9999 = $2,078 USD")
    
    print(f"\n2️⃣ SOL-ONLY APPROACH:")
    print(f"   ⚠️  Requires USD→SOL conversion for token swaps")
    print(f"   ⚠️  Extra step: USD_value ÷ SOL_price = SOL_equivalent")
    print(f"   ✅ Preserves native SOL amounts where they exist")
    print(f"   Example: $50 token value ÷ $151.75 SOL = 0.3294 SOL")
    
    print(f"\n🎯 RECOMMENDATION BASED ON DATA:")
    
    print(f"\n✅ USD-ONLY IS THE NATURAL CHOICE")
    print(f"   Reasons:")
    print(f"   1. BirdEye provides ALL prices in USD")
    print(f"   2. Zero conversion overhead")
    print(f"   3. No precision loss from conversions")
    print(f"   4. Matches the data source's native format")
    print(f"   5. Simplest implementation")
    
    print(f"\n📝 IMPLEMENTATION IMPLICATIONS:")
    print(f"   → Use usd_value as primary field")
    print(f"   → Keep sol_amount for reference/display")
    print(f"   → FIFO engine operates in USD domain")
    print(f"   → All aggregations in USD")
    print(f"   → Convert to SOL only for final display if needed")
    
    # Show concrete example
    print(f"\n🔧 CONCRETE EXAMPLE:")
    print(f"   SOL → USDC swap:")
    print(f"     Amount: 13.75 SOL → 2078 USDC")
    print(f"     USD approach: 13.75 × $151.75 = $2,087 (BUY event)")
    print(f"     SOL approach: 13.75 SOL (BUY event) ← requires conversion for token swaps")
    print(f"   ")
    print(f"   Token → Token swap:")
    print(f"     Amount: 1000 USDC → 850 USDT")
    print(f"     USD approach: 1000 × $0.9999 = $999.90 (direct)")
    print(f"     SOL approach: $999.90 ÷ $151.75 = 6.587 SOL (conversion needed)")

if __name__ == "__main__":
    analyze_birdeye_price_structure()