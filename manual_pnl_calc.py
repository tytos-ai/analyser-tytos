import json
from decimal import Decimal, getcontext

# Set precision for Decimal calculations
getcontext().prec = 10

# Read the transaction data from the file
with open('/home/mrima/tytos/wallet-analyser/manual_verification_transactions.json', 'r') as f:
    data = json.load(f)

transactions = data['data']['items']

# Sort transactions by block_unix_time to ensure chronological order
transactions.sort(key=lambda x: x['block_unix_time'])

# FIFO queue for BNSOL holdings: list of (amount, cost_per_unit) tuples
bnsol_holdings = []
total_realized_pnl = Decimal('0')

# Track the last known price of BNSOL for unrealized PnL
last_bnsol_price = Decimal('0')

# Iterate through transactions
for tx in transactions:
    tx_hash = tx['tx_hash']
    block_time = tx['block_unix_time']
    owner = tx['owner']

    # Determine if it's a SOL -> BNSOL (buy) or BNSOL -> SOL (sell) swap
    # Assuming 'quote' is SOL and 'base' is BNSOL for these transactions based on sample
    # And assuming ui_change_amount is the net change for the owner's wallet

    sol_mint = "So11111111111111111111111111111111111111112"
    bnsol_mint = "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85"

    # Find the relevant token and its change for the owner
    # We need to be careful here as the 'owner' field is present, but the change_amount
    # is for the token itself, not necessarily from the owner's perspective in all cases.
    # However, for simple swaps, 'from' and 'to' in type_swap usually indicate direction.

    # Let's rely on the 'type_swap' and 'ui_change_amount' for the owner's perspective
    # If ui_change_amount for BNSOL is positive, it's a buy of BNSOL
    # If ui_change_amount for BNSOL is negative, it's a sell of BNSOL

    is_bnsol_buy = False
    is_bnsol_sell = False
    bnsol_amount = Decimal('0')
    sol_amount = Decimal('0')
    bnsol_price_at_tx = Decimal('0') # Price of BNSOL in USD at the time of transaction

    # Check base token (BNSOL)
    if tx['base']['address'] == bnsol_mint:
        bnsol_amount = Decimal(str(tx['base']['ui_amount']))
        bnsol_price_at_tx = Decimal(str(tx['base_price']))
        if tx['base']['type_swap'] == 'to': # BNSOL received
            is_bnsol_buy = True
        elif tx['base']['type_swap'] == 'from': # BNSOL sent
            is_bnsol_sell = True

    # Check quote token (SOL)
    if tx['quote']['address'] == sol_mint:
        sol_amount = Decimal(str(tx['quote']['ui_amount']))
        # The price of SOL is tx['quote_price']
        # We need to determine the SOL equivalent cost/revenue for BNSOL transactions
        # For SOL -> BNSOL, SOL is spent (negative ui_change_amount for SOL)
        # For BNSOL -> SOL, SOL is received (positive ui_change_amount for SOL)

    # Update last_bnsol_price
    if bnsol_price_at_tx > Decimal('0'):
        last_bnsol_price = bnsol_price_at_tx

    if is_bnsol_buy:
        # Cost of BNSOL is SOL_amount * SOL_price_at_tx
        # However, the Birdeye data directly gives us the BNSOL price in USD (base_price)
        # So, cost_per_unit for BNSOL is simply bnsol_price_at_tx
        cost_per_unit = bnsol_price_at_tx
        bnsol_holdings.append({'amount': bnsol_amount, 'cost_per_unit': cost_per_unit})
        # print(f"BUY: {bnsol_amount} BNSOL at ${cost_per_unit}/BNSOL. Holdings: {bnsol_holdings}")

    elif is_bnsol_sell:
        amount_to_sell = bnsol_amount # This is the ui_amount of BNSOL sent
        sale_price_per_unit = bnsol_price_at_tx # Price of BNSOL in USD at sale time

        # print(f"SELL: {amount_to_sell} BNSOL at ${sale_price_per_unit}/BNSOL. Current Holdings: {bnsol_holdings}")

        while amount_to_sell > Decimal('0') and bnsol_holdings:
            lot = bnsol_holdings[0]
            amount_in_lot = lot['amount']
            cost_per_unit_lot = lot['cost_per_unit']

            if amount_to_sell >= amount_in_lot:
                # Sell the entire lot
                realized_gain_loss = (sale_price_per_unit - cost_per_unit_lot) * amount_in_lot
                total_realized_pnl += realized_gain_loss
                amount_to_sell -= amount_in_lot
                bnsol_holdings.pop(0)
                # print(f"  Sold full lot of {amount_in_lot} at {sale_price_per_unit}. Realized: {realized_gain_loss}")
            else:
                # Sell part of the lot
                realized_gain_loss = (sale_price_per_unit - cost_per_unit_lot) * amount_to_sell
                total_realized_pnl += realized_gain_loss
                lot['amount'] -= amount_to_sell
                amount_to_sell = Decimal('0')
                # print(f"  Sold partial lot of {amount_to_sell} at {sale_price_per_unit}. Realized: {realized_gain_loss}. Remaining in lot: {lot['amount']}")
        # if amount_to_sell > Decimal('0'):
            # print(f"  WARNING: Not enough BNSOL in holdings to cover sell. Remaining to sell: {amount_to_sell}")

# Calculate unrealized PnL for remaining holdings
total_unrealized_pnl = Decimal('0')
if last_bnsol_price > Decimal('0'):
    for lot in bnsol_holdings:
        unrealized_gain_loss = (last_bnsol_price - lot['cost_per_unit']) * lot['amount']
        total_unrealized_pnl += unrealized_gain_loss

# Final results
total_pnl = total_realized_pnl + total_unrealized_pnl

print(f"Manual PnL Analysis Results:")
print(f"  Total Realized PnL: ${total_realized_pnl:,.2f}")
print(f"  Total Unrealized PnL: ${total_unrealized_pnl:,.2f} (based on last BNSOL price: ${last_bnsol_price:,.2f})")
print(f"  Total PnL (Realized + Unrealized): ${total_pnl:,.2f}")
