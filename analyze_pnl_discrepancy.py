#!/usr/bin/env python3
"""
Analyze the significant discrepancy between Python and Rust P&L results
"""

import json
from decimal import Decimal

def analyze_discrepancy():
    """Analyze the P&L calculation discrepancy"""
    
    print("🔍 ANALYZING P&L CALCULATION DISCREPANCY")
    print("=" * 70)
    
    # Load Python results
    with open('python_usd_pnl_reference_20250704_083125.json', 'r') as f:
        python_data = json.load(f)
    
    # Load Rust results
    with open('rust_pnl_results.json', 'r') as f:
        rust_data = json.load(f)
    
    # Extract key metrics
    python_pnl = python_data['pnl_calculation']
    rust_summary = rust_data['data']['results']['MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa']['pnl_report']['summary']
    
    print("📊 SUMMARY COMPARISON:")
    print("-" * 40)
    print(f"Python Results:")
    print(f"  Realized P&L: ${python_pnl['realized_pnl_usd']}")
    print(f"  Unrealized P&L: ${python_pnl['unrealized_pnl_usd']}")
    print(f"  Total Invested: ${python_pnl['total_invested_usd']}")
    print(f"  Total Withdrawn: ${python_pnl['total_withdrawn_usd']}")
    print(f"  Buy Events: {python_pnl['buy_events']}")
    print(f"  Sell Events: {python_pnl['sell_events']}")
    print(f"  Active Positions: {len(python_pnl['current_positions'])}")
    
    print(f"\nRust Results:")
    print(f"  Realized P&L: ${rust_summary['realized_pnl_usd']}")
    print(f"  Unrealized P&L: ${rust_summary['unrealized_pnl_usd']}")
    print(f"  Total P&L: ${rust_summary['total_pnl_usd']}")
    print(f"  Total Trades: {rust_summary['total_trades']}")
    print(f"  Winning Trades: {rust_summary['winning_trades']}")
    print(f"  Losing Trades: {rust_summary['losing_trades']}")
    print(f"  ROI: {rust_summary['roi_percentage']}%")
    
    print(f"\n🚨 CRITICAL DISCREPANCIES:")
    print("-" * 40)
    
    # Calculate differences
    python_realized = Decimal(python_pnl['realized_pnl_usd'])
    python_unrealized = Decimal(python_pnl['unrealized_pnl_usd'])
    python_total = python_realized + python_unrealized
    
    rust_realized = Decimal(rust_summary['realized_pnl_usd'])
    rust_unrealized = Decimal(rust_summary['unrealized_pnl_usd'])
    rust_total = Decimal(rust_summary['total_pnl_usd'])
    
    realized_diff = rust_realized - python_realized
    unrealized_diff = rust_unrealized - python_unrealized
    total_diff = rust_total - python_total
    
    print(f"Realized P&L Difference: ${realized_diff:+,.2f}")
    print(f"Unrealized P&L Difference: ${unrealized_diff:+,.2f}")
    print(f"Total P&L Difference: ${total_diff:+,.2f}")
    
    print(f"\n🔍 POTENTIAL CAUSES:")
    print("-" * 40)
    
    print("1. EVENT COUNT MISMATCH:")
    print(f"   Python Events: {len(python_data['events'])}")
    print(f"   Rust Events Processed: {rust_data['data']['results']['MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa']['pnl_report']['metadata']['events_processed']}")
    
    print(f"\n2. CURRENCY DOMAIN ISSUES:")
    print(f"   Python uses embedded USD prices directly")
    print(f"   Rust may still have currency conversion issues")
    
    print(f"\n3. UNREALIZED P&L CALCULATION:")
    print(f"   Python: ${python_unrealized:.2f} (set to 0 - no current prices)")
    print(f"   Rust: ${rust_unrealized:,.2f} (massive negative - price enhancement issue?)")
    
    print(f"\n4. FIFO LOGIC DIFFERENCES:")
    print(f"   Different cost basis calculations")
    print(f"   Different position tracking")
    
    print(f"\n🎯 INVESTIGATION PRIORITIES:")
    print("-" * 40)
    print("1. ✅ Event generation logic (verified as correct)")
    print("2. ⚠️ USD value calculation in events")
    print("3. ⚠️ FIFO cost basis tracking")  
    print("4. 🚨 Price enhancement step (causing massive unrealized loss)")
    print("5. ⚠️ Token position aggregation")
    
    # Analyze specific events
    print(f"\n📋 EVENT ANALYSIS:")
    print("-" * 40)
    
    print("Python Events (first 10):")
    for i, event in enumerate(python_data['events'][:10]):
        print(f"  {i+1}. {event['event_type']} {event['token_symbol']} - USD: ${event['usd_value']}")
    
    print(f"\n🎯 RECOMMENDED FIXES:")
    print("-" * 40)
    print("1. Verify USD value calculation in ProcessedSwap")
    print("2. Check price enhancement step - likely causing massive unrealized loss")
    print("3. Validate FIFO cost basis calculations")
    print("4. Ensure consistent event generation between implementations")
    
    # Check for obvious issues
    print(f"\n🚨 OBVIOUS ISSUES DETECTED:")
    print("-" * 40)
    
    if abs(rust_unrealized) > 1000000:  # > $1M unrealized loss
        print("❌ MASSIVE UNREALIZED LOSS: Price enhancement likely broken")
        print("   → Check enhance_report_with_current_prices function")
        print("   → Verify current price fetching logic")
    
    if rust_realized > python_realized * 1000:  # Realized P&L 1000x higher
        print("❌ INFLATED REALIZED P&L: Currency conversion issue")
        print("   → Check USD value calculations in events")
        print("   → Verify FIFO cost basis logic")
    
    event_count_diff = abs(len(python_data['events']) - rust_data['data']['results']['MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa']['pnl_report']['metadata']['events_processed'])
    if event_count_diff > 5:
        print("❌ EVENT COUNT MISMATCH: Different event generation")
        print("   → Check transaction parsing logic")
        print("   → Verify dual event generation")

if __name__ == "__main__":
    analyze_discrepancy()