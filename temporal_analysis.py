#!/usr/bin/env python3
"""
Temporal Consistency Analysis for Currency Domain Selection
"""

import requests
from decimal import Decimal, getcontext
getcontext().prec = 28

def analyze_temporal_consistency():
    """Analyze SOL price consistency across different transactions"""
    
    print("⏰ TEMPORAL CONSISTENCY ANALYSIS")
    print("=" * 60)
    
    # Get sample transactions with timestamps
    headers = {
        "accept": "application/json",
        "x-chain": "solana", 
        "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
    }
    
    url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
    params = {
        "address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
        "offset": 0,
        "limit": 15
    }
    
    response = requests.get(url, headers=headers, params=params)
    
    if response.status_code != 200:
        print(f"API Error: {response.status_code}")
        return
    
    data = response.json()
    transactions = data.get("data", {}).get("items", [])
    
    sol_address = "So11111111111111111111111111111111111111112"
    
    # Extract SOL prices from different transactions
    sol_prices = []
    
    for tx in transactions:
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        timestamp = tx.get('block_unix_time', 0)
        
        sol_price = None
        if quote.get('address') == sol_address:
            sol_price = quote.get('price', 0)
        elif base.get('address') == sol_address:
            sol_price = base.get('price', 0)
        
        if sol_price:
            sol_prices.append({
                'timestamp': timestamp,
                'price': Decimal(str(sol_price)),
                'tx_hash': tx.get('tx_hash', '')[:20] + '...'
            })
    
    print(f"📊 SOL PRICE VARIATION ACROSS {len(sol_prices)} TRANSACTIONS:")
    print("-" * 60)
    
    if not sol_prices:
        print("No SOL prices found in transactions")
        return
    
    for i, price_data in enumerate(sol_prices):
        from datetime import datetime
        dt = datetime.fromtimestamp(price_data['timestamp'])
        print(f"  {i+1}. ${price_data['price']:.2f} at {dt.strftime('%H:%M:%S')} ({price_data['tx_hash']})")
    
    # Calculate price statistics
    prices = [p['price'] for p in sol_prices]
    min_price = min(prices)
    max_price = max(prices)
    avg_price = sum(prices) / len(prices)
    price_range = max_price - min_price
    price_variance_pct = (price_range / avg_price) * 100
    
    print(f"\n📈 PRICE STATISTICS:")
    print(f"  Min Price: ${min_price:.2f}")
    print(f"  Max Price: ${max_price:.2f}")
    print(f"  Average:   ${avg_price:.2f}")
    print(f"  Range:     ${price_range:.2f}")
    print(f"  Variance:  {price_variance_pct:.2f}%")
    
    # Analyze the impact on different approaches
    print(f"\n🔍 IMPACT ANALYSIS:")
    
    # Simulate a token-to-token swap with different SOL prices
    token_usd_value = Decimal("1000.00")  # $1000 token swap
    
    print(f"\nExample: $1000 token-to-token swap")
    print(f"SOL equivalent using different prices:")
    
    for i, price_data in enumerate(sol_prices[:3]):  # Show first 3
        sol_equiv = token_usd_value / price_data['price']
        print(f"  Price ${price_data['price']:.2f}: {sol_equiv:.6f} SOL")
    
    # Show the variation impact
    sol_equiv_min = token_usd_value / max_price  # Lower price = more SOL
    sol_equiv_max = token_usd_value / min_price  # Higher price = less SOL
    sol_equiv_variance = ((sol_equiv_max - sol_equiv_min) / sol_equiv_min) * 100
    
    print(f"\n⚠️  SOL EQUIVALENT VARIANCE:")
    print(f"  Range: {sol_equiv_min:.6f} - {sol_equiv_max:.6f} SOL")
    print(f"  Variance: {sol_equiv_variance:.2f}%")
    
    return {
        'price_variance_pct': price_variance_pct,
        'sol_equiv_variance_pct': sol_equiv_variance,
        'avg_price': avg_price,
        'price_count': len(sol_prices)
    }

def analyze_feasibility():
    """Analyze feasibility of each approach"""
    
    print("\n" + "=" * 60)
    print("🚀 FEASIBILITY ANALYSIS")
    print("=" * 60)
    
    temporal_data = analyze_temporal_consistency()
    
    print(f"\n📋 FEASIBILITY ASSESSMENT:")
    
    print(f"\n1️⃣  DUAL CURRENCY APPROACH (Current):")
    print(f"   ✅ Technically feasible")
    print(f"   ❌ Mathematically inconsistent")
    print(f"   ❌ Currency mixing in aggregation")
    print(f"   🔧 COMPLEXITY: High (dual engines)")
    print(f"   📊 ACCURACY: Poor (mixing errors)")
    
    print(f"\n2️⃣  USD-ONLY APPROACH:")
    print(f"   ✅ Technically feasible")
    print(f"   ✅ Mathematically consistent")
    print(f"   ✅ BirdEye provides all USD prices")
    print(f"   ✅ Single currency domain")
    print(f"   ⚠️  SOL amounts converted to USD")
    print(f"   🔧 COMPLEXITY: Low (single engine)")
    print(f"   📊 ACCURACY: High (native USD prices)")
    
    if temporal_data:
        print(f"\n3️⃣  SOL-ONLY APPROACH:")
        print(f"   ✅ Technically feasible")
        print(f"   ✅ Mathematically consistent")
        print(f"   ✅ Preserves native SOL amounts")
        print(f"   ⚠️  SOL price variance: {temporal_data['price_variance_pct']:.2f}%")
        print(f"   ⚠️  Impact on token swaps: ±{temporal_data['sol_equiv_variance_pct']:.2f}% variance")
        print(f"   🔧 COMPLEXITY: Medium (price selection logic)")
        print(f"   📊 ACCURACY: Dependent on SOL price consistency")
    
    print(f"\n🎯 FINAL RECOMMENDATION:")
    
    if temporal_data and temporal_data['price_variance_pct'] < 5.0:
        print(f"   SOL price variance is LOW ({temporal_data['price_variance_pct']:.2f}%)")
        print(f"   ✅ SOL-ONLY approach is mathematically viable")
        print(f"   ✅ Preserves native SOL amounts")
        print(f"   ✅ Minimal conversion errors")
    else:
        print(f"   SOL price variance is HIGH ({temporal_data.get('price_variance_pct', 'unknown'):.2f}%)")
        print(f"   ✅ USD-ONLY approach is safer")
        print(f"   ✅ Leverages native USD price data")
        print(f"   ✅ Eliminates SOL price dependency")
    
    print(f"\n📝 IMPLEMENTATION PRIORITY:")
    print(f"   1. Fix current dual currency issues")
    print(f"   2. Implement USD-only approach (safest)")
    print(f"   3. Consider SOL-only if SOL price stability improves")

if __name__ == "__main__":
    analyze_feasibility()