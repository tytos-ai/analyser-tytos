#!/usr/bin/env python3
"""
Debug Rust event generation to understand why 511 events are created vs Python's 104
"""

import json
import requests
from decimal import Decimal, getcontext

getcontext().prec = 28

print("🚨 DEBUGGING RUST EVENT GENERATION ISSUE")
print("=" * 60)

print("📊 EXPECTED FROM BIRDEYE DATA:")
print("- 100 unique transactions")
print("- Each SOL ↔ Token swap = 1 event")
print("- Each Token ↔ Token swap = 2 events")
print("- Python total: 104 events from 100 transactions")

print("\n🔍 RUST PROBLEM:")
print("- 511 total events generated (5x too many!)")
print("- 102 events processed + 409 filtered = 511 total")
print("- This suggests massive over-generation in ProcessedSwap logic")

print("\n🎯 ANALYSIS PLAN:")
print("1. Check if BirdEye returns multiple entries per tx_hash")
print("2. Verify Rust aggregation logic isn't creating duplicate swaps")
print("3. Check if dual event logic is creating too many events")

# Load sample data
with open('manual_verification_transactions.json', 'r') as f:
    data = json.load(f)

transactions = data['data']['items']

print(f"\n📈 BIRDEYE DATA VERIFICATION:")
print(f"Total entries: {len(transactions)}")

# Check for duplicates
tx_hashes = [tx['tx_hash'] for tx in transactions]
unique_hashes = set(tx_hashes)
print(f"Unique tx_hashes: {len(unique_hashes)}")

if len(tx_hashes) != len(unique_hashes):
    print("❌ DUPLICATE TRANSACTION HASHES FOUND!")
    # Find duplicates
    hash_counts = {}
    for tx_hash in tx_hashes:
        hash_counts[tx_hash] = hash_counts.get(tx_hash, 0) + 1
    
    duplicates = {h: c for h, c in hash_counts.items() if c > 1}
    print(f"Duplicates: {duplicates}")
else:
    print("✅ No duplicate transaction hashes - each tx is unique")

print(f"\n🔄 TRANSACTION TYPE ANALYSIS:")

sol_address = "So11111111111111111111111111111111111111112"
event_count = 0
transaction_types = {
    'sol_to_token': 0,
    'token_to_sol': 0, 
    'token_to_token': 0
}

for i, tx in enumerate(transactions):  # Analyze ALL transactions
    quote = tx['quote']
    base = tx['base']
    
    quote_change = Decimal(str(quote['ui_change_amount']))
    base_change = Decimal(str(base['ui_change_amount']))
    
    if i < 10 or i % 10 == 9:  # Print first 10 and every 10th after
        print(f"\nTransaction {i+1}: {tx['tx_hash'][:16]}...")
        print(f"  Quote: {quote['symbol']} change {quote_change:+.6f}")
        print(f"  Base: {base['symbol']} change {base_change:+.6f}")
    
    # Determine transaction type and expected events
    if quote['address'] == sol_address or base['address'] == sol_address:
        if (quote['address'] == sol_address and quote_change < 0) or (base['address'] == sol_address and base_change < 0):
            if i < 10 or i % 10 == 9:
                print(f"  → SOL → Token: 1 BUY event expected")
            transaction_types['sol_to_token'] += 1
            event_count += 1
        else:
            if i < 10 or i % 10 == 9:
                print(f"  → Token → SOL: 1 SELL event expected")
            transaction_types['token_to_sol'] += 1
            event_count += 1
    else:
        if i < 10 or i % 10 == 9:
            print(f"  → Token → Token: 2 events expected (SELL + BUY)")
        transaction_types['token_to_token'] += 1
        event_count += 2

print(f"\n📊 ACTUAL EVENT GENERATION (ALL 100 transactions):")
print(f"SOL ↔ Token swaps: {transaction_types['sol_to_token'] + transaction_types['token_to_sol']} (1 event each)")
print(f"  - SOL → Token: {transaction_types['sol_to_token']}")
print(f"  - Token → SOL: {transaction_types['token_to_sol']}")
print(f"Token ↔ Token swaps: {transaction_types['token_to_token']} (2 events each)")
print(f"Total expected events: {event_count}")
print(f"Python actual: 104 events {'✅' if event_count == 104 else '❓'}")
print(f"Rust actual: 511 events ❌ (5x over-generation!)")

print(f"\n🚨 ROOT CAUSE HYPOTHESIS:")
print("1. ❌ Rust ProcessedSwap aggregation creating multiple swaps per transaction")
print("2. ❌ Rust dual event logic incorrectly applied") 
print("3. ❌ Rust processing same transactions multiple times")
print("4. ❌ Event filtering logic counting wrong")

print(f"\n🔧 NEXT DEBUGGING STEPS:")
print("1. Add detailed logging to ProcessedSwap::from_birdeye_transactions_with_embedded_prices")
print("2. Count how many ProcessedSwaps are created from 100 BirdEye transactions")
print("3. Count how many FinancialEvents are created from ProcessedSwaps")
print("4. Verify the aggregation logic isn't creating duplicate swaps")