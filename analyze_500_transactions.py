#!/usr/bin/env python3
"""
Analyze 500 transactions in batches of 100 (offset 0, 100, 200, 300, 400)
to understand transaction patterns and expected event generation
"""

import json
import requests
from decimal import Decimal, getcontext
from datetime import datetime
import time

getcontext().prec = 28

class TransactionAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        self.sol_address = "So11111111111111111111111111111111111111112"
        self.wallet_address = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"

    def fetch_transactions(self, limit: int = 100, offset: int = 0):
        """Fetch transactions for analysis"""
        params = {
            "address": self.wallet_address,
            "offset": offset,
            "limit": limit
        }
        
        print(f"🔍 Fetching {limit} transactions (offset={offset})...")
        
        response = requests.get(self.base_url, headers=self.headers, params=params)
        if response.status_code == 200:
            data = response.json()
            if data.get("success"):
                transactions = data.get("data", {}).get("items", [])
                print(f"✅ Successfully fetched {len(transactions)} transactions")
                return transactions
            else:
                print(f"❌ API returned success=false: {data}")
                return []
        else:
            print(f"❌ API request failed: {response.status_code} - {response.text}")
            return []

    def analyze_batch(self, transactions, batch_num):
        """Analyze a batch of transactions"""
        print(f"\n📊 BATCH {batch_num} ANALYSIS ({len(transactions)} transactions):")
        print("-" * 50)
        
        transaction_types = {
            'sol_to_token': 0,
            'token_to_sol': 0, 
            'token_to_token': 0
        }
        
        event_count = 0
        token_pairs = set()
        
        for i, tx in enumerate(transactions):
            quote = tx['quote']
            base = tx['base']
            
            # Track unique token pairs
            pair = f"{quote['symbol']} ↔ {base['symbol']}"
            token_pairs.add(pair)
            
            quote_change = Decimal(str(quote['ui_change_amount']))
            base_change = Decimal(str(base['ui_change_amount']))
            
            # Show first few transactions in each batch
            if i < 3:
                print(f"  TX{i+1}: {quote['symbol']} {quote_change:+.2f} ↔ {base['symbol']} {base_change:+.2f}")
            
            # Determine transaction type and expected events
            if quote['address'] == self.sol_address or base['address'] == self.sol_address:
                if (quote['address'] == self.sol_address and quote_change < 0) or (base['address'] == self.sol_address and base_change < 0):
                    transaction_types['sol_to_token'] += 1
                    event_count += 1
                else:
                    transaction_types['token_to_sol'] += 1
                    event_count += 1
            else:
                transaction_types['token_to_token'] += 1
                event_count += 2  # Token-to-token generates 2 events
        
        print(f"  Token pairs in batch: {len(token_pairs)}")
        for pair in sorted(token_pairs):
            print(f"    - {pair}")
        
        print(f"  SOL → Token: {transaction_types['sol_to_token']} transactions")
        print(f"  Token → SOL: {transaction_types['token_to_sol']} transactions") 
        print(f"  Token → Token: {transaction_types['token_to_token']} transactions")
        print(f"  Expected events: {event_count}")
        
        return transaction_types, event_count, token_pairs

    def analyze_500_transactions(self):
        """Fetch and analyze 500 transactions in 5 batches of 100"""
        print("🚀 COMPREHENSIVE 500 TRANSACTION ANALYSIS")
        print("=" * 80)
        
        total_stats = {
            'sol_to_token': 0,
            'token_to_sol': 0, 
            'token_to_token': 0
        }
        total_events = 0
        all_token_pairs = set()
        all_transactions = []
        
        # Fetch 5 batches of 100 transactions each
        for batch in range(5):
            offset = batch * 100
            
            # Add delay between requests to be nice to API
            if batch > 0:
                time.sleep(1)
            
            transactions = self.fetch_transactions(limit=100, offset=offset)
            
            if not transactions:
                print(f"⚠️ No transactions returned for batch {batch+1}")
                continue
            
            all_transactions.extend(transactions)
            batch_stats, batch_events, batch_pairs = self.analyze_batch(transactions, batch+1)
            
            # Add to totals
            for key in total_stats:
                total_stats[key] += batch_stats[key]
            total_events += batch_events
            all_token_pairs.update(batch_pairs)
        
        # Overall summary
        print(f"\n🎯 OVERALL SUMMARY (500 transactions):")
        print("=" * 50)
        print(f"Total transactions fetched: {len(all_transactions)}")
        print(f"Unique token pairs: {len(all_token_pairs)}")
        
        print(f"\nToken pair distribution:")
        for pair in sorted(all_token_pairs):
            print(f"  - {pair}")
        
        print(f"\nTransaction type totals:")
        print(f"  SOL → Token: {total_stats['sol_to_token']} transactions = {total_stats['sol_to_token']} events")
        print(f"  Token → SOL: {total_stats['token_to_sol']} transactions = {total_stats['token_to_sol']} events")
        print(f"  Token → Token: {total_stats['token_to_token']} transactions = {total_stats['token_to_token'] * 2} events")
        print(f"  Total expected events: {total_events}")
        
        # Compare with known issues
        print(f"\n🚨 COMPARISON WITH KNOWN ISSUES:")
        print(f"Expected events (500 tx): {total_events}")
        print(f"Python generates ~{total_events/100*104:.0f} events (104 events per 100 tx)")
        print(f"Rust generates ~{total_events/100*511:.0f} events (511 events per 100 tx) ❌")
        print(f"Rust over-generation factor: {511/104:.1f}x")
        
        # Event generation insight
        token_token_percentage = (total_stats['token_to_token'] / len(all_transactions)) * 100
        print(f"\nKey insights:")
        print(f"- Token→Token swaps: {token_token_percentage:.1f}% of transactions")
        print(f"- These generate 2 events each (SELL + BUY)")
        print(f"- SOL swaps: {100-token_token_percentage:.1f}% generate 1 event each")
        
        if total_stats['token_to_token'] > 0:
            print(f"\n⚠️ Token-to-token swaps found! This explains extra events in Python (104 vs 100)")
        else:
            print(f"\n✅ All swaps involve SOL - should be exactly 1 event per transaction")
        
        # Save analysis results
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"transaction_analysis_500_{timestamp}.json"
        
        analysis_result = {
            "total_transactions": len(all_transactions),
            "transaction_types": total_stats,
            "total_expected_events": total_events,
            "unique_token_pairs": list(all_token_pairs),
            "rust_issue_scale": f"{511/104:.1f}x over-generation"
        }
        
        with open(filename, 'w') as f:
            json.dump(analysis_result, f, indent=2)
        
        print(f"\n💾 Analysis saved to: {filename}")
        
        return analysis_result

def main():
    analyzer = TransactionAnalyzer()
    results = analyzer.analyze_500_transactions()

if __name__ == "__main__":
    main()