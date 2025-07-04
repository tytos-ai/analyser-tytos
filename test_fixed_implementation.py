#!/usr/bin/env python3
"""
Test the fixed Rust implementation to see if event over-generation is resolved
"""

import json
import requests
import time

def test_batch_job():
    """Test batch job with the same wallet to check event counts"""
    
    # Configuration for batch job
    payload = {
        "wallet_addresses": ["MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"],
        "filters": {
            "max_transactions_to_fetch": 100,
            "timeframe_mode": "none"
        }
    }
    
    print("🧪 TESTING FIXED RUST IMPLEMENTATION")
    print("=" * 60)
    print(f"Submitting batch job with 100 transactions limit...")
    
    # Submit batch job using the correct endpoint
    response = requests.post(
        "http://localhost:8081/api/services/control",
        json={
            "action": "start",
            "service": "pnl_analysis",
            "config": {
                "mode": "batch",
                "wallet_addresses": ["MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"],
                "filters": {
                    "max_transactions_to_fetch": 100,
                    "timeframe_mode": "none"
                }
            }
        }
    )
    
    if response.status_code != 200:
        print(f"❌ Failed to submit batch job: {response.status_code}")
        print(f"Response: {response.text}")
        return
    
    result = response.json()
    job_id = result.get("run_id") or result.get("job_id")
    
    if not job_id:
        print(f"❌ No job ID returned: {result}")
        return
    
    print(f"✅ Job submitted with ID: {job_id}")
    print(f"Waiting for completion...")
    
    # Poll for completion
    for attempt in range(30):  # 30 attempts = 30 seconds
        time.sleep(1)
        
        status_response = requests.get(f"http://localhost:8081/api/services/status")
        if status_response.status_code == 200:
            status = status_response.json()
            print(f"Attempt {attempt+1}: Checking status...")
            
            # Try to get results
            results_response = requests.get(f"http://localhost:8081/api/batch/results/{job_id}")
            if results_response.status_code == 200:
                results = results_response.json()
                
                print(f"\n📊 RESULTS:")
                print(f"Job Status: Completed")
                
                # Extract key metrics
                wallet_results = results.get("results", {})
                if wallet_results:
                    for wallet, wallet_data in wallet_results.items():
                        if isinstance(wallet_data, dict):
                            summary = wallet_data.get("summary", {})
                            
                            print(f"\nWallet: {wallet}")
                            print(f"  Total Trades: {summary.get('total_trades', 'N/A')}")
                            print(f"  Events Processed: Look in logs for debug info")
                            print(f"  Realized P&L: ${summary.get('realized_pnl_usd', 0):.2f}")
                            print(f"  Unrealized P&L: ${summary.get('unrealized_pnl_usd', 0):.2f}")
                            print(f"  Total P&L: ${summary.get('total_pnl_usd', 0):.2f}")
                            
                            current_holdings = wallet_data.get("current_holdings", [])
                            print(f"  Current Holdings: {len(current_holdings)} positions")
                
                print(f"\n🔍 Check server logs for debug information about:")
                print(f"  - Number of transactions fetched vs events generated")
                print(f"  - Deduplication statistics")
                print(f"  - USD value calculations")
                
                return results
        
        if attempt == 29:
            print(f"⏰ Timeout waiting for job completion")
            return None
    
    print(f"❌ Job did not complete in time")
    return None

def main():
    test_batch_job()

if __name__ == "__main__":
    main()