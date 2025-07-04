#!/usr/bin/env python3
"""
DEEP TRANSACTION STRUCTURE ANALYSIS
Thoroughly analyze why duplicate tx_hashes exist in BirdEye data
Don't assume - investigate every field and pattern
"""

import json
import requests
from collections import defaultdict, Counter
from datetime import datetime
import time

class DeepTransactionAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        self.wallet_address = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"

    def fetch_and_save_transactions(self, max_transactions: int = 2000):
        """Fetch transactions and save for analysis"""
        all_transactions = []
        batch_size = 100
        offset = 0
        
        print(f"🔍 Fetching {max_transactions} transactions for deep analysis...")
        
        while len(all_transactions) < max_transactions:
            remaining = max_transactions - len(all_transactions)
            current_limit = min(batch_size, remaining)
            
            params = {
                "address": self.wallet_address,
                "offset": offset,
                "limit": current_limit
            }
            
            print(f"Fetching: offset={offset}, limit={current_limit}")
            response = requests.get(self.base_url, headers=self.headers, params=params)
            
            if response.status_code == 200:
                data = response.json()
                if data.get("success"):
                    batch = data.get("data", {}).get("items", [])
                    if not batch:
                        print(f"No more transactions at offset {offset}")
                        break
                    all_transactions.extend(batch)
                    offset += len(batch)
                    print(f"  ✅ Fetched {len(batch)} transactions")
                else:
                    print(f"  ❌ API returned success=false")
                    break
            else:
                print(f"  ❌ Request failed: {response.status_code}")
                break
            
            time.sleep(0.2)  # Be nice to API
            
            if len(batch) < current_limit:
                break
        
        # Save raw data
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"deep_analysis_transactions_{len(all_transactions)}_{timestamp}.json"
        
        with open(filename, 'w') as f:
            json.dump({
                "metadata": {
                    "total_transactions": len(all_transactions),
                    "wallet_address": self.wallet_address,
                    "fetch_timestamp": timestamp
                },
                "transactions": all_transactions
            }, f, indent=2)
        
        print(f"💾 Saved {len(all_transactions)} transactions to {filename}")
        return all_transactions, filename

    def analyze_duplicate_patterns(self, transactions):
        """Deep analysis of duplicate transaction patterns"""
        print(f"\n🔍 DEEP DUPLICATE PATTERN ANALYSIS")
        print("=" * 80)
        
        # Group by tx_hash
        tx_groups = defaultdict(list)
        for i, tx in enumerate(transactions):
            tx_hash = tx.get('tx_hash')
            if tx_hash:
                tx_groups[tx_hash].append((i, tx))
        
        duplicates = {hash: group for hash, group in tx_groups.items() if len(group) > 1}
        
        print(f"Total unique tx_hashes: {len(tx_groups)}")
        print(f"Duplicate tx_hashes: {len(duplicates)}")
        print(f"Total transactions: {len(transactions)}")
        print(f"Duplicate entries: {sum(len(group) for group in duplicates.values()) - len(duplicates)}")
        
        print(f"\n📊 DETAILED DUPLICATE ANALYSIS:")
        
        for i, (tx_hash, group) in enumerate(duplicates.items(), 1):
            print(f"\n{i}. DUPLICATE HASH: {tx_hash}")
            print(f"   Entries: {len(group)}")
            print(f"   Original positions: {[pos for pos, _ in group]}")
            
            # Analyze every field difference
            self._compare_duplicate_entries(group)
    
    def _compare_duplicate_entries(self, group):
        """Compare every field in duplicate entries"""
        if len(group) < 2:
            return
        
        # Extract just the transaction data
        transactions = [tx for _, tx in group]
        
        print(f"   🔍 FIELD-BY-FIELD COMPARISON:")
        
        # Get all possible fields from all transactions
        all_fields = set()
        for tx in transactions:
            all_fields.update(self._get_all_field_paths(tx))
        
        # Compare each field
        differences = []
        identical_fields = []
        
        for field_path in sorted(all_fields):
            values = []
            for tx in transactions:
                value = self._get_nested_value(tx, field_path)
                values.append(value)
            
            if len(set(str(v) for v in values)) > 1:  # Different values
                differences.append((field_path, values))
            else:
                identical_fields.append(field_path)
        
        if differences:
            print(f"   🚨 DIFFERENT FIELDS ({len(differences)}):")
            for field_path, values in differences:
                print(f"     {field_path}:")
                for i, value in enumerate(values):
                    print(f"       [{i+1}] {value}")
        else:
            print(f"   ✅ ALL FIELDS IDENTICAL")
        
        print(f"   📋 IDENTICAL FIELDS: {len(identical_fields)}")
        
        # Special analysis for important fields
        self._analyze_critical_fields(transactions)
    
    def _analyze_critical_fields(self, transactions):
        """Analyze critical fields that might explain duplicates"""
        print(f"   🎯 CRITICAL FIELD ANALYSIS:")
        
        critical_fields = [
            'ins_index', 'inner_ins_index', 'volume_usd', 'volume',
            'quote.ui_change_amount', 'base.ui_change_amount',
            'quote.symbol', 'base.symbol', 'quote.type', 'base.type',
            'quote.type_swap', 'base.type_swap', 'source', 'tx_type',
            'block_unix_time', 'block_number', 'address', 'owner'
        ]
        
        for field in critical_fields:
            values = []
            for tx in transactions:
                value = self._get_nested_value(tx, field)
                values.append(value)
            
            if len(set(str(v) for v in values)) > 1:
                print(f"     🔥 {field}: {values}")
        
        # Check if this represents multi-instruction transaction
        ins_indices = [self._get_nested_value(tx, 'ins_index') for tx in transactions]
        inner_ins_indices = [self._get_nested_value(tx, 'inner_ins_index') for tx in transactions]
        
        if len(set(str(i) for i in ins_indices)) > 1 or len(set(str(i) for i in inner_ins_indices)) > 1:
            print(f"     💡 MULTI-INSTRUCTION TRANSACTION:")
            print(f"        ins_index: {ins_indices}")
            print(f"        inner_ins_index: {inner_ins_indices}")
            print(f"        → This tx_hash contains multiple instructions")
        
        # Check amounts - might be partial amounts of larger transaction
        quote_amounts = [self._get_nested_value(tx, 'quote.ui_change_amount') for tx in transactions]
        base_amounts = [self._get_nested_value(tx, 'base.ui_change_amount') for tx in transactions]
        
        print(f"     📊 AMOUNTS:")
        print(f"        Quote changes: {quote_amounts}")
        print(f"        Base changes: {base_amounts}")
        
        if len(transactions) == 2:
            total_quote = sum(float(a) for a in quote_amounts if a is not None)
            total_base = sum(float(a) for a in base_amounts if a is not None)
            print(f"        Net effect: Quote {total_quote:+.6f}, Base {total_base:+.6f}")

    def _get_all_field_paths(self, obj, prefix=""):
        """Get all field paths in nested object"""
        paths = set()
        if isinstance(obj, dict):
            for key, value in obj.items():
                current_path = f"{prefix}.{key}" if prefix else key
                paths.add(current_path)
                if isinstance(value, dict):
                    paths.update(self._get_all_field_paths(value, current_path))
                elif isinstance(value, list) and value and isinstance(value[0], dict):
                    paths.update(self._get_all_field_paths(value[0], f"{current_path}[0]"))
        return paths
    
    def _get_nested_value(self, obj, field_path):
        """Get value from nested field path"""
        try:
            current = obj
            for part in field_path.split('.'):
                if '[0]' in part:
                    key = part.replace('[0]', '')
                    current = current[key][0] if current.get(key) else None
                else:
                    current = current[part]
            return current
        except (KeyError, TypeError, IndexError, AttributeError):
            return None

    def analyze_instruction_patterns(self, transactions):
        """Analyze instruction index patterns across all transactions"""
        print(f"\n🔍 INSTRUCTION PATTERN ANALYSIS")
        print("=" * 80)
        
        # Analyze all instruction combinations
        ins_combinations = defaultdict(list)
        
        for i, tx in enumerate(transactions):
            ins_index = tx.get('ins_index')
            inner_ins_index = tx.get('inner_ins_index')
            tx_hash = tx.get('tx_hash')
            
            key = (ins_index, inner_ins_index)
            ins_combinations[key].append((i, tx_hash, tx))
        
        print(f"Instruction index combinations found:")
        for (ins, inner), txs in sorted(ins_combinations.items(), key=lambda x: len(x[1]), reverse=True):
            print(f"  (ins:{ins}, inner:{inner}): {len(txs)} transactions")
            
            # Check for duplicates within this combination
            hash_counts = Counter(tx_hash for _, tx_hash, _ in txs)
            duplicates_in_combo = {h: c for h, c in hash_counts.items() if c > 1}
            
            if duplicates_in_combo:
                print(f"    🚨 Duplicates in this combination: {len(duplicates_in_combo)} hashes")
                for hash, count in list(duplicates_in_combo.items())[:3]:  # Show first 3
                    print(f"      {hash[:16]}...: {count} times")

    def analyze_temporal_patterns(self, transactions):
        """Analyze temporal patterns that might explain duplicates"""
        print(f"\n🔍 TEMPORAL PATTERN ANALYSIS")
        print("=" * 80)
        
        # Group by block_unix_time and block_number
        time_groups = defaultdict(list)
        block_groups = defaultdict(list)
        
        for tx in transactions:
            block_time = tx.get('block_unix_time')
            block_number = tx.get('block_number')
            
            if block_time:
                time_groups[block_time].append(tx)
            if block_number:
                block_groups[block_number].append(tx)
        
        # Find blocks with multiple transactions
        multi_tx_blocks = {block: txs for block, txs in block_groups.items() if len(txs) > 1}
        
        print(f"Blocks with multiple transactions: {len(multi_tx_blocks)}")
        
        for block_num, txs in list(multi_tx_blocks.items())[:5]:  # Show first 5
            print(f"\n  Block {block_num}: {len(txs)} transactions")
            
            # Check for duplicate hashes within same block
            hashes = [tx.get('tx_hash') for tx in txs]
            hash_counts = Counter(hashes)
            duplicates = {h: c for h, c in hash_counts.items() if c > 1}
            
            if duplicates:
                print(f"    🚨 Duplicate hashes within block: {duplicates}")
            
            # Show transaction details
            for i, tx in enumerate(txs[:3]):  # First 3 txs
                quote_sym = tx.get('quote', {}).get('symbol', 'N/A')
                base_sym = tx.get('base', {}).get('symbol', 'N/A')
                ins_index = tx.get('ins_index', 'N/A')
                tx_hash = tx.get('tx_hash', 'N/A')[:16]
                print(f"    [{i+1}] {quote_sym}↔{base_sym} ins:{ins_index} hash:{tx_hash}...")

    def run_complete_analysis(self):
        """Run complete deep analysis"""
        print(f"🚀 DEEP TRANSACTION STRUCTURE ANALYSIS")
        print(f"🎯 Goal: Understand WHY duplicate tx_hashes exist")
        print("=" * 80)
        
        # Fetch and save data
        transactions, filename = self.fetch_and_save_transactions(2000)
        
        if not transactions:
            print("❌ No transactions to analyze")
            return
        
        # Run all analyses
        self.analyze_duplicate_patterns(transactions)
        self.analyze_instruction_patterns(transactions)
        self.analyze_temporal_patterns(transactions)
        
        print(f"\n💾 All data saved to: {filename}")
        print(f"📋 Analysis complete - {len(transactions)} transactions analyzed")
        
        return filename

def main():
    analyzer = DeepTransactionAnalyzer()
    analyzer.run_complete_analysis()

if __name__ == "__main__":
    main()