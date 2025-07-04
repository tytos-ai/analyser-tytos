import json
from collections import defaultdict

def consolidate_transactions():
    """
    Consolidates transaction entries sharing the same hash into a single record
    representing the net change of tokens for the trader.
    """
    try:
        with open("duplicate_analysis_531_groups.json", 'r') as f:
            grouped_transactions = json.load(f)
    except FileNotFoundError:
        print("Error: The file 'duplicate_analysis_531_groups.json' was not found.")
        print("Please run the 'analyze_duplicate_transactions.py' script first.")
        return

    print(f"Loaded {len(grouped_transactions)} transaction groups to consolidate.")

    net_summary = {}

    for tx_hash, entries in grouped_transactions.items():
        net_token_changes = defaultdict(float)
        # Use the first entry to get common data
        block_time = entries[0].get("block_unix_time")
        owner = entries[0].get("owner")
        sources = list(set(e.get("source") for e in entries))

        for entry in entries:
            # The 'base' and 'quote' structure contains the token changes
            for leg in ['base', 'quote']:
                if leg in entry and isinstance(entry[leg], dict):
                    symbol = entry[leg].get("symbol")
                    change = entry[leg].get("ui_change_amount")
                    if symbol and isinstance(change, (int, float)):
                        net_token_changes[symbol] += change
        
        # Filter out tokens with a net change of zero
        final_changes = {s: c for s, c in net_token_changes.items() if abs(c) > 1e-9} # Use tolerance for float comparison

        net_summary[tx_hash] = {
            "owner": owner,
            "block_unix_time": block_time,
            "sources": sources,
            "net_token_changes": final_changes,
            "original_entries_count": len(entries)
        }

    # Save the consolidated summary to a file
    with open("net_transaction_summary.json", 'w') as f:
        json.dump(net_summary, f, indent=4)
    
    print(f"Successfully consolidated transactions into net_transaction_summary.json")

    # Print a few examples for comparison
    print("\n--- Consolidation Examples ---")
    for i, tx_hash in enumerate(list(grouped_transactions.keys())[:3]):
        print(f"\n--- Original Transaction Group (Hash: {tx_hash}) ---")
        for entry in grouped_transactions[tx_hash]:
            base = entry.get('base', {})
            quote = entry.get('quote', {})
            print(f"  - Entry: {base.get('symbol')} {base.get('ui_change_amount')} | {quote.get('symbol')} {quote.get('ui_change_amount')}")
        
        print(f"\n--- Consolidated Net Effect ---")
        print(json.dumps(net_summary[tx_hash]['net_token_changes'], indent=2))
        print("-" * 40)

if __name__ == "__main__":
    consolidate_transactions()
