#!/usr/bin/env python3

import json
from collections import defaultdict

def analyze_transaction_structure():
    """Analyze Birdeye transaction data to understand its true structure"""
    
    # Load transaction data
    with open('transactions_4GQeEya6ZTwvXre4Br6ZfDyfe2WQMkcDz2QbkJZazVqS.json', 'r') as f:
        data = json.load(f)
    
    transactions = data['data']['items']
    
    print("=== BIRDEYE TRANSACTION DATA STRUCTURE ANALYSIS ===")
    print(f"Total transactions: {len(transactions)}")
    print()
    
    # Analyze patterns
    patterns = defaultdict(int)
    sol_patterns = defaultdict(int)
    
    for tx in transactions:
        quote_symbol = tx['quote']['symbol']
        quote_type = tx['quote']['type_swap']
        quote_change = tx['quote']['ui_change_amount']
        
        base_symbol = tx['base']['symbol']
        base_type = tx['base']['type_swap']
        base_change = tx['base']['ui_change_amount']
        
        # Pattern analysis
        pattern = f"{quote_symbol}({quote_type}) <-> {base_symbol}({base_type})"
        patterns[pattern] += 1
        
        # SOL involvement analysis
        if quote_symbol == 'SOL':
            sol_pattern = f"SOL({quote_type}) <-> {base_symbol}({base_type})"
            sol_patterns[sol_pattern] += 1
        elif base_symbol == 'SOL':
            sol_pattern = f"{quote_symbol}({quote_type}) <-> SOL({base_type})"
            sol_patterns[sol_pattern] += 1
    
    print("=== TRANSACTION PATTERNS ===")
    for pattern, count in sorted(patterns.items(), key=lambda x: x[1], reverse=True)[:10]:
        print(f"  {pattern}: {count}")
    
    print("\n=== SOL INVOLVEMENT PATTERNS ===")
    for pattern, count in sorted(sol_patterns.items(), key=lambda x: x[1], reverse=True):
        print(f"  {pattern}: {count}")
    
    print("\n=== DETAILED ANALYSIS OF FIRST 10 TRANSACTIONS ===")
    for i, tx in enumerate(transactions[:10]):
        print(f"\nTransaction {i+1}: {tx['tx_hash'][:8]}...")
        
        quote_symbol = tx['quote']['symbol']
        quote_change = tx['quote']['ui_change_amount']
        quote_type = tx['quote']['type_swap']
        
        base_symbol = tx['base']['symbol']
        base_change = tx['base']['ui_change_amount']
        base_type = tx['base']['type_swap']
        
        volume_usd = tx['volume_usd']
        
        print(f"  Quote: {quote_symbol} {quote_change:+.6f} ({quote_type})")
        print(f"  Base:  {base_symbol} {base_change:+.6f} ({base_type})")
        print(f"  Volume USD: ${volume_usd:.2f}")
        
        # Economic interpretation
        if quote_type == 'from' and base_type == 'to':
            print(f"  💡 Economic: User traded {abs(quote_change)} {quote_symbol} → {base_change} {base_symbol}")
            print(f"  💡 P&L Event: BUY {base_symbol} (cost: {abs(quote_change)} {quote_symbol})")
        elif quote_type == 'to' and base_type == 'from':
            print(f"  💡 Economic: User traded {abs(base_change)} {base_symbol} → {quote_change} {quote_symbol}")
            print(f"  💡 P&L Event: SELL {base_symbol} (received: {quote_change} {quote_symbol})")
        else:
            print(f"  ❓ Unclear pattern: {quote_type} / {base_type}")
    
    print("\n=== P&L IMPLICATIONS ===")
    print("Key insights for P&L calculation:")
    print("1. Each transaction represents ONE economic event")
    print("2. Quote + Base represent the two sides of the same trade")
    print("3. 'from' = what user gave up, 'to' = what user received")
    print("4. For P&L, we need to track the ASSET being acquired/disposed")
    print("5. SOL often acts as the medium of exchange (like USD)")

if __name__ == "__main__":
    analyze_transaction_structure()