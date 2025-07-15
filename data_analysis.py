#!/usr/bin/env python3

import json
import os
from collections import defaultdict
from datetime import datetime

def analyze_transaction_data(file_path):
    """Analyze Birdeye transaction data to understand the structure"""
    with open(file_path, 'r') as f:
        data = json.load(f)
    
    wallet_address = os.path.basename(file_path).replace('transactions_', '').replace('.json', '')
    print(f"\n=== ANALYZING WALLET: {wallet_address} ===")
    
    transactions = data['data']['items']
    print(f"Total transactions: {len(transactions)}")
    
    # Group by tx_hash and block_unix_time
    by_tx_hash = defaultdict(list)
    by_block_time = defaultdict(list)
    
    for tx in transactions:
        by_tx_hash[tx['tx_hash']].append(tx)
        by_block_time[tx['block_unix_time']].append(tx)
    
    print(f"Unique tx_hash count: {len(by_tx_hash)}")
    print(f"Unique block_unix_time count: {len(by_block_time)}")
    
    # Look for patterns
    print("\n--- TRANSACTION PATTERNS ---")
    
    # Check for same tx_hash with different records
    multi_record_tx = {tx_hash: records for tx_hash, records in by_tx_hash.items() if len(records) > 1}
    print(f"Transactions with multiple records (same tx_hash): {len(multi_record_tx)}")
    
    if multi_record_tx:
        # Analyze first multi-record transaction
        first_tx_hash = list(multi_record_tx.keys())[0]
        records = multi_record_tx[first_tx_hash]
        print(f"\nExample multi-record transaction: {first_tx_hash}")
        print(f"Records count: {len(records)}")
        
        for i, record in enumerate(records):
            print(f"  Record {i+1}:")
            print(f"    Quote: {record['quote']['symbol']} {record['quote']['ui_change_amount']}")
            print(f"    Base: {record['base']['symbol']} {record['base']['ui_change_amount']}")
            print(f"    Volume USD: {record['volume_usd']}")
            print(f"    Address: {record['address']}")
            print(f"    Inner ins index: {record.get('inner_ins_index', 'None')}")
    
    # Analyze first few transactions to understand the pattern
    print("\n--- FIRST 5 TRANSACTIONS ---")
    for i, tx in enumerate(transactions[:5]):
        print(f"\nTransaction {i+1}:")
        print(f"  TX Hash: {tx['tx_hash']}")
        print(f"  Block time: {tx['block_unix_time']}")
        print(f"  Quote: {tx['quote']['symbol']} change={tx['quote']['ui_change_amount']} type_swap={tx['quote']['type_swap']}")
        print(f"  Base: {tx['base']['symbol']} change={tx['base']['ui_change_amount']} type_swap={tx['base']['type_swap']}")
        print(f"  Volume USD: {tx['volume_usd']}")
        print(f"  Address: {tx['address']}")
        print(f"  Inner ins index: {tx.get('inner_ins_index', 'None')}")
    
    # Check if quote and base sides swap roles
    print("\n--- QUOTE/BASE PATTERNS ---")
    type_swap_patterns = defaultdict(int)
    for tx in transactions:
        pattern = f"quote_{tx['quote']['type_swap']}_base_{tx['base']['type_swap']}"
        type_swap_patterns[pattern] += 1
    
    print("Type swap patterns:")
    for pattern, count in sorted(type_swap_patterns.items()):
        print(f"  {pattern}: {count}")
    
    # Check change_amount patterns
    print("\n--- CHANGE AMOUNT PATTERNS ---")
    change_patterns = defaultdict(int)
    for tx in transactions:
        quote_sign = "pos" if tx['quote']['ui_change_amount'] > 0 else "neg"
        base_sign = "pos" if tx['base']['ui_change_amount'] > 0 else "neg"
        pattern = f"quote_{quote_sign}_base_{base_sign}"
        change_patterns[pattern] += 1
    
    print("Change amount patterns:")
    for pattern, count in sorted(change_patterns.items()):
        print(f"  {pattern}: {count}")

def main():
    # Analyze multiple wallet files
    transaction_files = [
        '/home/mrima/tytos/wallet-analyser/transactions_4GQeEya6ZTwvXre4Br6ZfDyfe2WQMkcDz2QbkJZazVqS.json',
        '/home/mrima/tytos/wallet-analyser/transactions_7dGrdJRYtsNR8UYxZ3TnifXGjGc9eRYLq9sELwYpuuUu.json',
        '/home/mrima/tytos/wallet-analyser/transactions_8Bu2Lmdu5KYKfJJ9nuAjnT5CUhDSCweyUwuTfXQrmDqs.json'
    ]
    
    for file_path in transaction_files:
        if os.path.exists(file_path):
            analyze_transaction_data(file_path)

if __name__ == "__main__":
    main()