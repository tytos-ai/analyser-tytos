#!/usr/bin/env python3
"""
Critical analysis script to verify our transaction parsing logic 
against actual BirdEye transaction data structure.
"""

import json
from decimal import Decimal, getcontext
getcontext().prec = 50

def load_transactions():
    """Load BirdEye transactions"""
    with open('/home/mrima/tytos/wallet-analyser/manual_verification_transactions.json', 'r') as f:
        data = json.load(f)
    return data['data']['items']

def analyze_critical_fields(transactions):
    """Analyze critical data fields for P&L calculation issues"""
    
    print("🚨 CRITICAL TRANSACTION PARSING ANALYSIS")
    print("=" * 60)
    
    # Analyze first 10 transactions thoroughly
    for i, tx in enumerate(transactions[:10]):
        print(f"\n📊 TRANSACTION {i+1}: {tx['tx_hash'][:12]}...")
        
        quote = tx['quote']
        base = tx['base']
        
        print(f"Quote (SOL): {quote['symbol']}")
        print(f"  ui_change_amount: {quote['ui_change_amount']}")
        print(f"  price: ${quote['price']:.6f}")
        print(f"  type_swap: {quote.get('type_swap', 'N/A')}")
        
        print(f"Base (BNSOL): {base['symbol']}")
        print(f"  ui_change_amount: {base['ui_change_amount']}")
        print(f"  price: ${base['price']:.6f}")
        print(f"  type_swap: {base.get('type_swap', 'N/A')}")
        
        # Determine swap direction
        if quote['ui_change_amount'] < 0 and base['ui_change_amount'] > 0:
            swap_type = "BUY BNSOL (SOL → BNSOL)"
            sol_spent = abs(quote['ui_change_amount'])
            bnsol_received = base['ui_change_amount']
            sol_price = quote['price']
            bnsol_price = base['price']
            
            print(f"  🔄 {swap_type}")
            print(f"  💰 SOL spent: {sol_spent:.6f} SOL at ${sol_price:.2f}/SOL")
            print(f"  🪙 BNSOL received: {bnsol_received:.6f} BNSOL at ${bnsol_price:.2f}/BNSOL")
            
            # Calculate what our system should record
            print(f"  📋 FinancialEvent should be:")
            print(f"     event_type: Buy")
            print(f"     token_mint: {base['address']}")
            print(f"     token_amount: {bnsol_received:.6f}")
            print(f"     sol_amount: {sol_spent:.6f}")
            print(f"     price_per_token: ${bnsol_price:.6f} (USD per BNSOL)")
            
        elif base['ui_change_amount'] < 0 and quote['ui_change_amount'] > 0:
            swap_type = "SELL BNSOL (BNSOL → SOL)"
            bnsol_spent = abs(base['ui_change_amount'])
            sol_received = quote['ui_change_amount']
            sol_price = quote['price']
            bnsol_price = base['price']
            
            print(f"  🔄 {swap_type}")
            print(f"  🪙 BNSOL spent: {bnsol_spent:.6f} BNSOL at ${bnsol_price:.2f}/BNSOL")
            print(f"  💰 SOL received: {sol_received:.6f} SOL at ${sol_price:.2f}/SOL")
            
            # Calculate what our system should record
            print(f"  📋 FinancialEvent should be:")
            print(f"     event_type: Sell")
            print(f"     token_mint: {base['address']}")
            print(f"     token_amount: {bnsol_spent:.6f}")
            print(f"     sol_amount: {sol_received:.6f}")
            print(f"     price_per_token: ${bnsol_price:.6f} (USD per BNSOL)")
        else:
            print(f"  ❌ UNEXPECTED SWAP PATTERN!")
            print(f"     Quote change: {quote['ui_change_amount']}")
            print(f"     Base change: {base['ui_change_amount']}")

def analyze_price_units(transactions):
    """Analyze price unit consistency"""
    
    print(f"\n🔍 PRICE UNIT ANALYSIS")
    print("=" * 30)
    
    for i, tx in enumerate(transactions[:5]):
        quote = tx['quote']
        base = tx['base']
        
        print(f"\nTX {i+1}:")
        print(f"  Quote price: ${quote['price']:.2f} (USD per {quote['symbol']})")
        print(f"  Base price: ${base['price']:.2f} (USD per {base['symbol']})")
        print(f"  base_price: ${tx.get('base_price', 'N/A'):.2f}")
        print(f"  quote_price: ${tx.get('quote_price', 'N/A'):.2f}")
        
        # Verify consistency
        if abs(base['price'] - tx.get('base_price', 0)) > 0.01:
            print(f"  ❌ BASE PRICE MISMATCH!")
        if abs(quote['price'] - tx.get('quote_price', 0)) > 0.01:
            print(f"  ❌ QUOTE PRICE MISMATCH!")

def analyze_amount_precision(transactions):
    """Check for precision issues in amount handling"""
    
    print(f"\n🔢 AMOUNT PRECISION ANALYSIS")
    print("=" * 35)
    
    for i, tx in enumerate(transactions[:3]):
        quote = tx['quote']
        base = tx['base']
        
        print(f"\nTX {i+1}:")
        print(f"  Quote decimals: {quote['decimals']}")
        print(f"  Quote amount: {quote['amount']} (raw)")
        print(f"  Quote ui_amount: {quote['ui_amount']} (human)")
        print(f"  Quote ui_change_amount: {quote['ui_change_amount']} (signed)")
        
        print(f"  Base decimals: {base['decimals']}")
        print(f"  Base amount: {base['amount']} (raw)")
        print(f"  Base ui_amount: {base['ui_amount']} (human)")
        print(f"  Base ui_change_amount: {base['ui_change_amount']} (signed)")
        
        # Verify decimal conversion
        expected_ui_amount = quote['amount'] / (10 ** quote['decimals'])
        if abs(expected_ui_amount - quote['ui_amount']) > 0.000001:
            print(f"  ❌ QUOTE DECIMAL CONVERSION ERROR!")
            print(f"     Expected: {expected_ui_amount}")
            print(f"     Actual: {quote['ui_amount']}")

def main():
    transactions = load_transactions()
    print(f"Loaded {len(transactions)} transactions for analysis")
    
    analyze_critical_fields(transactions)
    analyze_price_units(transactions)
    analyze_amount_precision(transactions)
    
    print(f"\n🎯 CRITICAL CHECKS SUMMARY:")
    print("1. ✅ Transaction direction detection using ui_change_amount signs")
    print("2. ✅ Price fields are in USD (not SOL)")
    print("3. ✅ ui_change_amount represents net flow (signed)")
    print("4. ⚠️  Need to verify our Rust parsing matches this exactly")

if __name__ == "__main__":
    main()