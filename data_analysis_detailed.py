#!/usr/bin/env python3
"""
Detailed BirdEye Data Analysis for Currency Domain Decision
"""

import requests
import json
from decimal import Decimal, getcontext

# Set high precision for financial calculations
getcontext().prec = 28

def analyze_birdeye_data():
    """Analyze BirdEye data structure and currency information"""
    
    headers = {
        "accept": "application/json",
        "x-chain": "solana", 
        "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
    }
    
    url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
    params = {
        "address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
        "offset": 0,
        "limit": 20  # Get more samples
    }
    
    response = requests.get(url, headers=headers, params=params)
    
    if response.status_code != 200:
        print(f"API Error: {response.status_code}")
        return
    
    data = response.json()
    if not data.get("success"):
        print("API call unsuccessful")
        return
    
    transactions = data.get("data", {}).get("items", [])
    print(f"🔍 Analyzing {len(transactions)} transactions for currency patterns")
    print("=" * 80)
    
    sol_address = "So11111111111111111111111111111111111111112"
    
    # Categorize transactions
    sol_swaps = []
    token_swaps = []
    
    for i, tx in enumerate(transactions):
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        # Classify transaction type
        if quote_is_sol or base_is_sol:
            sol_swaps.append(tx)
        else:
            token_swaps.append(tx)
    
    print(f"📊 Transaction Classification:")
    print(f"  SOL Swaps: {len(sol_swaps)}")
    print(f"  Token-to-Token Swaps: {len(token_swaps)}")
    print()
    
    # Analyze SOL swaps
    print("🟡 SOL SWAP ANALYSIS:")
    print("=" * 40)
    analyze_sol_swaps(sol_swaps, sol_address)
    
    print("\n🔴 TOKEN-TO-TOKEN SWAP ANALYSIS:")
    print("=" * 40)
    analyze_token_swaps(token_swaps)
    
    # Analyze price data availability
    print("\n💰 PRICE DATA ANALYSIS:")
    print("=" * 40)
    analyze_price_data(transactions)

def analyze_sol_swaps(sol_swaps, sol_address):
    """Analyze SOL swaps for currency domain patterns"""
    
    for i, tx in enumerate(sol_swaps[:5]):  # Analyze first 5
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        print(f"\nSOL Swap {i+1}:")
        print(f"  Hash: {tx.get('tx_hash', '')[:20]}...")
        
        if base_is_sol:
            # SOL is in base (spent or received)
            sol_amount = base.get('ui_change_amount', 0)
            sol_price = base.get('price', 0)
            token_amount = quote.get('ui_change_amount', 0)
            token_price = quote.get('price', 0)
            token_symbol = quote.get('symbol', 'UNKNOWN')
            
            print(f"  SOL: {sol_amount} @ ${sol_price:.2f} = ${sol_amount * sol_price:.2f}")
            print(f"  {token_symbol}: {token_amount} @ ${token_price:.6f} = ${token_amount * token_price:.2f}")
            
        else:
            # SOL is in quote (spent or received)
            sol_amount = quote.get('ui_change_amount', 0) 
            sol_price = quote.get('price', 0)
            token_amount = base.get('ui_change_amount', 0)
            token_price = base.get('price', 0)
            token_symbol = base.get('symbol', 'UNKNOWN')
            
            print(f"  SOL: {sol_amount} @ ${sol_price:.2f} = ${sol_amount * sol_price:.2f}")
            print(f"  {token_symbol}: {token_amount} @ ${token_price:.6f} = ${token_amount * token_price:.2f}")
        
        # Check mathematical consistency
        sol_usd_value = abs(sol_amount * sol_price)
        token_usd_value = abs(token_amount * token_price)
        difference = abs(sol_usd_value - token_usd_value)
        percentage_diff = (difference / max(sol_usd_value, token_usd_value)) * 100 if max(sol_usd_value, token_usd_value) > 0 else 0
        
        print(f"  Value Check: SOL=${sol_usd_value:.2f} vs Token=${token_usd_value:.2f}")
        print(f"  Difference: ${difference:.2f} ({percentage_diff:.2f}%)")
        
        # Top-level volume check
        volume_usd = tx.get('volume_usd', 0)
        print(f"  Volume USD: ${volume_usd:.2f}")
        
        # Analysis: Can we derive all USD values reliably?
        print(f"  ✅ SOL USD Value: ${sol_usd_value:.2f} (SOL amount × SOL price)")
        print(f"  ✅ Token USD Value: ${token_usd_value:.2f} (Token amount × Token price)")
        print(f"  ✅ Volume USD: ${volume_usd:.2f} (BirdEye calculated)")

def analyze_token_swaps(token_swaps):
    """Analyze token-to-token swaps for currency domain patterns"""
    
    for i, tx in enumerate(token_swaps[:5]):  # Analyze first 5
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        print(f"\nToken Swap {i+1}:")
        print(f"  Hash: {tx.get('tx_hash', '')[:20]}...")
        
        # Extract token information
        quote_symbol = quote.get('symbol', 'UNKNOWN')
        quote_amount = quote.get('ui_change_amount', 0)
        quote_price = quote.get('price', 0)
        
        base_symbol = base.get('symbol', 'UNKNOWN')
        base_amount = base.get('ui_change_amount', 0)
        base_price = base.get('price', 0)
        
        print(f"  Quote: {quote_symbol} {quote_amount} @ ${quote_price:.6f} = ${quote_amount * quote_price:.2f}")
        print(f"  Base:  {base_symbol} {base_amount} @ ${base_price:.6f} = ${base_amount * base_price:.2f}")
        
        # Check mathematical consistency
        quote_usd_value = abs(quote_amount * quote_price)
        base_usd_value = abs(base_amount * base_price)
        difference = abs(quote_usd_value - base_usd_value)
        percentage_diff = (difference / max(quote_usd_value, base_usd_value)) * 100 if max(quote_usd_value, base_usd_value) > 0 else 0
        
        print(f"  Value Check: Quote=${quote_usd_value:.2f} vs Base=${base_usd_value:.2f}")
        print(f"  Difference: ${difference:.2f} ({percentage_diff:.2f}%)")
        
        # Top-level volume check
        volume_usd = tx.get('volume_usd', 0)
        print(f"  Volume USD: ${volume_usd:.2f}")
        
        # Analysis: What SOL information is available?
        print(f"  ❌ No direct SOL amounts in token-to-token swaps")
        print(f"  ✅ Quote USD Value: ${quote_usd_value:.2f} (Quote amount × Quote price)")
        print(f"  ✅ Base USD Value: ${base_usd_value:.2f} (Base amount × Base price)")
        print(f"  ✅ Volume USD: ${volume_usd:.2f} (BirdEye calculated)")
        print(f"  ❓ SOL Equivalent: Would need external SOL price conversion")

def analyze_price_data(transactions):
    """Analyze price data availability and consistency"""
    
    print("Price Data Sources Available:")
    print("-" * 30)
    
    for i, tx in enumerate(transactions[:3]):
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        print(f"\nTransaction {i+1}:")
        print(f"  Quote Price: ${quote.get('price', 0):.6f} (embedded in quote)")
        print(f"  Base Price: ${base.get('price', 0):.6f} (embedded in base)")
        print(f"  Top-level quote_price: ${tx.get('quote_price', 0):.6f}")
        print(f"  Top-level base_price: ${tx.get('base_price', 0):.6f}")
        print(f"  Volume USD: ${tx.get('volume_usd', 0):.2f}")
        print(f"  Volume: {tx.get('volume', 0)} (presumably in base token units)")
        
        # Verify price consistency
        quote_price_match = quote.get('price', 0) == tx.get('quote_price', 0)
        base_price_match = base.get('price', 0) == tx.get('base_price', 0)
        
        print(f"  ✅ Quote price consistency: {quote_price_match}")
        print(f"  ✅ Base price consistency: {base_price_match}")

if __name__ == "__main__":
    analyze_birdeye_data()