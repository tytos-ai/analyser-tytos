

import json
import glob
from collections import defaultdict

def analyze_duplicate_transactions():
    """
    Analyzes duplicate transaction hashes by grouping all entries by hash
    and saving the grouped data to a file.
    """
    transaction_files = glob.glob("transactions_*.json")
    all_transactions = []
    for filename in transaction_files:
        with open(filename, 'r') as f:
            try:
                data = json.load(f)
                if "data" in data and "items" in data["data"]:
                    all_transactions.extend(data["data"]["items"])
            except json.JSONDecodeError:
                print(f"Warning: Could not decode JSON from {filename}")

    if not all_transactions:
        print("No transactions found to analyze.")
        return

    print(f"Loaded a total of {len(all_transactions)} transaction entries.")

    # Group transactions by tx_hash
    grouped_by_hash = defaultdict(list)
    for tx in all_transactions:
        if tx.get("tx_hash"):
            grouped_by_hash[tx["tx_hash"]].append(tx)

    # Filter for duplicates
    duplicates = {
        hash:
        transactions
        for hash, transactions in grouped_by_hash.items()
        if len(transactions) > 1
    }

    print(f"Found {len(duplicates)} transaction hashes with multiple entries.")

    if duplicates:
        # Save the full analysis to a file
        output_filename = f"duplicate_analysis_{len(duplicates)}_groups.json"
        with open(output_filename, 'w') as f:
            json.dump(duplicates, f, indent=4)
        print(f"Full analysis of duplicate transactions saved to {output_filename}")

        # Print details for a few examples
        print("\n--- Examples of Duplicate Transaction Hashes ---")
        for i, (hash_val, transactions) in enumerate(duplicates.items()):
            if i >= 3:
                break
            print(f"\n--- Hash: {hash_val} ({len(transactions)} entries) ---")
            for entry in transactions:
                # Print a summary of each entry
                base = entry.get('base', {})
                quote = entry.get('quote', {})
                print(f"  - Source: {entry.get('source')}")
                print(f"    Base: {base.get('ui_amount')} {base.get('symbol')} ({base.get('type_swap')})")
                print(f"    Quote: {quote.get('ui_amount')} {quote.get('symbol')} ({quote.get('type_swap')})")
                print(f"    Volume (USD): {entry.get('volume_usd')}")
                print("-" * 20)

if __name__ == "__main__":
    analyze_duplicate_transactions()

