import json
from collections import defaultdict
from datetime import datetime

LOG_FILE = "pnl_verification_log.txt"

def run_pnl_verification(filepath):
    """ 
    Loads transaction data, calculates PNL step-by-step, and creates a 
    detailed log file of the entire process.
    """
    try:
        with open(filepath, 'r') as f:
            raw_data = json.load(f)
            transactions = raw_data.get("data", {}).get("items", [])
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error loading or parsing {filepath}: {e}")
        return

    # 1. Consolidate and Standardize
    grouped_by_hash = defaultdict(list)
    for tx in transactions:
        if tx.get("tx_hash"):
            grouped_by_hash[tx["tx_hash"]].append(tx)

    processed_events = []
    for tx_hash, entries in grouped_by_hash.items():
        net_token_changes = defaultdict(float)
        total_volume_usd = 0
        for entry in entries:
            total_volume_usd += entry.get("volume_usd", 0)
            for leg in ['base', 'quote']:
                if leg in entry and isinstance(entry[leg], dict):
                    symbol = entry[leg].get("symbol")
                    change = entry[leg].get("ui_change_amount")
                    if symbol and isinstance(change, (int, float)):
                        net_token_changes[symbol] += change
        
        in_flows = {s: c for s, c in net_token_changes.items() if c > 1e-9}
        out_flows = {s: abs(c) for s, c in net_token_changes.items() if c < -1e-9}

        if len(in_flows) > 0 and len(out_flows) > 0:
            processed_events.append({
                "tx_hash": tx_hash,
                "timestamp": entries[0].get("block_unix_time"),
                "in_flows": in_flows,
                "out_flows": out_flows,
                "total_usd_value": total_volume_usd / len(entries) if entries else 0
            })

    # 2. Chronological Sort
    processed_events.sort(key=lambda x: x["timestamp"])

    # 3. FIFO PNL Calculation with Detailed Logging
    asset_lots = defaultdict(list)
    realized_pnl = 0

    with open(LOG_FILE, 'w') as log:
        log.write("--- PNL Verification Log ---\n")
        log.write(f"Report generated on: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
        log.write("\nInitial State: asset_lots = {}, realized_pnl = $0.00\n")

        for i, event in enumerate(processed_events):
            log.write("-" * 50 + "\n")
            log.write(f"Transaction {i+1}/{len(processed_events)} | Hash: {event['tx_hash']}\n")
            log.write(f"Timestamp: {datetime.fromtimestamp(event['timestamp']).strftime('%Y-%m-%d %H:%M:%S')}\n")
            log.write(f"Action: Swap {list(event['out_flows'].keys())} for {list(event['in_flows'].keys())}\n")
            log.write(f"USD Value: ${event['total_usd_value']:.2f}\n\n")

            proceeds_from_sale = event["total_usd_value"]
            
            # Handle disposals
            log.write("  Disposing Assets (Out-Flow):\n")
            cost_of_goods_sold = 0
            for symbol, amount_sold in event["out_flows"].items():
                log.write(f"    - Selling {amount_sold:.4f} {symbol}\n")
                lots = asset_lots[symbol]
                
                if not lots:
                    # Assume cost basis equals proceeds if no prior acquisition is found
                    cost_of_goods_sold += proceeds_from_sale
                    log.write(f"      - WARNING: No acquisition history for {symbol}. Assuming cost basis equals proceeds (${proceeds_from_sale:.2f}) for this portion. PNL = $0.\n")
                    continue

                amount_to_account_for = amount_sold
                while amount_to_account_for > 0 and lots:
                    if lots[0]['qty'] <= amount_to_account_for:
                        lot = lots.pop(0)
                        cost_of_goods_sold += lot['qty'] * lot['cost_per_unit']
                        amount_to_account_for -= lot['qty']
                        log.write(f"      - Used lot: {lot['qty']:.4f} {symbol} @ ${lot['cost_per_unit']:.4f}/unit. (Lot depleted)\n")
                    else:
                        cost_of_goods_sold += amount_to_account_for * lots[0]['cost_per_unit']
                        lots[0]['qty'] -= amount_to_account_for
                        log.write(f"      - Used partial lot: {amount_to_account_for:.4f} {symbol} @ ${lots[0]['cost_per_unit']:.4f}/unit.\n")
                        amount_to_account_for = 0

            pnl_for_event = proceeds_from_sale - cost_of_goods_sold
            realized_pnl += pnl_for_event
            log.write(f"    - PNL for this event: ${proceeds_from_sale:.2f} (Proceeds) - ${cost_of_goods_sold:.2f} (Cost) = ${pnl_for_event:.2f}\n")
            log.write(f"    - Running Realized PNL: ${realized_pnl:.2f}\n\n")

            # Handle acquisitions
            log.write("  Acquiring Assets (In-Flow):\n")
            cost_of_acquisition = event["total_usd_value"]
            for symbol, amount_acquired in event["in_flows"].items():
                if amount_acquired > 0:
                    cost_per_unit = cost_of_acquisition / amount_acquired
                    asset_lots[symbol].append({'qty': amount_acquired, 'cost_per_unit': cost_per_unit})
                    log.write(f"    - Acquired {amount_acquired:.4f} {symbol} at ${cost_per_unit:.4f}/unit. Added new lot.\n")
        
        log.write("=" * 50 + "\n")
        log.write("--- FINAL SUMMARY ---\n")
        log.write(f"Total Realized PNL after {len(processed_events)} transactions: ${realized_pnl:.2f}\n")
        log.write("\nFinal Holdings (Asset Lots):\n")
        for symbol, lots in asset_lots.items():
            total_qty = sum(l['qty'] for l in lots)
            if total_qty > 1e-9:
                log.write(f"  - {symbol}: {total_qty:.4f} units\n")

    print(f"Verification complete. Detailed log written to {LOG_FILE}")
    print(f"Final Realized PNL from verifier: ${realized_pnl:.2f}")

if __name__ == "__main__":
    run_pnl_verification("pnl_analysis_txs.json")
