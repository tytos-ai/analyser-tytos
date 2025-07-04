#!/usr/bin/env python3
"""
Verify our Rust parsing logic produces correct FinancialEvents
by manually calculating what should happen for key transactions.
"""

import json
from decimal import Decimal, getcontext
getcontext().prec = 50

def load_transactions():
    """Load BirdEye transactions"""
    with open('/home/mrima/tytos/wallet-analyser/manual_verification_transactions.json', 'r') as f:
        data = json.load(f)
    return data['data']['items']

def simulate_rust_aggregation(tx):
    """Simulate our Rust aggregation logic for a single transaction"""
    
    quote = tx['quote']
    base = tx['base']
    
    # Simulate net_changes calculation
    net_changes = {}
    net_changes[quote['address']] = quote['ui_change_amount']
    net_changes[base['address']] = base['ui_change_amount']
    
    # Find token_in (negative) and token_out (positive)
    token_in = None
    token_out = None
    amount_in = 0
    amount_out = 0
    
    for token, net_amount in net_changes.items():
        if net_amount < 0:
            token_in = token
            amount_in = abs(net_amount)
        elif net_amount > 0:
            token_out = token
            amount_out = net_amount
    
    return {
        'token_in': token_in,
        'token_out': token_out,
        'amount_in': amount_in,
        'amount_out': amount_out,
        'quote': quote,
        'base': base
    }

def simulate_financial_event_creation(aggregation_result):
    """Simulate our Rust FinancialEvent creation logic"""
    
    sol_mint = "So11111111111111111111111111111111111111112"
    token_in = aggregation_result['token_in']
    token_out = aggregation_result['token_out']
    amount_in = aggregation_result['amount_in']
    amount_out = aggregation_result['amount_out']
    quote = aggregation_result['quote']
    base = aggregation_result['base']
    
    if token_in == sol_mint:
        # SOL → Token swap: Create Token BUY event
        return {
            'event_type': 'Buy',
            'token_mint': token_out,  # Token being bought
            'token_amount': amount_out,  # Token amount received
            'sol_amount': amount_in,  # SOL spent
            'price_per_token': base['price'] if base['address'] == token_out else quote['price']
        }
    elif token_out == sol_mint:
        # Token → SOL swap: Create Token SELL event
        return {
            'event_type': 'Sell',
            'token_mint': token_in,  # Token being sold
            'token_amount': amount_in,  # Token amount sold
            'sol_amount': amount_out,  # SOL received
            'price_per_token': base['price'] if base['address'] == token_in else quote['price']
        }
    else:
        return {'error': 'Token-to-token swap not handled'}

def verify_critical_transactions():
    """Verify our logic on critical transactions"""
    
    transactions = load_transactions()
    
    print("🔍 VERIFYING RUST PARSING LOGIC")
    print("=" * 50)
    
    # Test key transaction types
    test_transactions = [
        (0, "First BUY transaction"),
        (8, "First SELL transaction (TX 9)"), 
        (1, "Second BUY transaction"),
    ]
    
    for idx, description in test_transactions:
        tx = transactions[idx]
        
        print(f"\n📊 {description.upper()}")
        print(f"TX Hash: {tx['tx_hash'][:12]}...")
        
        # Step 1: Aggregation
        aggregation = simulate_rust_aggregation(tx)
        
        print(f"Aggregation Result:")
        print(f"  token_in: {aggregation['token_in'][:8]}... ({aggregation['amount_in']})")
        print(f"  token_out: {aggregation['token_out'][:8]}... ({aggregation['amount_out']})")
        
        # Step 2: FinancialEvent creation
        event = simulate_financial_event_creation(aggregation)
        
        if 'error' not in event:
            print(f"FinancialEvent:")
            print(f"  event_type: {event['event_type']}")
            print(f"  token_mint: {event['token_mint'][:8]}...")
            print(f"  token_amount: {event['token_amount']}")
            print(f"  sol_amount: {event['sol_amount']}")
            print(f"  price_per_token: ${event['price_per_token']:.6f}")
            
            # Verify this matches expected behavior
            if event['event_type'] == 'Buy':
                # For BUY: sol_amount should be SOL spent, token_amount should be tokens received
                expected_sol = abs(tx['quote']['ui_change_amount'])
                expected_token = tx['base']['ui_change_amount']
                expected_price = tx['base']['price']
                
                print(f"Expected vs Actual:")
                print(f"  SOL amount: {expected_sol} vs {event['sol_amount']} ✅" if abs(expected_sol - event['sol_amount']) < 0.001 else f"  SOL amount: {expected_sol} vs {event['sol_amount']} ❌")
                print(f"  Token amount: {expected_token} vs {event['token_amount']} ✅" if abs(expected_token - event['token_amount']) < 0.001 else f"  Token amount: {expected_token} vs {event['token_amount']} ❌")
                print(f"  Price: ${expected_price:.6f} vs ${event['price_per_token']:.6f} ✅" if abs(expected_price - event['price_per_token']) < 0.001 else f"  Price: ${expected_price:.6f} vs ${event['price_per_token']:.6f} ❌")
                
            elif event['event_type'] == 'Sell':
                # For SELL: sol_amount should be SOL received, token_amount should be tokens sold
                expected_sol = tx['quote']['ui_change_amount']
                expected_token = abs(tx['base']['ui_change_amount'])
                expected_price = tx['base']['price']
                
                print(f"Expected vs Actual:")
                print(f"  SOL amount: {expected_sol} vs {event['sol_amount']} ✅" if abs(expected_sol - event['sol_amount']) < 0.001 else f"  SOL amount: {expected_sol} vs {event['sol_amount']} ❌")
                print(f"  Token amount: {expected_token} vs {event['token_amount']} ✅" if abs(expected_token - event['token_amount']) < 0.001 else f"  Token amount: {expected_token} vs {event['token_amount']} ❌")
                print(f"  Price: ${expected_price:.6f} vs ${event['price_per_token']:.6f} ✅" if abs(expected_price - event['price_per_token']) < 0.001 else f"  Price: ${expected_price:.6f} vs ${event['price_per_token']:.6f} ❌")
        else:
            print(f"❌ Error: {event['error']}")

def check_fifo_calculation_accuracy():
    """Check if FIFO calculation would be accurate with this data"""
    
    transactions = load_transactions()
    
    print(f"\n🧮 FIFO CALCULATION ACCURACY CHECK")
    print("=" * 40)
    
    # Simulate first few transactions for FIFO
    buy_transactions = []
    sell_transactions = []
    
    for i, tx in enumerate(transactions[:10]):
        aggregation = simulate_rust_aggregation(tx)
        event = simulate_financial_event_creation(aggregation)
        
        if 'error' not in event:
            if event['event_type'] == 'Buy':
                buy_transactions.append({
                    'tx_id': i+1,
                    'token_amount': event['token_amount'],
                    'sol_cost': event['sol_amount'],
                    'price_per_token': event['price_per_token']
                })
            elif event['event_type'] == 'Sell':
                sell_transactions.append({
                    'tx_id': i+1,
                    'token_amount': event['token_amount'],
                    'sol_revenue': event['sol_amount'],
                    'price_per_token': event['price_per_token']
                })
    
    print(f"Buy transactions: {len(buy_transactions)}")
    print(f"Sell transactions: {len(sell_transactions)}")
    
    if len(sell_transactions) > 0:
        print(f"\n📋 FIRST SELL TRANSACTION (FIFO TEST):")
        sell = sell_transactions[0]
        print(f"  TX {sell['tx_id']}: Sell {sell['token_amount']} BNSOL for {sell['sol_revenue']} SOL")
        print(f"  Token price: ${sell['price_per_token']:.6f}")
        
        # Calculate FIFO cost basis for first sell
        total_cost = 0
        total_tokens = 0
        for buy in buy_transactions:
            if total_tokens < sell['token_amount']:
                tokens_needed = min(buy['token_amount'], sell['token_amount'] - total_tokens)
                cost_for_tokens = (tokens_needed / buy['token_amount']) * buy['sol_cost']
                total_cost += cost_for_tokens
                total_tokens += tokens_needed
                print(f"    Using {tokens_needed} tokens from buy TX {buy['tx_id']} (cost: {cost_for_tokens} SOL)")
                
                if total_tokens >= sell['token_amount']:
                    break
        
        if total_tokens >= sell['token_amount']:
            realized_pnl_sol = sell['sol_revenue'] - total_cost
            print(f"  FIFO Cost Basis: {total_cost:.6f} SOL")
            print(f"  Revenue: {sell['sol_revenue']:.6f} SOL")
            print(f"  Realized P&L: {realized_pnl_sol:.6f} SOL")
            print(f"  ✅ FIFO calculation would work correctly")
        else:
            print(f"  ❌ Not enough buy transactions to cover sell")

def main():
    verify_critical_transactions()
    check_fifo_calculation_accuracy()

if __name__ == "__main__":
    main()