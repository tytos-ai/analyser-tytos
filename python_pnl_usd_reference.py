#!/usr/bin/env python3
"""
Updated Python P&L Reference Implementation with USD-ONLY approach
Matches the new Rust USD-only implementation for accurate comparison
"""

import json
import requests
from decimal import Decimal, getcontext
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, asdict
from datetime import datetime
import time

# Set decimal precision for financial calculations
getcontext().prec = 28

@dataclass
class FinancialEvent:
    event_type: str  # "BUY" or "SELL"
    token_mint: str
    token_symbol: str
    token_amount: Decimal
    usd_value: Decimal    # USD value (primary currency domain)
    sol_amount: Decimal   # SOL amount (reference only)
    price_per_token: Decimal
    timestamp: int
    tx_hash: str
    
@dataclass
class Position:
    token_mint: str
    token_symbol: str
    quantity: Decimal
    average_cost_usd: Decimal  # USD per token
    total_cost_usd: Decimal    # Total USD invested
    
@dataclass
class PnLCalculation:
    realized_pnl_usd: Decimal
    unrealized_pnl_usd: Decimal
    total_invested_usd: Decimal
    total_withdrawn_usd: Decimal
    current_positions: Dict[str, Position]
    total_events: int
    buy_events: int
    sell_events: int

class PythonUSDPnLCalculator:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        self.sol_address = "So11111111111111111111111111111111111111112"
        
    def fetch_transactions(self, wallet_address: str, limit: int = 100) -> List[Dict]:
        """Fetch transactions from BirdEye API"""
        
        params = {
            "address": wallet_address,
            "offset": 0,
            "limit": limit
        }
        
        print(f"🔍 Fetching {limit} transactions for wallet {wallet_address[:8]}...")
        
        response = requests.get(self.base_url, headers=self.headers, params=params)
        if response.status_code == 200:
            data = response.json()
            if data.get("success"):
                transactions = data.get("data", {}).get("items", [])
                print(f"✅ Successfully fetched {len(transactions)} transactions")
                return transactions
            else:
                print(f"❌ API returned success=false")
                return []
        else:
            print(f"❌ API request failed: {response.status_code}")
            return []
    
    def parse_transaction_to_events(self, tx: Dict) -> List[FinancialEvent]:
        """Parse BirdEye transaction to FinancialEvent(s) using USD-ONLY approach"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        # Determine direction using change_amount signs
        quote_change = Decimal(str(quote.get('ui_change_amount', 0)))
        base_change = Decimal(str(base.get('ui_change_amount', 0)))
        
        if quote_change < 0:
            # Quote was spent, base was received
            token_spent = {
                'symbol': quote.get('symbol'),
                'address': quote.get('address'),
                'amount': abs(quote_change),
                'price': Decimal(str(quote.get('price', 0)))
            }
            token_received = {
                'symbol': base.get('symbol'),
                'address': base.get('address'),
                'amount': base_change,
                'price': Decimal(str(base.get('price', 0)))
            }
        else:
            # Base was spent, quote was received
            token_spent = {
                'symbol': base.get('symbol'),
                'address': base.get('address'),
                'amount': abs(base_change),
                'price': Decimal(str(base.get('price', 0)))
            }
            token_received = {
                'symbol': quote.get('symbol'),
                'address': quote.get('address'),
                'amount': quote_change,
                'price': Decimal(str(quote.get('price', 0)))
            }
        
        events = []
        timestamp = tx.get('block_unix_time', 0)
        tx_hash = tx.get('tx_hash', '')
        
        # USD-ONLY APPROACH: Calculate USD values using embedded prices
        spent_usd = token_spent['amount'] * token_spent['price']
        received_usd = token_received['amount'] * token_received['price']
        
        # Determine event type(s) based on SOL involvement
        if token_received['address'] == self.sol_address:
            # Token → SOL (SELL event)
            events.append(FinancialEvent(
                event_type="SELL",
                token_mint=token_spent['address'],
                token_symbol=token_spent['symbol'],
                token_amount=token_spent['amount'],
                usd_value=spent_usd,  # USD value from embedded price
                sol_amount=token_received['amount'],  # Actual SOL received
                price_per_token=token_spent['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
        elif token_spent['address'] == self.sol_address:
            # SOL → Token (BUY event)
            events.append(FinancialEvent(
                event_type="BUY",
                token_mint=token_received['address'],
                token_symbol=token_received['symbol'],
                token_amount=token_received['amount'],
                usd_value=received_usd,  # USD value from embedded price
                sol_amount=token_spent['amount'],  # Actual SOL spent
                price_per_token=token_received['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
        else:
            # Token → Token (dual events) - USD-ONLY APPROACH
            # Both events use the SAME USD value for value conservation
            transaction_usd_value = spent_usd  # Use spent USD value for both events
            
            # SELL event for token spent
            events.append(FinancialEvent(
                event_type="SELL",
                token_mint=token_spent['address'],
                token_symbol=token_spent['symbol'],
                token_amount=token_spent['amount'],
                usd_value=transaction_usd_value,  # USD value (consistent)
                sol_amount=Decimal('0'),  # No SOL involved
                price_per_token=token_spent['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
            
            # BUY event for token received
            events.append(FinancialEvent(
                event_type="BUY",
                token_mint=token_received['address'],
                token_symbol=token_received['symbol'],
                token_amount=token_received['amount'],
                usd_value=transaction_usd_value,  # SAME USD value for conservation
                sol_amount=Decimal('0'),  # No SOL involved
                price_per_token=token_received['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
        
        return events
    
    def calculate_fifo_pnl(self, events: List[FinancialEvent]) -> PnLCalculation:
        """Calculate P&L using FIFO methodology with USD-only currency domain"""
        
        print(f"\n🧮 Calculating USD-ONLY FIFO P&L for {len(events)} events...")
        
        # Sort events by timestamp
        sorted_events = sorted(events, key=lambda e: e.timestamp)
        
        positions = {}  # token_mint -> List of Position entries
        realized_pnl_usd = Decimal('0')
        total_invested_usd = Decimal('0')
        total_withdrawn_usd = Decimal('0')
        buy_count = 0
        sell_count = 0
        
        for event in sorted_events:
            token_mint = event.token_mint
            
            if event.event_type == "BUY":
                buy_count += 1
                total_invested_usd += event.usd_value
                
                # Add to positions (FIFO queue)
                if token_mint not in positions:
                    positions[token_mint] = []
                
                cost_per_token_usd = event.usd_value / event.token_amount if event.token_amount > 0 else Decimal('0')
                
                positions[token_mint].append({
                    'quantity': event.token_amount,
                    'cost_per_token_usd': cost_per_token_usd,
                    'total_cost_usd': event.usd_value,
                    'timestamp': event.timestamp
                })
                
                print(f"  BUY: {event.token_amount} {event.token_symbol} for ${event.usd_value:.2f} @ ${cost_per_token_usd:.6f}/token")
                
            elif event.event_type == "SELL":
                sell_count += 1
                total_withdrawn_usd += event.usd_value
                
                if token_mint not in positions or not positions[token_mint]:
                    print(f"  ⚠️ SELL without position: {event.token_amount} {event.token_symbol}")
                    continue
                
                remaining_to_sell = event.token_amount
                sell_proceeds_usd = event.usd_value
                total_cost_basis_usd = Decimal('0')
                
                # FIFO: Sell from oldest positions first
                while remaining_to_sell > 0 and positions[token_mint]:
                    oldest_position = positions[token_mint][0]
                    
                    if oldest_position['quantity'] <= remaining_to_sell:
                        # Sell entire oldest position
                        sold_quantity = oldest_position['quantity']
                        cost_basis_usd = oldest_position['total_cost_usd']
                        
                        remaining_to_sell -= sold_quantity
                        total_cost_basis_usd += cost_basis_usd
                        
                        # Remove this position
                        positions[token_mint].pop(0)
                        
                    else:
                        # Partial sell of oldest position
                        sold_quantity = remaining_to_sell
                        proportional_cost_usd = (sold_quantity / oldest_position['quantity']) * oldest_position['total_cost_usd']
                        
                        total_cost_basis_usd += proportional_cost_usd
                        
                        # Update the position
                        oldest_position['quantity'] -= sold_quantity
                        oldest_position['total_cost_usd'] -= proportional_cost_usd
                        oldest_position['cost_per_token_usd'] = oldest_position['total_cost_usd'] / oldest_position['quantity'] if oldest_position['quantity'] > 0 else Decimal('0')
                        
                        remaining_to_sell = Decimal('0')
                
                # Calculate realized P&L for this sell (USD domain)
                pnl_usd = sell_proceeds_usd - total_cost_basis_usd
                realized_pnl_usd += pnl_usd
                
                print(f"  SELL: {event.token_amount} {event.token_symbol} for ${event.usd_value:.2f} (cost basis: ${total_cost_basis_usd:.2f}, P&L: ${pnl_usd:+.2f})")
        
        # Calculate current positions
        current_positions = {}
        unrealized_pnl_usd = Decimal('0')  # We don't have current prices, so set to 0
        
        for token_mint, position_list in positions.items():
            if position_list:
                total_quantity = sum(p['quantity'] for p in position_list)
                total_cost_usd = sum(p['total_cost_usd'] for p in position_list)
                avg_cost_usd = total_cost_usd / total_quantity if total_quantity > 0 else Decimal('0')
                
                # Get symbol from events
                symbol = next((e.token_symbol for e in events if e.token_mint == token_mint), "UNKNOWN")
                
                current_positions[token_mint] = Position(
                    token_mint=token_mint,
                    token_symbol=symbol,
                    quantity=total_quantity,
                    average_cost_usd=avg_cost_usd,
                    total_cost_usd=total_cost_usd
                )
        
        print(f"\n📊 USD-ONLY P&L Summary:")
        print(f"  Realized P&L: ${realized_pnl_usd:+.2f}")
        print(f"  Total Invested: ${total_invested_usd:.2f}")
        print(f"  Total Withdrawn: ${total_withdrawn_usd:.2f}")
        print(f"  Net Cash Flow: ${total_withdrawn_usd - total_invested_usd:+.2f}")
        print(f"  Buy Events: {buy_count}")
        print(f"  Sell Events: {sell_count}")
        print(f"  Active Positions: {len(current_positions)}")
        
        return PnLCalculation(
            realized_pnl_usd=realized_pnl_usd,
            unrealized_pnl_usd=unrealized_pnl_usd,
            total_invested_usd=total_invested_usd,
            total_withdrawn_usd=total_withdrawn_usd,
            current_positions=current_positions,
            total_events=len(events),
            buy_events=buy_count,
            sell_events=sell_count
        )
    
    def run_full_analysis(self, wallet_address: str) -> Dict[str, Any]:
        """Run complete analysis: fetch transactions, parse events, calculate P&L"""
        
        print(f"🚀 Starting Python USD-ONLY P&L Reference Analysis")
        print(f"Wallet: {wallet_address}")
        print("=" * 60)
        
        # Fetch transactions
        transactions = self.fetch_transactions(wallet_address, limit=100)
        
        if not transactions:
            print("❌ No transactions fetched, aborting analysis")
            return {}
        
        # Parse to events
        print(f"\n📋 Parsing {len(transactions)} transactions to events...")
        all_events = []
        
        for i, tx in enumerate(transactions):
            events = self.parse_transaction_to_events(tx)
            all_events.extend(events)
            
            if i < 5:  # Show first 5 for verification
                quote = tx.get('quote', {})
                base = tx.get('base', {})
                print(f"  Tx {i+1}: {quote.get('symbol')} ${quote.get('price', 0):.4f} ↔ {base.get('symbol')} ${base.get('price', 0):.4f} → {len(events)} event(s)")
        
        print(f"✅ Generated {len(all_events)} financial events from {len(transactions)} transactions")
        
        # Calculate P&L
        pnl_result = self.calculate_fifo_pnl(all_events)
        
        # Save results for comparison
        results = {
            'wallet_address': wallet_address,
            'transaction_count': len(transactions),
            'event_count': len(all_events),
            'pnl_calculation': asdict(pnl_result),
            'events': [asdict(event) for event in all_events],
            'raw_transactions': transactions
        }
        
        # Save to file
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"python_usd_pnl_reference_{timestamp}.json"
        
        # Convert Decimal to string for JSON serialization
        def decimal_to_str(obj):
            if isinstance(obj, dict):
                return {k: decimal_to_str(v) for k, v in obj.items()}
            elif isinstance(obj, list):
                return [decimal_to_str(item) for item in obj]
            elif isinstance(obj, Decimal):
                return str(obj)
            else:
                return obj
        
        json_safe_results = decimal_to_str(results)
        
        with open(filename, 'w') as f:
            json.dump(json_safe_results, f, indent=2)
        
        print(f"\n💾 Results saved to {filename}")
        
        # Print summary for easy comparison
        print(f"\n📋 PYTHON USD-ONLY REFERENCE RESULTS:")
        print("=" * 40)
        print(f"Wallet Address: {wallet_address}")
        print(f"Transactions Processed: {len(transactions)}")
        print(f"Financial Events Generated: {len(all_events)}")
        print(f"  - BUY Events: {pnl_result.buy_events}")
        print(f"  - SELL Events: {pnl_result.sell_events}")
        print(f"")
        print(f"P&L CALCULATION (USD-ONLY):")
        print(f"  Realized P&L: ${pnl_result.realized_pnl_usd:.2f}")
        print(f"  Total Invested: ${pnl_result.total_invested_usd:.2f}")
        print(f"  Total Withdrawn: ${pnl_result.total_withdrawn_usd:.2f}")
        print(f"  Active Positions: {len(pnl_result.current_positions)}")
        
        if pnl_result.current_positions:
            print(f"\nCURRENT POSITIONS:")
            for mint, position in pnl_result.current_positions.items():
                print(f"  {position.token_symbol}: {position.quantity} tokens @ ${position.average_cost_usd:.6f}/token")
        
        print(f"\n✅ Python USD-only reference analysis complete!")
        print(f"Ready to compare with Rust USD-only implementation.")
        
        return results

def main():
    calculator = PythonUSDPnLCalculator()
    
    # Use the same wallet we've been testing
    wallet_address = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    
    results = calculator.run_full_analysis(wallet_address)
    
    print(f"\n" + "="*60)
    print(f"NEXT STEPS:")
    print(f"1. Start the Rust API server: cargo run -p api_server")
    print(f"2. Submit batch job via test_usd_only_implementation.py")
    print(f"3. Compare USD-only results between Python and Rust")
    print(f"4. Verify mathematical consistency and accuracy")
    print(f"="*60)

if __name__ == "__main__":
    main()