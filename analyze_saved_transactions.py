#!/usr/bin/env python3
"""
Analyze the saved transaction data to understand duplicate patterns
"""

import json
from collections import defaultdict, Counter

def analyze_saved_data(filename):
    """Analyze the saved transaction data"""
    print(f"🔍 ANALYZING SAVED TRANSACTION DATA: {filename}")
    print("=" * 80)
    
    with open(filename, 'r') as f:
        data = json.load(f)
    
    transactions = data['transactions']
    print(f"Total transactions: {len(transactions)}")
    
    # Group by tx_hash
    tx_groups = defaultdict(list)
    for i, tx in enumerate(transactions):
        tx_hash = tx.get('tx_hash')
        if tx_hash:
            tx_groups[tx_hash].append((i, tx))
    
    duplicates = {hash: group for hash, group in tx_groups.items() if len(group) > 1}
    
    print(f"Unique tx_hashes: {len(tx_groups)}")
    print(f"Duplicate tx_hashes: {len(duplicates)}")
    print(f"Duplicate entries: {sum(len(group) for group in duplicates.values()) - len(duplicates)}")
    
    # Analyze the first few duplicates in detail
    print(f"\n📊 DETAILED DUPLICATE ANALYSIS (first 10):")
    
    for i, (tx_hash, group) in enumerate(list(duplicates.items())[:10], 1):
        print(f"\n{i}. HASH: {tx_hash[:32]}...")
        print(f"   Entries: {len(group)} at positions {[pos for pos, _ in group]}")
        
        transactions_data = [tx for _, tx in group]
        
        # Compare critical fields
        critical_fields = [
            'ins_index', 'inner_ins_index', 'volume_usd', 'volume',
            'quote.ui_change_amount', 'base.ui_change_amount',
            'quote.symbol', 'base.symbol', 'source', 'address'
        ]
        
        print(f"   📋 CRITICAL FIELD COMPARISON:")
        differences = []
        
        for field in critical_fields:
            values = []
            for tx in transactions_data:
                if '.' in field:
                    parts = field.split('.')
                    value = tx.get(parts[0], {}).get(parts[1]) if tx.get(parts[0]) else None
                else:
                    value = tx.get(field)
                values.append(value)
            
            unique_values = list(set(str(v) for v in values))
            if len(unique_values) > 1:
                differences.append((field, values))
                print(f"     🔥 {field}: {values}")
            
        if not differences:
            print(f"     ✅ ALL CRITICAL FIELDS IDENTICAL")
        
        # Show transaction details
        print(f"   📊 TRANSACTION DETAILS:")
        for j, (pos, tx) in enumerate(group):
            quote_sym = tx.get('quote', {}).get('symbol', 'N/A')
            base_sym = tx.get('base', {}).get('symbol', 'N/A')
            quote_change = tx.get('quote', {}).get('ui_change_amount', 0)
            base_change = tx.get('base', {}).get('ui_change_amount', 0)
            ins_index = tx.get('ins_index', 'N/A')
            inner_ins_index = tx.get('inner_ins_index', 'N/A')
            volume = tx.get('volume_usd', 0)
            source = tx.get('source', 'N/A')
            
            print(f"     [{j+1}] pos:{pos} {quote_sym} {quote_change:+.2f} ↔ {base_sym} {base_change:+.2f}")
            print(f"         ins:{ins_index}, inner:{inner_ins_index}, vol:${volume:.2f}, src:{source}")

    # Analyze instruction patterns
    print(f"\n🔍 INSTRUCTION PATTERN ANALYSIS:")
    
    ins_combinations = defaultdict(list)
    for tx in transactions:
        ins_index = tx.get('ins_index')
        inner_ins_index = tx.get('inner_ins_index')
        tx_hash = tx.get('tx_hash')
        
        key = (ins_index, inner_ins_index)
        ins_combinations[key].append(tx_hash)
    
    print(f"Instruction combinations:")
    for (ins, inner), hashes in sorted(ins_combinations.items(), key=lambda x: len(x[1]), reverse=True):
        hash_counts = Counter(hashes)
        duplicates_in_combo = sum(1 for count in hash_counts.values() if count > 1)
        print(f"  (ins:{ins}, inner:{inner}): {len(hashes)} txs, {duplicates_in_combo} duplicate hashes")

    # Analyze sources and duplicate patterns
    print(f"\n🔍 SOURCE PATTERN ANALYSIS:")
    source_duplicates = defaultdict(int)
    for group in duplicates.values():
        sources = [tx.get('source') for _, tx in group]
        unique_sources = set(sources)
        for source in unique_sources:
            source_duplicates[source] += 1
    
    print(f"Sources involved in duplicates:")
    for source, count in sorted(source_duplicates.items(), key=lambda x: x[1], reverse=True):
        print(f"  {source}: {count} duplicate groups")

    # Show distribution of duplicate sizes
    print(f"\n📊 DUPLICATE SIZE DISTRIBUTION:")
    size_counts = Counter(len(group) for group in duplicates.values())
    for size, count in sorted(size_counts.items()):
        print(f"  {size} entries per hash: {count} duplicate groups")

    # Save detailed duplicate analysis
    duplicate_analysis = {}
    for tx_hash, group in duplicates.items():
        duplicate_analysis[tx_hash] = {
            'count': len(group),
            'positions': [pos for pos, _ in group],
            'transactions': [tx for _, tx in group]
        }
    
    analysis_filename = f"duplicate_analysis_{len(duplicates)}_groups.json"
    with open(analysis_filename, 'w') as f:
        json.dump(duplicate_analysis, f, indent=2)
    
    print(f"\n💾 Detailed duplicate analysis saved to: {analysis_filename}")

def main():
    # Use the most recent saved file
    filename = "deep_analysis_transactions_2000_20250704_091138.json"
    analyze_saved_data(filename)

if __name__ == "__main__":
    main()