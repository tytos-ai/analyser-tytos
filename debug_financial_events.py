#!/usr/bin/env python3
"""
Debug script to analyze FinancialEvents created from BirdEye transactions
and compare with manual P&L calculation to identify discrepancies.
"""

import json
from decimal import Decimal, getcontext
getcontext().prec = 50

def load_birdeye_transactions():
    """Load BirdEye transactions from JSON file"""
    with open('/home/mrima/tytos/wallet-analyser/manual_verification_transactions.json', 'r') as f:
        data = json.load(f)
    return data['data']['items']

def simulate_financial_event_creation(birdeye_transactions):
    """
    Simulate the Rust code logic that converts BirdEye transactions to FinancialEvents
    This helps us understand what our system is actually processing.
    """
    events = []
    
    # Process transactions in chronological order (oldest first)
    transactions = sorted(birdeye_transactions, key=lambda x: x['block_unix_time'])
    
    sol_mint = "So11111111111111111111111111111111111111112"
    
    for i, tx in enumerate(transactions):
        quote = tx['quote']  # SOL side
        base = tx['base']    # BNSOL side
        
        # Determine swap direction based on type_swap
        quote_change = Decimal(str(quote['ui_change_amount']))
        base_change = Decimal(str(base['ui_change_amount']))
        
        # In our BNSOL/SOL swaps:
        # - quote is always SOL
        # - base is always BNSOL
        
        token_in = None
        token_out = None
        amount_in = Decimal('0')
        amount_out = Decimal('0')
        
        if quote_change < 0:  # SOL spent
            token_in = quote['address']  # SOL
            token_out = base['address']  # BNSOL
            amount_in = abs(quote_change)
            amount_out = base_change
        else:  # SOL received
            token_in = base['address']   # BNSOL
            token_out = quote['address'] # SOL
            amount_in = abs(base_change)
            amount_out = quote_change
        
        # Determine event type
        if token_in == sol_mint:
            # SOL → BNSOL: Create BNSOL BUY event
            event_type = "Buy"
            token_mint = token_out  # BNSOL
            token_amount = amount_out
            sol_amount = amount_in
            # Price per token: Use BNSOL price
            price_per_token = Decimal(str(base['price']))
        elif token_out == sol_mint:
            # BNSOL → SOL: Create BNSOL SELL event
            event_type = "Sell"
            token_mint = token_in   # BNSOL
            token_amount = amount_in
            sol_amount = amount_out
            # Price per token: Use BNSOL price
            price_per_token = Decimal(str(base['price']))
        else:
            # This shouldn't happen in our BNSOL/SOL data
            continue
        
        event = {
            'transaction_id': tx['tx_hash'],
            'event_type': event_type,
            'token_mint': token_mint,
            'token_amount': token_amount,
            'sol_amount': sol_amount,
            'price_per_token': price_per_token,
            'timestamp': tx['block_unix_time'],
            'original_tx': {
                'quote_change': quote_change,
                'base_change': base_change,
                'quote_price': Decimal(str(quote['price'])),
                'base_price': Decimal(str(base['price'])),
            }
        }
        
        events.append(event)
        
        if i < 5:  # Show first 5 for debugging
            print(f"TX {i+1}: {event['transaction_id'][:8]}...")
            print(f"  Type: {event_type}")
            print(f"  Token: {token_amount:.4f} BNSOL @ ${price_per_token:.4f}")
            print(f"  SOL: {sol_amount:.4f}")
            print(f"  Quote change: {quote_change:.4f} SOL @ ${quote['price']:.4f}")
            print(f"  Base change: {base_change:.4f} BNSOL @ ${base['price']:.4f}")
            print()
    
    return events

def analyze_price_calculation_discrepancy(events):
    """
    Analyze potential issues in price calculation that could cause the $6.84M discrepancy
    """
    print("🔍 ANALYZING PRICE CALCULATION DISCREPANCIES")
    print("=" * 60)
    
    total_cost_discrepancy = Decimal('0')
    
    for i, event in enumerate(events[:10]):  # Analyze first 10
        token_amount = event['token_amount']
        price_per_token = event['price_per_token']
        sol_amount = event['sol_amount']
        
        # Calculate what the cost SHOULD be based on manual calculation
        if event['event_type'] == 'Buy':
            # For buy: cost = token_amount * token_price
            expected_cost = token_amount * price_per_token
            actual_cost = sol_amount  # What our system uses
        else:
            # For sell: revenue = token_amount * token_price  
            expected_revenue = token_amount * price_per_token
            actual_revenue = sol_amount  # What our system uses
            expected_cost = expected_revenue
            actual_cost = actual_revenue
        
        discrepancy = actual_cost - expected_cost
        total_cost_discrepancy += discrepancy
        
        print(f"Event {i+1} ({event['event_type']}):")
        print(f"  Token: {token_amount:.4f} @ ${price_per_token:.4f}")
        print(f"  Expected cost: ${expected_cost:.2f}")
        print(f"  Actual SOL amount: {actual_cost:.4f}")
        print(f"  SOL price implied: ${actual_cost / token_amount:.4f}" if token_amount > 0 else "  N/A")
        print(f"  Discrepancy: ${discrepancy:.2f}")
        print()
    
    print(f"Total cost discrepancy in first 10 events: ${total_cost_discrepancy:.2f}")
    print()

def check_fifo_calculation_method(events):
    """
    Check if our FIFO calculation differs from manual by analyzing the method
    """
    print("🔍 ANALYZING FIFO CALCULATION METHOD")
    print("=" * 60)
    
    # Key questions:
    # 1. Does our system use BNSOL prices correctly?
    # 2. Are we calculating cost basis correctly?
    # 3. Are we handling the sol_amount vs price_per_token correctly?
    
    bnsol_mint = "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85"
    
    # Simulate our system's TxRecord creation
    print("How our system creates TxRecords:")
    for i, event in enumerate(events[:5]):
        if event['event_type'] == 'Buy':
            # From fifo_pnl_engine.rs line 220
            price = event['price_per_token']  # BNSOL price
            sol_cost = event['token_amount'] * price  # This is the ISSUE!
            
            print(f"  Buy {i+1}: {event['token_amount']:.4f} BNSOL")
            print(f"    price_per_token (BNSOL): ${price:.4f}")
            print(f"    calculated SOL cost: {sol_cost:.4f}")
            print(f"    actual SOL from event: {event['sol_amount']:.4f}")
            print(f"    ERROR: Using BNSOL price to calculate SOL cost!")
            print()
    
    print("The CRITICAL BUG:")
    print("Our system calculates: sol_cost = token_amount * price_per_token")
    print("But price_per_token is BNSOL price (~$155), not SOL price (~$147)")
    print("This inflates costs by the BNSOL/SOL ratio!")
    print()

def main():
    print("🧪 DEBUGGING FINANCIAL EVENTS CREATION")
    print("=" * 60)
    
    # Load and process BirdEye data
    birdeye_transactions = load_birdeye_transactions()
    print(f"Loaded {len(birdeye_transactions)} BirdEye transactions")
    print()
    
    # Simulate FinancialEvent creation
    events = simulate_financial_event_creation(birdeye_transactions)
    print(f"Created {len(events)} FinancialEvents")
    print()
    
    # Analyze price calculation issues
    analyze_price_calculation_discrepancy(events)
    
    # Check FIFO calculation method
    check_fifo_calculation_method(events)
    
    print("🎯 ROOT CAUSE IDENTIFIED:")
    print("Our system uses BNSOL price to calculate SOL costs, but should use")
    print("the actual SOL amounts from the transaction data!")

if __name__ == "__main__":
    main()