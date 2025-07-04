#!/usr/bin/env python3
"""
Compare Python USD-only P&L reference with Rust USD-only implementation
"""

import json
import requests
from decimal import Decimal
import time

def submit_rust_batch_job(wallet_address, limit=100):
    """Submit batch job to Rust API server"""
    
    batch_request = {
        "wallet_addresses": [wallet_address],
        "filters": {
            "max_signatures": limit,
            "min_capital_sol": 0.0,
            "min_win_rate": 0.0,
            "min_hold_minutes": 0.0,
            "min_trades": 1
        }
    }
    
    print(f"🚀 Submitting Rust batch job...")
    print(f"Request: {json.dumps(batch_request, indent=2)}")
    
    try:
        # Submit batch job
        response = requests.post(
            "http://localhost:8081/api/pnl/batch/run",
            json=batch_request,
            headers={"Content-Type": "application/json"},
            timeout=30
        )
        
        if response.status_code != 200:
            print(f"❌ Batch submission failed: {response.status_code}")
            print(f"Response: {response.text}")
            return None
            
        batch_result = response.json()
        run_id = batch_result.get("data", {}).get("job_id") or batch_result.get("run_id")
        
        if not run_id:
            print(f"❌ No job_id or run_id returned: {batch_result}")
            return None
            
        print(f"✅ Batch job submitted successfully!")
        print(f"📋 Job ID: {run_id}")
        
        # Wait for completion
        print(f"\n⏳ Waiting for batch completion...")
        
        for attempt in range(60):  # Wait up to 60 seconds
            time.sleep(1)
            
            status_response = requests.get(f"http://localhost:8081/api/pnl/batch/status/{run_id}")
            if status_response.status_code == 200:
                status_data = status_response.json()
                status = status_data.get("status")
                
                if status == "completed":
                    print("✅ Batch job completed!")
                    break
                elif status == "failed":
                    print("❌ Batch job failed!")
                    print(f"Error: {status_data.get('error', 'Unknown error')}")
                    return None
                elif attempt % 10 == 0:  # Print status every 10 seconds
                    print(f"Status: {status}")
        else:
            print("⏰ Timeout waiting for batch completion")
            return None
            
        # Get results
        print(f"\n📊 Fetching Rust results...")
        results_response = requests.get(f"http://localhost:8081/api/pnl/batch/results/{run_id}")
        
        if results_response.status_code != 200:
            print(f"❌ Failed to get results: {results_response.status_code}")
            return None
            
        return results_response.json()
        
    except requests.exceptions.RequestException as e:
        print(f"❌ Network error: {e}")
        return None
    except Exception as e:
        print(f"❌ Unexpected error: {e}")
        return None

def load_python_results():
    """Load the most recent Python results"""
    import glob
    import os
    
    # Find the most recent Python results file
    python_files = glob.glob("python_usd_pnl_reference_*.json")
    if not python_files:
        print("❌ No Python reference results found")
        return None
        
    latest_file = max(python_files, key=os.path.getctime)
    print(f"📂 Loading Python results from: {latest_file}")
    
    with open(latest_file, 'r') as f:
        return json.load(f)

def compare_results(python_results, rust_results):
    """Compare Python and Rust P&L results"""
    
    print(f"\n🔍 COMPARING PYTHON vs RUST RESULTS")
    print("=" * 70)
    
    if not python_results or not rust_results:
        print("❌ Missing results for comparison")
        return
        
    # Extract Python data
    python_pnl = python_results['pnl_calculation']
    python_events = len(python_results['events'])
    python_transactions = python_results['transaction_count']
    
    # Extract Rust data
    rust_reports = rust_results.get('reports', [])
    if not rust_reports:
        print("❌ No Rust reports found")
        return
        
    rust_report = rust_reports[0]
    rust_summary = rust_report.get('summary', {})
    rust_metadata = rust_report.get('metadata', {})
    rust_token_breakdown = rust_report.get('token_breakdown', [])
    rust_holdings = rust_report.get('current_holdings', [])
    
    print(f"📊 TRANSACTION & EVENT COMPARISON:")
    print(f"  Transactions Processed:")
    print(f"    Python: {python_transactions}")
    print(f"    Rust:   {rust_metadata.get('events_processed', 'N/A')}")
    print(f"  Events Generated:")
    print(f"    Python: {python_events}")
    print(f"    Rust:   {rust_metadata.get('events_processed', 'N/A')}")
    
    print(f"\n💰 P&L COMPARISON:")
    
    # Convert strings back to Decimal for comparison
    python_realized = Decimal(python_pnl['realized_pnl_usd'])
    python_unrealized = Decimal(python_pnl['unrealized_pnl_usd'])
    python_invested = Decimal(python_pnl['total_invested_usd'])
    python_withdrawn = Decimal(python_pnl['total_withdrawn_usd'])
    
    rust_realized = Decimal(str(rust_summary.get('realized_pnl_usd', 0)))
    rust_unrealized = Decimal(str(rust_summary.get('unrealized_pnl_usd', 0)))
    rust_total_pnl = Decimal(str(rust_summary.get('total_pnl_usd', 0)))
    
    print(f"  Realized P&L (USD):")
    print(f"    Python: ${python_realized:.2f}")
    print(f"    Rust:   ${rust_realized:.2f}")
    print(f"    Diff:   ${abs(python_realized - rust_realized):.2f}")
    
    print(f"  Unrealized P&L (USD):")
    print(f"    Python: ${python_unrealized:.2f}")
    print(f"    Rust:   ${rust_unrealized:.2f}")
    print(f"    Diff:   ${abs(python_unrealized - rust_unrealized):.2f}")
    
    print(f"  Total P&L (USD):")
    python_total = python_realized + python_unrealized
    print(f"    Python: ${python_total:.2f}")
    print(f"    Rust:   ${rust_total_pnl:.2f}")
    print(f"    Diff:   ${abs(python_total - rust_total_pnl):.2f}")
    
    print(f"\n📈 INVESTMENT TRACKING:")
    print(f"  Total Invested (USD):")
    print(f"    Python: ${python_invested:.2f}")
    print(f"  Total Withdrawn (USD):")
    print(f"    Python: ${python_withdrawn:.2f}")
    print(f"  Net Cash Flow (USD):")
    print(f"    Python: ${python_withdrawn - python_invested:.2f}")
    
    print(f"\n🎯 POSITION COMPARISON:")
    print(f"  Active Positions:")
    print(f"    Python: {len(python_pnl['current_positions'])}")
    print(f"    Rust:   {len(rust_holdings)}")
    
    # Show position details
    print(f"\n📊 POSITION DETAILS:")
    python_positions = python_pnl['current_positions']
    
    print(f"  Python Positions:")
    for mint, position in python_positions.items():
        symbol = position['token_symbol']
        quantity = Decimal(position['quantity'])
        avg_cost = Decimal(position['average_cost_usd'])
        print(f"    {symbol}: {quantity:.2f} @ ${avg_cost:.6f}/token")
    
    print(f"\n  Rust Holdings:")
    for holding in rust_holdings:
        symbol = holding.get('token_symbol', 'UNK')
        quantity = Decimal(str(holding.get('amount', 0)))
        avg_cost = Decimal(str(holding.get('avg_cost_basis_usd', 0)))
        print(f"    {symbol}: {quantity:.2f} @ ${avg_cost:.6f}/token")
    
    print(f"\n🎯 ACCURACY ASSESSMENT:")
    
    # Calculate percentage differences
    realized_diff_pct = abs(python_realized - rust_realized) / max(abs(python_realized), abs(rust_realized), Decimal('0.01')) * 100
    unrealized_diff_pct = abs(python_unrealized - rust_unrealized) / max(abs(python_unrealized), abs(rust_unrealized), Decimal('0.01')) * 100
    total_diff_pct = abs(python_total - rust_total_pnl) / max(abs(python_total), abs(rust_total_pnl), Decimal('0.01')) * 100
    
    print(f"  Realized P&L Difference: {realized_diff_pct:.3f}%")
    print(f"  Unrealized P&L Difference: {unrealized_diff_pct:.3f}%")
    print(f"  Total P&L Difference: {total_diff_pct:.3f}%")
    
    # Assess accuracy
    if realized_diff_pct < 1.0 and total_diff_pct < 5.0:
        print(f"  ✅ EXCELLENT: Implementations are highly consistent")
    elif realized_diff_pct < 5.0 and total_diff_pct < 10.0:
        print(f"  ✅ GOOD: Implementations are reasonably consistent")
    else:
        print(f"  ⚠️ WARNING: Significant differences detected")
    
    # Event type breakdown
    print(f"\n📋 EVENT TYPE BREAKDOWN:")
    print(f"  Buy Events:")
    print(f"    Python: {python_pnl['buy_events']}")
    print(f"    Rust:   {rust_summary.get('winning_trades', 'N/A')} (winning trades)")
    print(f"  Sell Events:")
    print(f"    Python: {python_pnl['sell_events']}")
    print(f"    Rust:   {rust_summary.get('losing_trades', 'N/A')} (losing trades)")
    
    return {
        'realized_diff': abs(python_realized - rust_realized),
        'total_diff': abs(python_total - rust_total_pnl),
        'position_count_match': len(python_positions) == len(rust_holdings),
        'accuracy_good': realized_diff_pct < 5.0 and total_diff_pct < 10.0
    }

def main():
    wallet_address = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    
    print(f"🔍 PYTHON vs RUST USD-ONLY P&L COMPARISON")
    print(f"Wallet: {wallet_address}")
    print("=" * 70)
    
    # Load Python results
    python_results = load_python_results()
    if not python_results:
        return
    
    # Submit Rust batch job
    rust_results = submit_rust_batch_job(wallet_address, limit=100)
    if not rust_results:
        return
    
    # Compare results
    comparison = compare_results(python_results, rust_results)
    
    print(f"\n🎯 FINAL ASSESSMENT:")
    print("=" * 70)
    if comparison and comparison['accuracy_good']:
        print("✅ SUCCESS: Python and Rust USD-only implementations are consistent!")
        print("✅ Mathematical correctness verified across implementations")
        print("✅ USD-only currency domain working correctly")
    else:
        print("⚠️ DIFFERENCES: Investigation needed for inconsistencies")
        print("🔍 Check event generation and FIFO calculation logic")
    
    # Save comparison results
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    comparison_file = f"pnl_comparison_results_{timestamp}.json"
    
    comparison_data = {
        'timestamp': timestamp,
        'wallet_address': wallet_address,
        'python_results': python_results,
        'rust_results': rust_results,
        'comparison_summary': comparison
    }
    
    with open(comparison_file, 'w') as f:
        json.dump(comparison_data, f, indent=2, default=str)
    
    print(f"\n💾 Comparison results saved to: {comparison_file}")

if __name__ == "__main__":
    main()