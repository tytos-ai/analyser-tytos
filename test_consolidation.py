#!/usr/bin/env python3

import json
import os
from collections import defaultdict

def test_consolidation_logic():
    """Test our consolidation logic against real transaction data"""
    
    # Load real transaction data
    test_files = [
        'transactions_4GQeEya6ZTwvXre4Br6ZfDyfe2WQMkcDz2QbkJZazVqS.json',
        'transactions_7dGrdJRYtsNR8UYxZ3TnifXGjGc9eRYLq9sELwYpuuUu.json', 
        'transactions_8Bu2Lmdu5KYKfJJ9nuAjnT5CUhDSCweyUwuTfXQrmDqs.json'
    ]
    
    for file_path in test_files:
        if not os.path.exists(file_path):
            print(f"❌ Missing test file: {file_path}")
            continue
            
        with open(file_path, 'r') as f:
            data = json.load(f)
        
        wallet_address = os.path.basename(file_path).replace('transactions_', '').replace('.json', '')
        transactions = data['data']['items']
        
        print(f"\n=== Testing Wallet: {wallet_address} ===")
        print(f"Raw transactions: {len(transactions)}")
        
        # Simulate our consolidation logic
        consolidated_map = {}
        
        for tx in transactions:
            tx_hash = tx['tx_hash']
            
            if tx_hash not in consolidated_map:
                consolidated_map[tx_hash] = {
                    'tx_hash': tx_hash,
                    'block_unix_time': tx['block_unix_time'],
                    'net_token_changes': {},
                    'total_volume_usd': 0.0,
                    'source': tx['source'],
                    'wallet_address': wallet_address
                }
            
            # Add volume
            consolidated_map[tx_hash]['total_volume_usd'] += tx['volume_usd']
            
            # Process quote side
            quote = tx['quote']
            quote_address = quote['address']
            if quote_address not in consolidated_map[tx_hash]['net_token_changes']:
                consolidated_map[tx_hash]['net_token_changes'][quote_address] = {
                    'symbol': quote['symbol'],
                    'address': quote_address,
                    'net_ui_amount': 0.0,
                    'usd_value': 0.0,
                    'price_per_token': quote.get('price', 0.0)
                }
            
            net_change = consolidated_map[tx_hash]['net_token_changes'][quote_address]
            net_change['net_ui_amount'] += quote['ui_change_amount']
            net_change['usd_value'] += quote['ui_change_amount'] * quote.get('price', 0.0)
            
            # Process base side
            base = tx['base']
            base_address = base['address']
            if base_address not in consolidated_map[tx_hash]['net_token_changes']:
                consolidated_map[tx_hash]['net_token_changes'][base_address] = {
                    'symbol': base['symbol'],
                    'address': base_address,
                    'net_ui_amount': 0.0,
                    'usd_value': 0.0,
                    'price_per_token': base.get('price', 0.0)
                }
            
            net_change = consolidated_map[tx_hash]['net_token_changes'][base_address]
            net_change['net_ui_amount'] += base['ui_change_amount']
            net_change['usd_value'] += base['ui_change_amount'] * base.get('price', 0.0)
        
        # Analyze results
        consolidated_transactions = list(consolidated_map.values())
        print(f"Consolidated transactions: {len(consolidated_transactions)}")
        
        # Count financial events that would be created
        financial_events = 0
        for consolidated_tx in consolidated_transactions:
            for token_address, token_change in consolidated_tx['net_token_changes'].items():
                if abs(token_change['net_ui_amount']) > 1e-9:  # Skip zero changes
                    financial_events += 1
        
        print(f"Financial events: {financial_events}")
        print(f"Reduction ratio: {len(transactions)} -> {financial_events} ({financial_events/len(transactions)*100:.1f}%)")
        
        # Show example of consolidation
        multi_record_tx = [tx for tx in consolidated_transactions if len(tx['net_token_changes']) > 1]
        if multi_record_tx:
            example = multi_record_tx[0]
            print(f"\nExample consolidated transaction {example['tx_hash'][:8]}...:")
            for token_addr, change in example['net_token_changes'].items():
                if abs(change['net_ui_amount']) > 1e-9:
                    action = "BUY" if change['net_ui_amount'] > 0 else "SELL"
                    print(f"  {action} {change['net_ui_amount']:+.6f} {change['symbol']} (${change['usd_value']:+.2f})")

if __name__ == "__main__":
    test_consolidation_logic()