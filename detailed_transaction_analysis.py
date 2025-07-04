import json
import glob
from collections import defaultdict

def analyze_transactions_in_detail():
    """Performs a detailed analysis of the transaction data."""
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

    print(f"Analyzing a total of {len(all_transactions)} transactions...\n")

    # 1. Analyze the distribution of tx_type
    tx_type_distribution = defaultdict(int)
    for tx in all_transactions:
        tx_type_distribution[tx.get("tx_type")] += 1

    print("Transaction Type Distribution:")
    for tx_type, count in tx_type_distribution.items():
        print(f"  - {tx_type}: {count}")

    # 2. Examine the base and quote dictionaries
    if all_transactions:
        first_tx = all_transactions[0]
        if "base" in first_tx:
            print("\n'base' dictionary keys and data types:")
            for key, value in first_tx["base"].items():
                print(f"  - {key}: {type(value).__name__}")
        if "quote" in first_tx:
            print("\n'quote' dictionary keys and data types:")
            for key, value in first_tx["quote"].items():
                print(f"  - {key}: {type(value).__name__}")

    # 3. Check for data consistency (presence of all keys in all transactions)
    if all_transactions:
        all_keys = set(all_transactions[0].keys())
        inconsistent_keys = set()
        for tx in all_transactions[1:]:
            inconsistent_keys.update(all_keys.symmetric_difference(tx.keys()))

        if inconsistent_keys:
            print(f"\nFound inconsistent keys across transactions: {inconsistent_keys}")
        else:
            print("\nAll transactions have a consistent set of keys.")

if __name__ == "__main__":
    analyze_transactions_in_detail()
