#!/usr/bin/env python3
"""
Comprehensive BirdEye Transaction Data Structure Analysis
Fetch up to 5000 transactions and analyze:
- Data structure and fields
- Transaction hash uniqueness
- Transaction patterns and groupings
- Field value distributions
- Any data anomalies
"""

import json
import requests
from collections import defaultdict, Counter
from datetime import datetime
import time

class BirdEyeDataStructureAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        self.wallet_address = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"

    def fetch_batch(self, limit: int = 100, offset: int = 0):
        """Fetch a batch of transactions"""
        params = {
            "address": self.wallet_address,
            "offset": offset,
            "limit": limit
        }
        
        print(f"Fetching transactions: limit={limit}, offset={offset}")
        
        response = requests.get(self.base_url, headers=self.headers, params=params)
        if response.status_code == 200:
            data = response.json()
            if data.get("success"):
                transactions = data.get("data", {}).get("items", [])
                print(f"  ✅ Fetched {len(transactions)} transactions")
                return transactions
            else:
                print(f"  ❌ API returned success=false: {data}")
                return []
        else:
            print(f"  ❌ Request failed: {response.status_code}")
            return []

    def fetch_transactions(self, max_transactions: int = 5000):
        """Fetch up to max_transactions in batches of 100"""
        all_transactions = []
        batch_size = 100
        offset = 0
        
        print(f"🔍 Fetching up to {max_transactions} transactions...")
        
        while len(all_transactions) < max_transactions:
            remaining = max_transactions - len(all_transactions)
            current_limit = min(batch_size, remaining)
            
            batch = self.fetch_batch(limit=current_limit, offset=offset)
            
            if not batch:
                print(f"No more transactions available at offset {offset}")
                break
            
            all_transactions.extend(batch)
            offset += len(batch)
            
            # Be nice to the API
            time.sleep(0.5)
            
            if len(batch) < current_limit:
                print(f"Received fewer transactions than requested - end of data")
                break
        
        print(f"📊 Total transactions fetched: {len(all_transactions)}")
        return all_transactions

    def analyze_transaction_structure(self, transactions):
        """Analyze the structure of individual transactions"""
        print(f"\n🔍 TRANSACTION STRUCTURE ANALYSIS")
        print("=" * 60)
        
        if not transactions:
            print("No transactions to analyze")
            return
        
        # Analyze first transaction structure
        sample_tx = transactions[0]
        print(f"Sample transaction fields:")
        self._print_nested_structure(sample_tx, indent=2)
        
        # Check field consistency across all transactions
        print(f"\n📊 FIELD CONSISTENCY ACROSS {len(transactions)} TRANSACTIONS:")
        print("-" * 50)
        
        all_fields = set()
        field_presence = defaultdict(int)
        
        for tx in transactions:
            tx_fields = self._get_all_fields(tx)
            all_fields.update(tx_fields)
            for field in tx_fields:
                field_presence[field] += 1
        
        print(f"Total unique fields found: {len(all_fields)}")
        print(f"Fields present in all transactions:")
        for field in sorted(all_fields):
            presence_rate = field_presence[field] / len(transactions)
            if presence_rate == 1.0:
                print(f"  ✅ {field}")
            elif presence_rate > 0.9:
                print(f"  ⚠️ {field} ({presence_rate:.1%})")
            else:
                print(f"  ❌ {field} ({presence_rate:.1%})")

    def _print_nested_structure(self, obj, indent=0, max_depth=3):
        """Print nested object structure"""
        if indent > max_depth * 2:
            return
        
        spaces = " " * indent
        if isinstance(obj, dict):
            for key, value in obj.items():
                if isinstance(value, (dict, list)):
                    print(f"{spaces}{key}: {type(value).__name__}")
                    if indent < max_depth * 2:
                        self._print_nested_structure(value, indent + 2, max_depth)
                else:
                    print(f"{spaces}{key}: {type(value).__name__} = {str(value)[:50]}")
        elif isinstance(obj, list) and obj:
            print(f"{spaces}[{len(obj)} items, first item:]")
            self._print_nested_structure(obj[0], indent + 2, max_depth)

    def _get_all_fields(self, obj, prefix=""):
        """Get all field paths in a nested object"""
        fields = set()
        if isinstance(obj, dict):
            for key, value in obj.items():
                field_path = f"{prefix}.{key}" if prefix else key
                fields.add(field_path)
                if isinstance(value, dict):
                    fields.update(self._get_all_fields(value, field_path))
        return fields

    def analyze_transaction_hashes(self, transactions):
        """Analyze transaction hash uniqueness and patterns"""
        print(f"\n🔍 TRANSACTION HASH ANALYSIS")
        print("=" * 60)
        
        tx_hashes = [tx.get('tx_hash') for tx in transactions if tx.get('tx_hash')]
        
        print(f"Total transactions: {len(transactions)}")
        print(f"Transactions with tx_hash: {len(tx_hashes)}")
        print(f"Unique tx_hashes: {len(set(tx_hashes))}")
        
        # Find duplicates
        hash_counts = Counter(tx_hashes)
        duplicates = {h: count for h, count in hash_counts.items() if count > 1}
        
        if duplicates:
            print(f"\n🚨 DUPLICATE TRANSACTION HASHES FOUND:")
            for tx_hash, count in sorted(duplicates.items(), key=lambda x: x[1], reverse=True):
                print(f"  {tx_hash}: {count} occurrences")
                
                # Show details of duplicate transactions
                duplicate_txs = [tx for tx in transactions if tx.get('tx_hash') == tx_hash]
                print(f"    Details:")
                for i, tx in enumerate(duplicate_txs):
                    quote_sym = tx.get('quote', {}).get('symbol', 'N/A')
                    base_sym = tx.get('base', {}).get('symbol', 'N/A')
                    quote_change = tx.get('quote', {}).get('ui_change_amount', 0)
                    base_change = tx.get('base', {}).get('ui_change_amount', 0)
                    ins_index = tx.get('ins_index', 'N/A')
                    inner_ins_index = tx.get('inner_ins_index', 'N/A')
                    print(f"    [{i+1}] {quote_sym} {quote_change:+.2f} ↔ {base_sym} {base_change:+.2f} " +
                          f"(ins:{ins_index}, inner:{inner_ins_index})")
        else:
            print(f"✅ All transaction hashes are unique")

    def analyze_instruction_patterns(self, transactions):
        """Analyze instruction indices and patterns"""
        print(f"\n🔍 INSTRUCTION INDEX PATTERNS")
        print("=" * 60)
        
        ins_indices = [tx.get('ins_index') for tx in transactions if tx.get('ins_index') is not None]
        inner_ins_indices = [tx.get('inner_ins_index') for tx in transactions if tx.get('inner_ins_index') is not None]
        
        print(f"Instruction index distribution:")
        ins_counter = Counter(ins_indices)
        for idx, count in sorted(ins_counter.items()):
            print(f"  ins_index {idx}: {count} transactions ({count/len(transactions):.1%})")
        
        print(f"\nInner instruction index distribution:")
        inner_ins_counter = Counter(inner_ins_indices)
        for idx, count in sorted(inner_ins_counter.items()):
            print(f"  inner_ins_index {idx}: {count} transactions ({count/len(transactions):.1%})")
        
        # Analyze combinations
        combinations = [(tx.get('ins_index'), tx.get('inner_ins_index')) for tx in transactions]
        combo_counter = Counter(combinations)
        
        print(f"\nTop instruction index combinations:")
        for (ins, inner), count in combo_counter.most_common(10):
            print(f"  (ins:{ins}, inner:{inner}): {count} transactions ({count/len(transactions):.1%})")

    def analyze_token_patterns(self, transactions):
        """Analyze token symbols and addresses"""
        print(f"\n🔍 TOKEN PATTERNS ANALYSIS")
        print("=" * 60)
        
        quote_symbols = [tx.get('quote', {}).get('symbol') for tx in transactions]
        base_symbols = [tx.get('base', {}).get('symbol') for tx in transactions]
        
        all_symbols = quote_symbols + base_symbols
        symbol_counter = Counter([s for s in all_symbols if s])
        
        print(f"Top 20 token symbols:")
        for symbol, count in symbol_counter.most_common(20):
            print(f"  {symbol}: {count} occurrences")
        
        # Analyze token pairs
        token_pairs = []
        for tx in transactions:
            quote_sym = tx.get('quote', {}).get('symbol')
            base_sym = tx.get('base', {}).get('symbol')
            if quote_sym and base_sym:
                pair = tuple(sorted([quote_sym, base_sym]))
                token_pairs.append(pair)
        
        pair_counter = Counter(token_pairs)
        print(f"\nTop 15 token pairs:")
        for pair, count in pair_counter.most_common(15):
            print(f"  {pair[0]} ↔ {pair[1]}: {count} transactions")

    def analyze_transaction_types(self, transactions):
        """Analyze transaction type patterns"""
        print(f"\n🔍 TRANSACTION TYPE ANALYSIS")
        print("=" * 60)
        
        tx_types = [tx.get('tx_type') for tx in transactions if tx.get('tx_type')]
        type_counter = Counter(tx_types)
        
        print(f"Transaction types:")
        for tx_type, count in type_counter.most_common():
            print(f"  {tx_type}: {count} transactions ({count/len(transactions):.1%})")
        
        # Analyze sources
        sources = [tx.get('source') for tx in transactions if tx.get('source')]
        source_counter = Counter(sources)
        
        print(f"\nTransaction sources:")
        for source, count in source_counter.most_common():
            print(f"  {source}: {count} transactions ({count/len(transactions):.1%})")

    def analyze_value_patterns(self, transactions):
        """Analyze transaction value patterns"""
        print(f"\n🔍 VALUE PATTERNS ANALYSIS")
        print("=" * 60)
        
        # Analyze volume_usd distribution
        volumes = [tx.get('volume_usd', 0) for tx in transactions if tx.get('volume_usd')]
        if volumes:
            volumes_sorted = sorted(volumes)
            print(f"USD Volume statistics:")
            print(f"  Count: {len(volumes)}")
            print(f"  Min: ${min(volumes):,.2f}")
            print(f"  Max: ${max(volumes):,.2f}")
            print(f"  Median: ${volumes_sorted[len(volumes_sorted)//2]:,.2f}")
            print(f"  Mean: ${sum(volumes)/len(volumes):,.2f}")
        
        # Analyze change amount patterns
        quote_changes = []
        base_changes = []
        
        for tx in transactions:
            if tx.get('quote', {}).get('ui_change_amount') is not None:
                quote_changes.append(tx['quote']['ui_change_amount'])
            if tx.get('base', {}).get('ui_change_amount') is not None:
                base_changes.append(tx['base']['ui_change_amount'])
        
        print(f"\nChange amount patterns:")
        print(f"  Quote positive changes: {sum(1 for x in quote_changes if x > 0)} ({sum(1 for x in quote_changes if x > 0)/len(quote_changes):.1%})")
        print(f"  Quote negative changes: {sum(1 for x in quote_changes if x < 0)} ({sum(1 for x in quote_changes if x < 0)/len(quote_changes):.1%})")
        print(f"  Base positive changes: {sum(1 for x in base_changes if x > 0)} ({sum(1 for x in base_changes if x > 0)/len(base_changes):.1%})")
        print(f"  Base negative changes: {sum(1 for x in base_changes if x < 0)} ({sum(1 for x in base_changes if x < 0)/len(base_changes):.1%})")

    def analyze_timestamp_patterns(self, transactions):
        """Analyze timestamp patterns"""
        print(f"\n🔍 TIMESTAMP PATTERNS")
        print("=" * 60)
        
        timestamps = [tx.get('block_unix_time') for tx in transactions if tx.get('block_unix_time')]
        if timestamps:
            timestamps_sorted = sorted(timestamps)
            
            print(f"Timestamp range:")
            print(f"  Earliest: {datetime.fromtimestamp(min(timestamps))}")
            print(f"  Latest: {datetime.fromtimestamp(max(timestamps))}")
            print(f"  Span: {(max(timestamps) - min(timestamps)) / 86400:.1f} days")
            
            # Check for timestamp ordering
            original_order = [tx.get('block_unix_time') for tx in transactions if tx.get('block_unix_time')]
            is_sorted = original_order == sorted(original_order, reverse=True)
            print(f"  Sorted (newest first): {'Yes' if is_sorted else 'No'}")

    def run_comprehensive_analysis(self, max_transactions: int = 5000):
        """Run all analyses"""
        print(f"🚀 COMPREHENSIVE BIRDEYE DATA STRUCTURE ANALYSIS")
        print(f"Target: {max_transactions} transactions")
        print("=" * 80)
        
        # Fetch transactions
        transactions = self.fetch_transactions(max_transactions)
        
        if not transactions:
            print("❌ No transactions fetched - cannot proceed with analysis")
            return None
        
        # Run all analyses
        self.analyze_transaction_structure(transactions)
        self.analyze_transaction_hashes(transactions)
        self.analyze_instruction_patterns(transactions)
        self.analyze_token_patterns(transactions)
        self.analyze_transaction_types(transactions)
        self.analyze_value_patterns(transactions)
        self.analyze_timestamp_patterns(transactions)
        
        # Save raw data for further analysis
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"birdeye_raw_data_{len(transactions)}tx_{timestamp}.json"
        
        with open(filename, 'w') as f:
            json.dump({
                "analysis_timestamp": timestamp,
                "total_transactions": len(transactions),
                "transactions": transactions
            }, f, indent=2)
        
        print(f"\n💾 Raw data saved to: {filename}")
        
        return transactions

def main():
    analyzer = BirdEyeDataStructureAnalyzer()
    analyzer.run_comprehensive_analysis(max_transactions=1000)  # Start with 1000

if __name__ == "__main__":
    main()