
import json
from datetime import datetime

def get_transaction_time_period(filepath):
    """Reads a transaction file and prints the time period it covers."""
    try:
        with open(filepath, 'r') as f:
            data = json.load(f)
            transactions = data.get("data", {}).get("items", [])
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error loading or parsing {filepath}: {e}")
        return

    if not transactions:
        print("No transactions found in the file.")
        return

    timestamps = [tx.get("block_unix_time") for tx in transactions if tx.get("block_unix_time")]
    
    if not timestamps:
        print("No timestamps found in the transactions.")
        return

    start_time = min(timestamps)
    end_time = max(timestamps)

    start_date = datetime.fromtimestamp(start_time).strftime('%Y-%m-%d %H:%M:%S')
    end_date = datetime.fromtimestamp(end_time).strftime('%Y-%m-%d %H:%M:%S')

    print(f"The 100 transactions cover the following period:")
    print(f"  Start Date: {start_date}")
    print(f"  End Date:   {end_date}")

if __name__ == "__main__":
    get_transaction_time_period("pnl_analysis_txs.json")
