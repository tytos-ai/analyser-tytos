#!/usr/bin/env python3
"""
Comprehensive BirdEye Transaction Analysis Script
Fetches transactions from multiple addresses with different offsets and analyzes data structure patterns.
"""

import requests
import json
import time
from datetime import datetime
from typing import Dict, List, Any, Optional
import os

class BirdEyeAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        self.results = []
        
    def fetch_transactions(self, address: str, offset: int = 0, limit: int = 50) -> Optional[Dict]:
        """Fetch transactions for a specific address with offset"""
        params = {
            "address": address,
            "offset": offset,
            "limit": limit
        }
        
        try:
            print(f"Fetching transactions for {address} (offset: {offset}, limit: {limit})")
            response = requests.get(self.base_url, headers=self.headers, params=params)
            response.raise_for_status()
            
            data = response.json()
            print(f"  -> Success: {len(data.get('data', {}).get('items', []))} transactions found")
            return data
            
        except requests.exceptions.RequestException as e:
            print(f"  -> Error fetching {address}: {e}")
            return None
            
    def analyze_transaction_structure(self, tx: Dict) -> Dict[str, Any]:
        """Analyze the structure of a single transaction"""
        analysis = {
            "tx_hash": tx.get("tx_hash"),
            "tx_type": tx.get("tx_type"),
            "source": tx.get("source"),
            "timestamp": tx.get("block_unix_time"),
            "volume_usd": tx.get("volume_usd"),
            "volume": tx.get("volume"),
            "quote_token": {
                "symbol": tx.get("quote", {}).get("symbol"),
                "address": tx.get("quote", {}).get("address"),
                "decimals": tx.get("quote", {}).get("decimals"),
                "ui_amount": tx.get("quote", {}).get("ui_amount"),
                "ui_change_amount": tx.get("quote", {}).get("ui_change_amount"),
                "type_swap": tx.get("quote", {}).get("type_swap"),
                "price": tx.get("quote", {}).get("price"),
            },
            "base_token": {
                "symbol": tx.get("base", {}).get("symbol"),
                "address": tx.get("base", {}).get("address"),
                "decimals": tx.get("base", {}).get("decimals"),
                "ui_amount": tx.get("base", {}).get("ui_amount"),
                "ui_change_amount": tx.get("base", {}).get("ui_change_amount"),
                "type_swap": tx.get("base", {}).get("type_swap"),
                "price": tx.get("base", {}).get("price"),
            },
            "is_sol_swap": self.is_sol_involved(tx),
            "is_token_to_token": self.is_token_to_token_swap(tx),
            "swap_direction": self.determine_swap_direction(tx),
        }
        return analysis
        
    def is_sol_involved(self, tx: Dict) -> bool:
        """Check if SOL is involved in the transaction"""
        sol_address = "So11111111111111111111111111111111111111112"
        quote_addr = tx.get("quote", {}).get("address", "")
        base_addr = tx.get("base", {}).get("address", "")
        return quote_addr == sol_address or base_addr == sol_address
        
    def is_token_to_token_swap(self, tx: Dict) -> bool:
        """Check if this is a token-to-token swap (no SOL involved)"""
        return not self.is_sol_involved(tx)
        
    def determine_swap_direction(self, tx: Dict) -> str:
        """Determine the swap direction based on change amounts"""
        quote_change = tx.get("quote", {}).get("ui_change_amount", 0)
        base_change = tx.get("base", {}).get("ui_change_amount", 0)
        
        if quote_change > 0 and base_change < 0:
            return "base_to_quote"
        elif quote_change < 0 and base_change > 0:
            return "quote_to_base"
        else:
            return "unknown"
            
    def run_comprehensive_analysis(self):
        """Run comprehensive analysis with multiple addresses and offsets"""
        
        # Test addresses - mix of known active traders and various wallet types
        test_addresses = [
            "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",  # From your example
            "7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5",   # Known active trader
            "A6yAe6LF1taeEbNaiL1Kp4x2FYYtJhVFoNPmqPBmEMzs",   # Another active wallet
            "DYw8jCTfwHNRJhhmFcbXvVDTqWMEVFBX6ZKUmG5CNSKK",   # Different wallet type
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",   # Random wallet
        ]
        
        # Different offset configurations to test pagination
        offset_configs = [0, 10, 25, 50, 100]
        
        all_transactions = []
        analysis_summary = {
            "total_addresses": len(test_addresses),
            "total_transactions": 0,
            "sol_swaps": 0,
            "token_to_token_swaps": 0,
            "unique_sources": set(),
            "unique_tx_types": set(),
            "token_symbols_seen": set(),
            "address_results": {}
        }
        
        for address in test_addresses:
            print(f"\n=== Analyzing Address: {address} ===")
            address_stats = {
                "transactions_found": 0,
                "offsets_tested": [],
                "errors": []
            }
            
            for offset in offset_configs:
                result = self.fetch_transactions(address, offset=offset, limit=20)
                
                if result and result.get("success"):
                    transactions = result.get("data", {}).get("items", [])
                    address_stats["transactions_found"] += len(transactions)
                    address_stats["offsets_tested"].append(offset)
                    
                    for tx in transactions:
                        all_transactions.append(tx)
                        
                        # Analyze transaction structure
                        tx_analysis = self.analyze_transaction_structure(tx)
                        
                        # Update summary statistics
                        analysis_summary["total_transactions"] += 1
                        
                        if tx_analysis["is_sol_swap"]:
                            analysis_summary["sol_swaps"] += 1
                        if tx_analysis["is_token_to_token"]:
                            analysis_summary["token_to_token_swaps"] += 1
                            
                        analysis_summary["unique_sources"].add(tx.get("source", "unknown"))
                        analysis_summary["unique_tx_types"].add(tx.get("tx_type", "unknown"))
                        
                        # Track token symbols
                        if tx_analysis["quote_token"]["symbol"]:
                            analysis_summary["token_symbols_seen"].add(tx_analysis["quote_token"]["symbol"])
                        if tx_analysis["base_token"]["symbol"]:
                            analysis_summary["token_symbols_seen"].add(tx_analysis["base_token"]["symbol"])
                        
                else:
                    address_stats["errors"].append(f"Failed at offset {offset}")
                
                # Be respectful to the API
                time.sleep(0.5)
                
            analysis_summary["address_results"][address] = address_stats
            
        # Convert sets to lists for JSON serialization
        analysis_summary["unique_sources"] = list(analysis_summary["unique_sources"])
        analysis_summary["unique_tx_types"] = list(analysis_summary["unique_tx_types"])
        analysis_summary["token_symbols_seen"] = list(analysis_summary["token_symbols_seen"])
        
        # Save detailed results
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        
        # Save all transactions
        transactions_file = f"transactions_analysis_{timestamp}.json"
        with open(transactions_file, 'w') as f:
            json.dump(all_transactions, f, indent=2)
        print(f"\nSaved {len(all_transactions)} transactions to {transactions_file}")
        
        # Save analysis summary
        summary_file = f"analysis_summary_{timestamp}.json"
        with open(summary_file, 'w') as f:
            json.dump(analysis_summary, f, indent=2)
        print(f"Saved analysis summary to {summary_file}")
        
        # Print summary to console
        self.print_analysis_summary(analysis_summary)
        
        return all_transactions, analysis_summary
        
    def print_analysis_summary(self, summary: Dict):
        """Print a formatted analysis summary"""
        print("\n" + "="*60)
        print("TRANSACTION ANALYSIS SUMMARY")
        print("="*60)
        
        print(f"Total Addresses Analyzed: {summary['total_addresses']}")
        print(f"Total Transactions Found: {summary['total_transactions']}")
        print(f"SOL-involved Swaps: {summary['sol_swaps']}")
        print(f"Token-to-Token Swaps: {summary['token_to_token_swaps']}")
        
        print(f"\nUnique Sources Found: {len(summary['unique_sources'])}")
        for source in sorted(summary['unique_sources']):
            print(f"  - {source}")
            
        print(f"\nUnique Transaction Types: {len(summary['unique_tx_types'])}")
        for tx_type in sorted(summary['unique_tx_types']):
            print(f"  - {tx_type}")
            
        print(f"\nToken Symbols Seen: {len(summary['token_symbols_seen'])}")
        for symbol in sorted(summary['token_symbols_seen'])[:20]:  # Show first 20
            print(f"  - {symbol}")
        if len(summary['token_symbols_seen']) > 20:
            print(f"  ... and {len(summary['token_symbols_seen']) - 20} more")
            
        print(f"\nPer-Address Results:")
        for address, stats in summary['address_results'].items():
            print(f"  {address[:8]}...{address[-8:]}: {stats['transactions_found']} transactions")
            if stats['errors']:
                print(f"    Errors: {stats['errors']}")

def main():
    analyzer = BirdEyeAnalyzer()
    
    print("Starting comprehensive BirdEye transaction analysis...")
    print("This will fetch transactions from multiple addresses with different offsets")
    print("and analyze data structure patterns.\n")
    
    try:
        transactions, summary = analyzer.run_comprehensive_analysis()
        
        print(f"\nAnalysis completed successfully!")
        print(f"Found {len(transactions)} total transactions across {summary['total_addresses']} addresses")
        
    except KeyboardInterrupt:
        print("\nAnalysis interrupted by user")
    except Exception as e:
        print(f"Error during analysis: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    main()