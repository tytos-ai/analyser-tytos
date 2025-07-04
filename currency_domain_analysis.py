#!/usr/bin/env python3
"""
Mathematical Analysis of Currency Domain Options
"""

from decimal import Decimal, getcontext
getcontext().prec = 28

def analyze_currency_options():
    """Analyze the mathematical implications of different currency domain approaches"""
    
    print("🔬 MATHEMATICAL ANALYSIS OF CURRENCY DOMAIN OPTIONS")
    print("=" * 80)
    
    # Sample data from BirdEye analysis
    transactions = [
        # SOL → Token swaps
        {
            "type": "sol_to_token",
            "sol_amount": Decimal("2.418328542"),
            "sol_price": Decimal("151.88"),
            "token_amount": Decimal("402.677159"),
            "token_price": Decimal("0.912153"),
            "token_symbol": "WIF"
        },
        {
            "type": "token_to_sol", 
            "sol_amount": Decimal("2.418682817"),
            "sol_price": Decimal("151.88"),
            "token_amount": Decimal("2006.730547"),
            "token_price": Decimal("0.183062"),
            "token_symbol": "MOODENG"
        },
        # Token-to-Token swap
        {
            "type": "token_to_token",
            "token_in_amount": Decimal("3698.576978"),
            "token_in_price": Decimal("0.999940"),
            "token_in_symbol": "USDC",
            "token_out_amount": Decimal("3143.110301"),
            "token_out_price": Decimal("1.176655"),
            "token_out_symbol": "EURC"
        }
    ]
    
    print("📊 SAMPLE TRANSACTIONS:")
    for i, tx in enumerate(transactions):
        print(f"\nTransaction {i+1}: {tx.get('type', 'unknown')}")
        if tx['type'] in ['sol_to_token', 'token_to_sol']:
            sol_usd = tx['sol_amount'] * tx['sol_price']
            token_usd = tx['token_amount'] * tx['token_price']
            print(f"  SOL: {tx['sol_amount']} @ ${tx['sol_price']} = ${sol_usd}")
            print(f"  {tx['token_symbol']}: {tx['token_amount']} @ ${tx['token_price']} = ${token_usd}")
        else:
            in_usd = tx['token_in_amount'] * tx['token_in_price']
            out_usd = tx['token_out_amount'] * tx['token_out_price']
            print(f"  {tx['token_in_symbol']}: {tx['token_in_amount']} @ ${tx['token_in_price']} = ${in_usd}")
            print(f"  {tx['token_out_symbol']}: {tx['token_out_amount']} @ ${tx['token_out_price']} = ${out_usd}")
    
    print("\n" + "=" * 80)
    print("OPTION 1: DUAL CURRENCY DOMAINS (Current Approach)")
    print("=" * 80)
    analyze_dual_currency_approach(transactions)
    
    print("\n" + "=" * 80)
    print("OPTION 2: USD-ONLY DOMAIN (Proposed Alternative)")
    print("=" * 80)
    analyze_usd_only_approach(transactions)
    
    print("\n" + "=" * 80)
    print("OPTION 3: SOL-ONLY DOMAIN (Alternative)")
    print("=" * 80)
    analyze_sol_only_approach(transactions)
    
    print("\n" + "=" * 80)
    print("MATHEMATICAL CORRECTNESS COMPARISON")
    print("=" * 80)
    compare_approaches(transactions)

def analyze_dual_currency_approach(transactions):
    """Analyze the current dual currency approach"""
    
    print("🔄 Current Implementation:")
    print("  - SOL swaps: Use sol_amount (SOL domain)")
    print("  - Token swaps: Use usd_value (USD domain)")
    print("  - Aggregation: Convert USD to SOL for totals")
    
    print("\n📝 FIFO Calculations:")
    
    sol_positions = []  # Store positions in SOL
    usd_positions = []  # Store positions in USD
    
    for tx in transactions:
        if tx['type'] == 'sol_to_token':
            # Store in SOL domain
            cost_per_token = tx['sol_amount'] / tx['token_amount']
            sol_positions.append({
                'symbol': tx['token_symbol'],
                'quantity': tx['token_amount'],
                'cost_per_token_sol': cost_per_token,
                'total_cost_sol': tx['sol_amount']
            })
            print(f"  BUY {tx['token_symbol']}: {tx['token_amount']} @ {cost_per_token:.8f} SOL/token")
            
        elif tx['type'] == 'token_to_sol':
            # Revenue in SOL domain
            revenue_per_token = tx['sol_amount'] / tx['token_amount']
            print(f"  SELL {tx['token_symbol']}: {tx['token_amount']} @ {revenue_per_token:.8f} SOL/token")
            
        else:  # token_to_token
            # Store in USD domain
            in_usd = tx['token_in_amount'] * tx['token_in_price']
            out_usd = tx['token_out_amount'] * tx['token_out_price']
            
            # SELL event (token spent)
            usd_positions.append({
                'symbol': tx['token_in_symbol'],
                'quantity': -tx['token_in_amount'],  # Negative for sell
                'cost_per_token_usd': tx['token_in_price'],
                'total_cost_usd': -in_usd
            })
            
            # BUY event (token received)
            cost_per_token = out_usd / tx['token_out_amount']
            usd_positions.append({
                'symbol': tx['token_out_symbol'],
                'quantity': tx['token_out_amount'],
                'cost_per_token_usd': cost_per_token,
                'total_cost_usd': out_usd
            })
            
            print(f"  SELL {tx['token_in_symbol']}: {tx['token_in_amount']} @ ${tx['token_in_price']:.6f}/token")
            print(f"  BUY {tx['token_out_symbol']}: {tx['token_out_amount']} @ ${cost_per_token:.6f}/token")
    
    print("\n⚠️  MATHEMATICAL ISSUES:")
    print("  1. ❌ Two separate FIFO engines (SOL and USD)")
    print("  2. ❌ Cannot directly compare SOL and USD P&L")
    print("  3. ❌ Aggregation requires currency conversion (error source)")
    print("  4. ❌ Total capital deployment mixes real SOL + converted SOL")
    
    # Demonstrate the mixing problem
    sol_capital = sum(pos['total_cost_sol'] for pos in sol_positions)
    usd_capital = sum(pos['total_cost_usd'] for pos in usd_positions if pos['total_cost_usd'] > 0)
    
    print(f"\n📊 Capital Deployment:")
    print(f"  SOL domain: {sol_capital} SOL")
    print(f"  USD domain: ${usd_capital}")
    print(f"  ❌ PROBLEM: Cannot add {sol_capital} SOL + ${usd_capital} USD directly!")

def analyze_usd_only_approach(transactions):
    """Analyze USD-only approach"""
    
    print("💵 USD-Only Implementation:")
    print("  - All transactions: Convert to USD at transaction time")
    print("  - FIFO: Single USD-based engine")
    print("  - Aggregation: Pure USD arithmetic")
    
    print("\n📝 FIFO Calculations:")
    
    usd_positions = []
    
    for tx in transactions:
        if tx['type'] == 'sol_to_token':
            # Convert SOL to USD at transaction time
            sol_usd_value = tx['sol_amount'] * tx['sol_price']
            cost_per_token_usd = sol_usd_value / tx['token_amount']
            
            usd_positions.append({
                'symbol': tx['token_symbol'],
                'quantity': tx['token_amount'],
                'cost_per_token_usd': cost_per_token_usd,
                'total_cost_usd': sol_usd_value
            })
            print(f"  BUY {tx['token_symbol']}: {tx['token_amount']} @ ${cost_per_token_usd:.6f}/token")
            
        elif tx['type'] == 'token_to_sol':
            # Convert SOL revenue to USD at transaction time
            sol_usd_value = tx['sol_amount'] * tx['sol_price']
            revenue_per_token_usd = sol_usd_value / tx['token_amount']
            print(f"  SELL {tx['token_symbol']}: {tx['token_amount']} @ ${revenue_per_token_usd:.6f}/token")
            
        else:  # token_to_token
            in_usd = tx['token_in_amount'] * tx['token_in_price']
            out_usd = tx['token_out_amount'] * tx['token_out_price']
            
            cost_per_token = out_usd / tx['token_out_amount']
            usd_positions.append({
                'symbol': tx['token_out_symbol'],
                'quantity': tx['token_out_amount'],
                'cost_per_token_usd': cost_per_token,
                'total_cost_usd': out_usd
            })
            
            print(f"  SELL {tx['token_in_symbol']}: {tx['token_in_amount']} @ ${tx['token_in_price']:.6f}/token")
            print(f"  BUY {tx['token_out_symbol']}: {tx['token_out_amount']} @ ${cost_per_token:.6f}/token")
    
    print("\n✅ MATHEMATICAL BENEFITS:")
    print("  1. ✅ Single FIFO engine (USD domain)")
    print("  2. ✅ Direct P&L comparison across all tokens")
    print("  3. ✅ Clean aggregation (pure USD arithmetic)")
    print("  4. ✅ Total capital deployment in consistent units")
    
    total_capital_usd = sum(pos['total_cost_usd'] for pos in usd_positions)
    print(f"\n📊 Capital Deployment:")
    print(f"  Total: ${total_capital_usd:.2f} USD")
    print(f"  ✅ CLEAN: All values in same currency domain")
    
    print("\n⚠️  POTENTIAL ISSUES:")
    print("  1. ❓ SOL price accuracy at transaction time")
    print("  2. ❓ Loss of 'native' SOL amounts for SOL transactions")
    print("  3. ❓ Temporal consistency of SOL prices")

def analyze_sol_only_approach(transactions):
    """Analyze SOL-only approach"""
    
    print("🟡 SOL-Only Implementation:")
    print("  - All transactions: Convert to SOL at transaction time")
    print("  - FIFO: Single SOL-based engine")
    print("  - Aggregation: Pure SOL arithmetic")
    
    print("\n📝 FIFO Calculations:")
    
    # Use a consistent SOL price for this analysis
    sol_price = Decimal("151.88")  # From transaction data
    
    sol_positions = []
    
    for tx in transactions:
        if tx['type'] == 'sol_to_token':
            # Use actual SOL amount
            cost_per_token_sol = tx['sol_amount'] / tx['token_amount']
            
            sol_positions.append({
                'symbol': tx['token_symbol'],
                'quantity': tx['token_amount'],
                'cost_per_token_sol': cost_per_token_sol,
                'total_cost_sol': tx['sol_amount']
            })
            print(f"  BUY {tx['token_symbol']}: {tx['token_amount']} @ {cost_per_token_sol:.8f} SOL/token")
            
        elif tx['type'] == 'token_to_sol':
            # Use actual SOL amount
            revenue_per_token_sol = tx['sol_amount'] / tx['token_amount']
            print(f"  SELL {tx['token_symbol']}: {tx['token_amount']} @ {revenue_per_token_sol:.8f} SOL/token")
            
        else:  # token_to_token
            # Convert USD values to SOL
            in_usd = tx['token_in_amount'] * tx['token_in_price']
            out_usd = tx['token_out_amount'] * tx['token_out_price']
            
            in_sol = in_usd / sol_price
            out_sol = out_usd / sol_price
            
            cost_per_token_sol = out_sol / tx['token_out_amount']
            sol_positions.append({
                'symbol': tx['token_out_symbol'],
                'quantity': tx['token_out_amount'],
                'cost_per_token_sol': cost_per_token_sol,
                'total_cost_sol': out_sol
            })
            
            print(f"  SELL {tx['token_in_symbol']}: {tx['token_in_amount']} @ {in_sol/tx['token_in_amount']:.8f} SOL/token")
            print(f"  BUY {tx['token_out_symbol']}: {tx['token_out_amount']} @ {cost_per_token_sol:.8f} SOL/token")
    
    print("\n✅ MATHEMATICAL BENEFITS:")
    print("  1. ✅ Single FIFO engine (SOL domain)")
    print("  2. ✅ Direct P&L comparison across all tokens")
    print("  3. ✅ Clean aggregation (pure SOL arithmetic)")
    print("  4. ✅ Native SOL amounts preserved for SOL transactions")
    
    total_capital_sol = sum(pos['total_cost_sol'] for pos in sol_positions)
    print(f"\n📊 Capital Deployment:")
    print(f"  Total: {total_capital_sol:.6f} SOL")
    print(f"  ✅ CLEAN: All values in same currency domain")
    
    print("\n⚠️  POTENTIAL ISSUES:")
    print("  1. ❓ SOL price accuracy for token-to-token conversions")
    print("  2. ❓ Which SOL price to use for token-to-token swaps?")
    print("  3. ❓ Temporal consistency across different transaction times")

def compare_approaches(transactions):
    """Compare all approaches mathematically"""
    
    print("🔍 MATHEMATICAL CORRECTNESS ANALYSIS:")
    print()
    
    criteria = [
        "Currency Domain Consistency",
        "FIFO Engine Simplicity", 
        "Aggregation Accuracy",
        "Data Preservation",
        "Implementation Complexity",
        "Error Propagation Risk"
    ]
    
    scores = {
        "Dual Currency": [2, 1, 1, 3, 1, 3],  # Current approach
        "USD-Only": [3, 3, 3, 2, 3, 2],      # Proposed
        "SOL-Only": [3, 3, 3, 3, 2, 2]       # Alternative
    }
    
    print(f"{'Criterion':<25} {'Dual':<6} {'USD':<6} {'SOL':<6}")
    print("-" * 50)
    
    for i, criterion in enumerate(criteria):
        print(f"{criterion:<25} {scores['Dual Currency'][i]:<6} {scores['USD-Only'][i]:<6} {scores['SOL-Only'][i]:<6}")
    
    print("-" * 50)
    totals = {approach: sum(scores[approach]) for approach in scores}
    for approach, total in totals.items():
        print(f"{approach:<25} {total:<6}")
    
    print("\n🎯 RECOMMENDATION ANALYSIS:")
    print()
    
    print("❌ DUAL CURRENCY APPROACH:")
    print("  - Mathematically inconsistent aggregation")
    print("  - Complex dual FIFO engines")
    print("  - Currency mixing errors")
    print("  - Current implementation has this problem")
    
    print("\n✅ USD-ONLY APPROACH:")
    print("  - BirdEye provides USD prices for ALL tokens")
    print("  - Single, consistent currency domain")
    print("  - Clean mathematical operations")
    print("  - Leverages existing price data")
    print("  - ⚠️  Converts SOL amounts to USD (may lose 'nativeness')")
    
    print("\n✅ SOL-ONLY APPROACH:")
    print("  - Native SOL amounts preserved")
    print("  - Single, consistent currency domain")
    print("  - Clean mathematical operations")
    print("  - ⚠️  Requires SOL price for token-to-token conversions")
    print("  - ⚠️  Which SOL price to use? (temporal consistency issue)")

if __name__ == "__main__":
    analyze_currency_options()