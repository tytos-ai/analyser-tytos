#!/usr/bin/env python3
"""
Analyze the price enhancement issue by examining the batch job results
to understand why unrealized P&L is so inflated.
"""

import json
from decimal import Decimal, getcontext
getcontext().prec = 50

def analyze_batch_results():
    """Analyze the batch job results to understand the price enhancement issue"""
    
    # From the batch job results, I can see:
    current_bnsol_price = Decimal("157.10913934429700589134881733")
    
    print("🔍 ANALYZING PRICE ENHANCEMENT ISSUE")
    print("=" * 60)
    print(f"Current BNSOL price from Jupiter: ${current_bnsol_price:.2f}")
    
    # Let's analyze some of the holdings from the results
    holdings_analysis = [
        {
            "amount": Decimal("39900.127559093"),
            "cost_basis_usd": Decimal("41084.633269553524172782326810"),
            "current_value_usd": Decimal("6268674.7005367673042056119276"),
            "unrealized_pnl_usd": Decimal("6227590.0672672137800328296008")
        },
        {
            "amount": Decimal("29179.12410173"),
            "cost_basis_usd": Decimal("30068.384323939999999999999999"),
            "current_value_usd": Decimal("4584307.0744432337732277320775"),
            "unrealized_pnl_usd": Decimal("4554238.6901192937732277320775")
        }
    ]
    
    print("\n📊 HOLDINGS ANALYSIS:")
    print("=" * 60)
    
    for i, holding in enumerate(holdings_analysis, 1):
        amount = holding["amount"]
        cost_basis = holding["cost_basis_usd"]
        current_value = holding["current_value_usd"]
        unrealized_pnl = holding["unrealized_pnl_usd"]
        
        # Calculate the average cost per token
        avg_cost_per_token = cost_basis / amount
        
        # Calculate current value using Jupiter price
        calculated_current_value = amount * current_bnsol_price
        
        print(f"Holding {i}:")
        print(f"  Amount: {amount:.4f} BNSOL")
        print(f"  Cost basis: ${cost_basis:.2f}")
        print(f"  Avg cost per token: ${avg_cost_per_token:.4f}")
        print(f"  Current price: ${current_bnsol_price:.2f}")
        print(f"  Current value: ${current_value:.2f}")
        print(f"  Calculated value: ${calculated_current_value:.2f}")
        print(f"  Unrealized P&L: ${unrealized_pnl:.2f}")
        
        # The problem: cost basis is in SOL terms (~$1.03), not USD terms
        # But current value is in USD terms using $157 price
        print(f"  🚨 ISSUE: Cost basis ${avg_cost_per_token:.4f} vs current ${current_bnsol_price:.2f}")
        print(f"  🚨 This is a {current_bnsol_price / avg_cost_per_token:.0f}x difference!")
        print()

def identify_root_cause():
    """Identify the root cause of the price enhancement issue"""
    
    print("🎯 ROOT CAUSE ANALYSIS:")
    print("=" * 60)
    
    print("1. COST BASIS ISSUE:")
    print("   - Our FIFO engine calculates cost basis correctly in SOL terms")
    print("   - Cost basis shows ~$1.03 per BNSOL (this is SOL cost, not USD)")
    print("   - But we're treating this as USD cost basis")
    print()
    
    print("2. CURRENT PRICE ISSUE:")
    print("   - Jupiter API provides BNSOL price in USD: $157.11")
    print("   - This is the correct USD price for BNSOL")
    print("   - But we're comparing USD current price to SOL cost basis")
    print()
    
    print("3. THE MISMATCH:")
    print("   - Cost basis: ~$1.03 (actually SOL, treated as USD)")
    print("   - Current price: $157.11 (correctly USD)")
    print("   - Unrealized P&L = (157.11 - 1.03) × amount = massive gains")
    print()
    
    print("4. THE FIX NEEDED:")
    print("   - Either convert cost basis from SOL to USD properly")
    print("   - Or convert current price from USD to SOL terms")
    print("   - Or recalculate cost basis in USD terms from the start")

def main():
    analyze_batch_results()
    identify_root_cause()
    
    print("\n💡 SOLUTION:")
    print("The price enhancement step should convert SOL-based cost basis")
    print("to USD terms before comparing with USD current prices!")

if __name__ == "__main__":
    main()