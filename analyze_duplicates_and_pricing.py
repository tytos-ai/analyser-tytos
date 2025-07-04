
import json
import glob
from collections import Counter

def analyze_duplicates_and_pricing():
    """
    Analyzes transaction data for duplicate transaction hashes and checks for 
    the presence of USD price information.
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

    total_transactions = len(all_transactions)
    print(f"Loaded a total of {total_transactions} transaction entries.")

    # 1. Analyze transaction hash uniqueness
    tx_hashes = [tx.get("tx_hash") for tx in all_transactions if tx.get("tx_hash")]
    unique_tx_hashes = set(tx_hashes)

    print(f"Found {len(unique_tx_hashes)} unique transaction hashes.")

    if len(tx_hashes) != len(unique_tx_hashes):
        print("\nFound duplicate transaction hashes. Investigating...")
        hash_counts = Counter(tx_hashes)
        duplicates = {hash: count for hash, count in hash_counts.items() if count > 1}
        
        print(f"There are {len(duplicates)} transaction hashes that are repeated.")
        
        # Show details for the first 3 duplicates found
        for i, (hash_val, count) in enumerate(duplicates.items()):
            if i >= 3:
                print("\nAnd more...")
                break
            print(f"\n--- Duplicate Hash: {hash_val} (repeated {count} times) ---")
            duplicate_txs = [tx for tx in all_transactions if tx.get("tx_hash") == hash_val]
            for tx_entry in duplicate_txs:
                print(json.dumps(tx_entry, indent=2))
    else:
        print("\nAll transaction hashes are unique.")

    # 2. Analyze token price information
    print("\n--- Checking for USD Price Information ---")
    if all_transactions:
        sample_tx = all_transactions[0]
        has_volume_usd = "volume_usd" in sample_tx
        has_base_price = "price" in sample_tx.get("base", {})
        has_quote_price = "price" in sample_tx.get("quote", {})

        print(f"Top-level 'volume_usd' key exists: {has_volume_usd}")
        print(f"'price' key exists in 'base' dictionary: {has_base_price}")
        print(f"'price' key exists in 'quote' dictionary: {has_quote_price}")

        if has_volume_usd and has_base_price and has_quote_price:
            print("\nIt appears that USD price information is embedded in the transaction data.")
            print(" - 'volume_usd' likely represents the total value of the swap in USD.")
            print(" - The 'price' key within the 'base' and 'quote' dictionaries likely represents the USD price per token at the time of the transaction.")
            
            print("\nSample price data from the first transaction:")
            print(f"  - Base Token ({sample_tx.get('base', {}).get('symbol')}): {sample_tx.get('base', {}).get('price')} USD")
            print(f"  - Quote Token ({sample_tx.get('quote', {}).get('symbol')}): {sample_tx.get('quote', {}).get('price')} USD")
            print(f"  - Total Volume: {sample_tx.get('volume_usd')} USD")

if __name__ == "__main__":
    analyze_duplicates_and_pricing()
