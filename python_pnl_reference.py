#!/usr/bin/env python3
"""
Python P&L Reference Implementation
Fetch 100 transactions, parse to events, calculate FIFO P&L for comparison with Rust implementation
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
    sol_amount: Decimal
    price_per_token: Decimal
    timestamp: int
    tx_hash: str
    
@dataclass
class Position:
    token_mint: str
    token_symbol: str
    quantity: Decimal
    average_cost: Decimal  # SOL per token
    total_cost: Decimal    # Total SOL invested
    
@dataclass
class PnLCalculation:
    realized_pnl: Decimal
    unrealized_pnl: Decimal
    total_invested: Decimal
    total_withdrawn: Decimal
    current_positions: Dict[str, Position]
    total_events: int
    buy_events: int
    sell_events: int

class PythonPnLCalculator:
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
        """Parse BirdEye transaction to FinancialEvent(s)"""
        
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
        
        # Determine event type(s) based on SOL involvement
        if token_received['address'] == self.sol_address:
            # Token → SOL (SELL event)
            events.append(FinancialEvent(
                event_type="SELL",
                token_mint=token_spent['address'],
                token_symbol=token_spent['symbol'],
                token_amount=token_spent['amount'],
                sol_amount=token_received['amount'],
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
                sol_amount=token_spent['amount'],
                price_per_token=token_received['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
        else:
            # Token → Token (dual events)
            sol_price = Decimal('155.0')  # Should match Rust implementation fallback
            
            # SELL event for token spent
            spent_usd = token_spent['amount'] * token_spent['price']
            spent_sol_equiv = spent_usd / sol_price
            
            events.append(FinancialEvent(
                event_type="SELL",
                token_mint=token_spent['address'],
                token_symbol=token_spent['symbol'],
                token_amount=token_spent['amount'],
                sol_amount=spent_sol_equiv,
                price_per_token=token_spent['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
            
            # BUY event for token received
            received_usd = token_received['amount'] * token_received['price']
            received_sol_equiv = received_usd / sol_price
            
            events.append(FinancialEvent(
                event_type="BUY",
                token_mint=token_received['address'],
                token_symbol=token_received['symbol'],
                token_amount=token_received['amount'],
                sol_amount=received_sol_equiv,
                price_per_token=token_received['price'],
                timestamp=timestamp,
                tx_hash=tx_hash
            ))
        
        return events
    
    def calculate_fifo_pnl(self, events: List[FinancialEvent]) -> PnLCalculation:
        """Calculate P&L using FIFO methodology"""
        
        print(f"\n🧮 Calculating FIFO P&L for {len(events)} events...")
        
        # Sort events by timestamp
        sorted_events = sorted(events, key=lambda e: e.timestamp)
        
        positions = {}  # token_mint -> List of Position entries
        realized_pnl = Decimal('0')
        total_invested = Decimal('0')
        total_withdrawn = Decimal('0')
        buy_count = 0
        sell_count = 0
        
        for event in sorted_events:
            token_mint = event.token_mint
            
            if event.event_type == "BUY":
                buy_count += 1
                total_invested += event.sol_amount
                
                # Add to positions (FIFO queue)
                if token_mint not in positions:
                    positions[token_mint] = []
                
                positions[token_mint].append({
                    'quantity': event.token_amount,
                    'cost_per_token': event.sol_amount / event.token_amount if event.token_amount > 0 else Decimal('0'),
                    'total_cost': event.sol_amount,
                    'timestamp': event.timestamp
                })
                
                print(f"  BUY: {event.token_amount} {event.token_symbol} for {event.sol_amount} SOL @ {event.sol_amount/event.token_amount if event.token_amount > 0 else 0:.6f} SOL/token")
                
            elif event.event_type == "SELL":
                sell_count += 1
                total_withdrawn += event.sol_amount
                
                if token_mint not in positions or not positions[token_mint]:
                    print(f"  ⚠️ SELL without position: {event.token_amount} {event.token_symbol}")
                    continue
                
                remaining_to_sell = event.token_amount
                sell_proceeds = event.sol_amount
                total_cost_basis = Decimal('0')
                
                # FIFO: Sell from oldest positions first
                while remaining_to_sell > 0 and positions[token_mint]:
                    oldest_position = positions[token_mint][0]
                    
                    if oldest_position['quantity'] <= remaining_to_sell:
                        # Sell entire oldest position
                        sold_quantity = oldest_position['quantity']
                        cost_basis = oldest_position['total_cost']
                        
                        remaining_to_sell -= sold_quantity
                        total_cost_basis += cost_basis
                        
                        # Remove this position
                        positions[token_mint].pop(0)
                        
                    else:
                        # Partial sell of oldest position
                        sold_quantity = remaining_to_sell
                        proportional_cost = (sold_quantity / oldest_position['quantity']) * oldest_position['total_cost']
                        
                        total_cost_basis += proportional_cost
                        
                        # Update the position
                        oldest_position['quantity'] -= sold_quantity
                        oldest_position['total_cost'] -= proportional_cost
                        
                        remaining_to_sell = Decimal('0')
                
                # Calculate realized P&L for this sell
                pnl = sell_proceeds - total_cost_basis
                realized_pnl += pnl
                
                print(f"  SELL: {event.token_amount} {event.token_symbol} for {event.sol_amount} SOL (cost basis: {total_cost_basis} SOL, P&L: {pnl:+.6f} SOL)")
        
        # Calculate current positions
        current_positions = {}
        unrealized_pnl = Decimal('0')  # We don't have current prices, so set to 0
        
        for token_mint, position_list in positions.items():
            if position_list:
                total_quantity = sum(p['quantity'] for p in position_list)
                total_cost = sum(p['total_cost'] for p in position_list)
                avg_cost = total_cost / total_quantity if total_quantity > 0 else Decimal('0')
                
                # Get symbol from events
                symbol = next((e.token_symbol for e in events if e.token_mint == token_mint), "UNKNOWN")
                
                current_positions[token_mint] = Position(
                    token_mint=token_mint,
                    token_symbol=symbol,
                    quantity=total_quantity,
                    average_cost=avg_cost,
                    total_cost=total_cost
                )
        
        print(f"\n📊 P&L Summary:")
        print(f"  Realized P&L: {realized_pnl:+.6f} SOL")
        print(f"  Total Invested: {total_invested:.6f} SOL")
        print(f"  Total Withdrawn: {total_withdrawn:.6f} SOL")
        print(f"  Net Cash Flow: {total_withdrawn - total_invested:+.6f} SOL")
        print(f"  Buy Events: {buy_count}")
        print(f"  Sell Events: {sell_count}")
        print(f"  Active Positions: {len(current_positions)}")
        
        return PnLCalculation(
            realized_pnl=realized_pnl,
            unrealized_pnl=unrealized_pnl,
            total_invested=total_invested,
            total_withdrawn=total_withdrawn,
            current_positions=current_positions,
            total_events=len(events),
            buy_events=buy_count,
            sell_events=sell_count
        )
    
    def run_full_analysis(self, wallet_address: str) -> Dict[str, Any]:
        """Run complete analysis: fetch transactions, parse events, calculate P&L"""
        
        print(f"🚀 Starting Python P&L Reference Analysis")
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
                print(f"  Tx {i+1}: {quote.get('symbol')} {quote.get('ui_change_amount')} ↔ {base.get('symbol')} {base.get('ui_change_amount')} → {len(events)} event(s)")
        
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
        filename = f"python_pnl_reference_{timestamp}.json"
        
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
        print(f"\n📋 PYTHON REFERENCE RESULTS:")
        print("=" * 40)
        print(f"Wallet Address: {wallet_address}")
        print(f"Transactions Processed: {len(transactions)}")
        print(f"Financial Events Generated: {len(all_events)}")
        print(f"  - BUY Events: {pnl_result.buy_events}")
        print(f"  - SELL Events: {pnl_result.sell_events}")
        print(f"")
        print(f"P&L CALCULATION:")
        print(f"  Realized P&L: {pnl_result.realized_pnl} SOL")
        print(f"  Total Invested: {pnl_result.total_invested} SOL")
        print(f"  Total Withdrawn: {pnl_result.total_withdrawn} SOL")
        print(f"  Active Positions: {len(pnl_result.current_positions)}")
        
        if pnl_result.current_positions:
            print(f"\nCURRENT POSITIONS:")
            for mint, position in pnl_result.current_positions.items():
                print(f"  {position.token_symbol}: {position.quantity} tokens @ {position.average_cost:.6f} SOL/token")
        
        print(f"\n✅ Python reference analysis complete!")
        print(f"Now run the Rust API server with the same wallet address to compare results.")
        
        return results

def main():
    calculator = PythonPnLCalculator()
    
    # Use the same wallet we've been testing
    wallet_address = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"
    
    results = calculator.run_full_analysis(wallet_address)
    
    print(f"\n" + "="*60)
    print(f"NEXT STEPS:")
    print(f"1. Start the Rust API server: cargo run -p api_server")
    print(f"2. Make API request to: POST /api/pnl/batch/run")
    print(f"3. Use wallet address: {wallet_address}")
    print(f"4. Use same 100 transactions (offset=0, limit=100)")
    print(f"5. Compare the P&L results between Python and Rust")
    print(f"="*60)

if __name__ == "__main__":
    main()