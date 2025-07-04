
import json
import requests
from collections import defaultdict
from datetime import datetime

API_KEY = "5ff313b239ac42e297b830b10ea1871d"

def get_current_price(token_address):
    """Fetches the current price of a token from Birdeye."""
    url = f"https://public-api.birdeye.so/public/price?address={token_address}"
    headers = {"X-API-KEY": API_KEY}
    try:
        response = requests.get(url, headers=headers)
        response.raise_for_status()
        data = response.json()
        if data.get("success") and data.get("data", {}).get("value"):
            return data["data"]["value"]
    except requests.exceptions.RequestException as e:
        print(f"Warning: Could not fetch price for {token_address}: {e}")
    return None

def calculate_pnl(filepath):
    """Loads transaction data, calculates PNL, and prints a report."""
    try:
        with open(filepath, 'r') as f:
            raw_data = json.load(f)
            transactions = raw_data.get("data", {}).get("items", [])
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error loading or parsing {filepath}: {e}")
        return

    if not transactions:
        print("No transactions found in the file.")
        return

    # 1. Consolidate and Standardize
    grouped_by_hash = defaultdict(list)
    for tx in transactions:
        if tx.get("tx_hash"):
            grouped_by_hash[tx["tx_hash"]].append(tx)

    processed_events = []
    for tx_hash, entries in grouped_by_hash.items():
        net_token_changes = defaultdict(float)
        token_addresses = {}
        total_volume_usd = 0

        for entry in entries:
            total_volume_usd += entry.get("volume_usd", 0)
            for leg in ['base', 'quote']:
                if leg in entry and isinstance(entry[leg], dict):
                    symbol = entry[leg].get("symbol")
                    addr = entry[leg].get("address")
                    change = entry[leg].get("ui_change_amount")
                    if symbol and addr and isinstance(change, (int, float)):
                        net_token_changes[symbol] += change
                        token_addresses[symbol] = addr
        
        # Determine net in-flow and out-flow
        in_flows = {s: c for s, c in net_token_changes.items() if c > 1e-9}
        out_flows = {s: abs(c) for s, c in net_token_changes.items() if c < -1e-9}

        # We need a clear swap of one asset type for another to value it
        if len(in_flows) > 0 and len(out_flows) > 0:
            processed_events.append({
                "tx_hash": tx_hash,
                "timestamp": entries[0].get("block_unix_time"),
                "in_flows": in_flows,
                "out_flows": out_flows,
                "token_addresses": token_addresses,
                "total_usd_value": total_volume_usd / len(entries) # Average volume if multiple entries
            })

    # 2. Chronological Sort
    processed_events.sort(key=lambda x: x["timestamp"])

    # 3. FIFO PNL Calculation
    asset_lots = defaultdict(list) # {symbol: [{qty: float, cost_per_unit: float}]}
    realized_pnl = 0

    for event in processed_events:
        proceeds_from_sale = event["total_usd_value"]

        # Handle disposals (out-flows)
        for symbol, amount_sold in event["out_flows"].items():
            cost_of_goods_sold = 0
            lots = asset_lots[symbol]
            
            while amount_sold > 0 and lots:
                if lots[0]['qty'] <= amount_sold:
                    lot = lots.pop(0)
                    cost_of_goods_sold += lot['qty'] * lot['cost_per_unit']
                    amount_sold -= lot['qty']
                else:
                    cost_of_goods_sold += amount_sold * lots[0]['cost_per_unit']
                    lots[0]['qty'] -= amount_sold
                    amount_sold = 0
            
            if cost_of_goods_sold > 0:
                 realized_pnl += proceeds_from_sale - cost_of_goods_sold

        # Handle acquisitions (in-flows)
        cost_of_acquisition = event["total_usd_value"]
        for symbol, amount_acquired in event["in_flows"].items():
            if amount_acquired > 0:
                cost_per_unit = cost_of_acquisition / amount_acquired
                asset_lots[symbol].append({'qty': amount_acquired, 'cost_per_unit': cost_per_unit})

    # 4. Calculate Unrealized PNL
    unrealized_pnl = 0
    holdings_summary = []
    print("\nFetching current prices for unrealized PNL calculation...")
    for symbol, lots in asset_lots.items():
        total_qty = sum(lot['qty'] for lot in lots)
        if total_qty < 1e-9:
            continue

        total_cost_basis = sum(lot['qty'] * lot['cost_per_unit'] for lot in lots)
        avg_cost_basis = total_cost_basis / total_qty
        
        token_address = event["token_addresses"].get(symbol)
        current_price = get_current_price(token_address) if token_address else None

        if current_price is not None:
            market_value = total_qty * current_price
            pnl = market_value - total_cost_basis
            unrealized_pnl += pnl
            status = "PROFIT" if pnl > 0 else "LOSS"
            holdings_summary.append(f"  - {symbol}: {total_qty:.4f} units held. Avg Cost: ${avg_cost_basis:.4f}. Current Price: ${current_price:.4f}. Unrealized PNL: ${pnl:.2f} ({status})")
        else:
            holdings_summary.append(f"  - {symbol}: {total_qty:.4f} units held. Avg Cost: ${avg_cost_basis:.4f}. Current Price: UNKNOWN.")

    # 5. Final Report
    print("--- PNL Analysis Report ---")
    print(f"Report generated on: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
    print(f"Total Realized PNL: ${realized_pnl:.2f}")
    print("\nCurrent Holdings & Unrealized PNL:")
    if holdings_summary:
        for line in holdings_summary:
            print(line)
    else:
        print("  No assets currently held.")
    print(f"\nTotal Unrealized PNL: ${unrealized_pnl:.2f}")
    print("-" * 25)
    print(f"Total Net PNL (Realized + Unrealized): ${realized_pnl + unrealized_pnl:.2f}")
    print("--- End of Report ---")

if __name__ == "__main__":
    calculate_pnl("pnl_analysis_txs.json")
