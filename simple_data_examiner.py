#!/usr/bin/env python3
"""
Simple data examiner to understand BirdEye transaction structure
"""

import json
import requests

def examine_raw_data():
    base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
    headers = {
        "accept": "application/json",
        "x-chain": "solana",
        "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
    }
    
    params = {
        "address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
        "limit": 10
    }
    
    print("🔍 Examining Raw BirdEye Transaction Data")
    print("=" * 50)
    
    response = requests.get(base_url, headers=headers, params=params)
    print(f"Response status: {response.status_code}")
    
    if response.status_code == 200:
        data = response.json()
        print(f"Response keys: {list(data.keys())}")
        print(f"Success: {data.get('success')}")
        
        if data.get("success") and "data" in data:
            items = data["data"].get("items", [])
            print(f"Number of transactions: {len(items)}")
            
            if len(items) > 0:
                print(f"\n📋 First Transaction Structure:")
                tx = items[0]
                print(f"Transaction keys: {list(tx.keys())}")
                
                # Examine quote structure
                if 'quote' in tx:
                    quote = tx['quote']
                    print(f"\nQuote structure:")
                    for key, value in quote.items():
                        print(f"  {key}: {value} (type: {type(value).__name__})")
                
                # Examine base structure
                if 'base' in tx:
                    base = tx['base']
                    print(f"\nBase structure:")
                    for key, value in base.items():
                        print(f"  {key}: {value} (type: {type(value).__name__})")
                
                # Show top-level fields
                print(f"\nTop-level fields:")
                for key, value in tx.items():
                    if key not in ['quote', 'base']:
                        print(f"  {key}: {value}")
                
                # Now analyze 5 transactions for patterns
                print(f"\n🎯 Pattern Analysis of First 5 Transactions:")
                print("=" * 55)
                
                sol_address = "So11111111111111111111111111111111111111112"
                
                for i, transaction in enumerate(items[:5]):
                    print(f"\nTransaction {i+1}:")
                    analyze_transaction_pattern(transaction, sol_address)
            else:
                print("No transactions found in response")
        else:
            print("API call unsuccessful or no data key")
            print(f"Full response: {data}")
    else:
        print(f"Request failed: {response.text}")

def analyze_transaction_pattern(tx, sol_address):
    """Analyze individual transaction pattern"""
    
    quote = tx.get('quote', {})
    base = tx.get('base', {})
    
    # Basic info
    print(f"  Hash: {tx.get('tx_hash', 'unknown')[:20]}...")
    print(f"  Source: {tx.get('source', 'unknown')}")
    
    # Quote analysis
    quote_symbol = quote.get('symbol', 'UNKNOWN')
    quote_change = quote.get('ui_change_amount', 0)
    quote_type_swap = quote.get('type_swap', 'unknown')
    quote_is_sol = quote.get('address') == sol_address
    
    print(f"  Quote: {quote_symbol} | Amount: {quote_change} | Direction: {quote_type_swap} | SOL: {quote_is_sol}")
    
    # Base analysis
    base_symbol = base.get('symbol', 'UNKNOWN')
    base_change = base.get('ui_change_amount', 0)
    base_type_swap = base.get('type_swap', 'unknown')
    base_is_sol = base.get('address') == sol_address
    
    print(f"  Base:  {base_symbol} | Amount: {base_change} | Direction: {base_type_swap} | SOL: {base_is_sol}")
    
    # Direction interpretation
    if quote_change < 0 and base_change > 0:
        direction = f"{quote_symbol} → {base_symbol}"
        spent_token = quote_symbol
        received_token = base_symbol
        spent_amount = abs(quote_change)
        received_amount = base_change
    elif quote_change > 0 and base_change < 0:
        direction = f"{base_symbol} → {quote_symbol}"
        spent_token = base_symbol
        received_token = quote_symbol
        spent_amount = abs(base_change)
        received_amount = quote_change
    else:
        direction = "UNCLEAR"
        spent_token = "unknown"
        received_token = "unknown"
        spent_amount = 0
        received_amount = 0
    
    print(f"  Interpretation: {direction}")
    print(f"  Spent: {spent_amount} {spent_token} | Received: {received_amount} {received_token}")
    
    # SOL involvement and event type
    sol_involved = quote_is_sol or base_is_sol
    
    if sol_involved:
        if quote_is_sol and quote_change < 0:
            event_type = "BUY (SOL → Token)"
            token_bought = base_symbol
            sol_spent = abs(quote_change)
            token_received = base_change
        elif quote_is_sol and quote_change > 0:
            event_type = "SELL (Token → SOL)"
            token_sold = base_symbol
            sol_received = quote_change
            token_spent = abs(base_change)
        elif base_is_sol and base_change > 0:
            event_type = "SELL (Token → SOL)"
            token_sold = quote_symbol
            sol_received = base_change
            token_spent = abs(quote_change)
        elif base_is_sol and base_change < 0:
            event_type = "BUY (SOL → Token)"
            token_bought = quote_symbol
            sol_spent = abs(base_change)
            token_received = quote_change
        else:
            event_type = "UNCLEAR SOL PATTERN"
    else:
        event_type = "TOKEN-TO-TOKEN (dual events needed)"
    
    print(f"  Event Type: {event_type}")
    
    # Mathematical verification
    quote_usd = abs(quote_change) * quote.get('price', 0)
    base_usd = abs(base_change) * base.get('price', 0)
    volume_usd = tx.get('volume_usd', 0)
    
    print(f"  Math: Quote=${quote_usd:.2f} | Base=${base_usd:.2f} | Volume=${volume_usd:.2f}")
    
    return {
        'direction': direction,
        'sol_involved': sol_involved,
        'event_type': event_type,
        'spent_token': spent_token,
        'received_token': received_token
    }

if __name__ == "__main__":
    examine_raw_data()