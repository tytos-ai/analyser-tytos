#!/usr/bin/env python3
"""
COMPREHENSIVE PNL DATA FIELD ANALYSIS
Understand each field in BirdEye data and how it should be used for accurate P&L calculation
"""

import json
from decimal import Decimal, getcontext
from collections import defaultdict
import statistics

getcontext().prec = 28

class PnLDataFieldAnalyzer:
    def __init__(self):
        self.sol_address = "So11111111111111111111111111111111111111112"

    def analyze_field_structure(self, transactions):
        """Analyze the structure and meaning of each field"""
        print(f"🔍 PNL DATA FIELD STRUCTURE ANALYSIS")
        print("=" * 80)
        
        sample_tx = transactions[0]
        
        print(f"📊 CORE TRANSACTION FIELDS:")
        print(f"  tx_hash: {sample_tx.get('tx_hash')[:16]}... (unique transaction identifier)")
        print(f"  block_unix_time: {sample_tx.get('block_unix_time')} (timestamp)")
        print(f"  tx_type: {sample_tx.get('tx_type')} (always 'swap' for our data)")
        print(f"  source: {sample_tx.get('source')} (DEX/AMM used)")
        print(f"  ins_index: {sample_tx.get('ins_index')} (instruction index within transaction)")
        print(f"  inner_ins_index: {sample_tx.get('inner_ins_index')} (inner instruction index)")
        
        print(f"\n📊 VOLUME FIELDS:")
        print(f"  volume_usd: ${sample_tx.get('volume_usd'):.2f} (USD value of the swap)")
        print(f"  volume: {sample_tx.get('volume')} (amount in base token units)")
        
        print(f"\n📊 QUOTE SIDE (Token A):")
        quote = sample_tx.get('quote', {})
        print(f"  symbol: {quote.get('symbol')}")
        print(f"  address: {quote.get('address')[:16]}...")
        print(f"  decimals: {quote.get('decimals')}")
        print(f"  amount: {quote.get('amount')} (raw amount)")
        print(f"  ui_amount: {quote.get('ui_amount')} (human-readable amount)")
        print(f"  change_amount: {quote.get('change_amount')} (raw change)")
        print(f"  ui_change_amount: {quote.get('ui_change_amount')} (human-readable change)")
        print(f"  price: ${quote.get('price'):.6f} (USD price per token)")
        print(f"  nearest_price: ${quote.get('nearest_price'):.6f} (nearest USD price)")
        print(f"  type: {quote.get('type')} (transfer type)")
        print(f"  type_swap: {quote.get('type_swap')} (swap direction)")
        
        print(f"\n📊 BASE SIDE (Token B):")
        base = sample_tx.get('base', {})
        print(f"  symbol: {base.get('symbol')}")
        print(f"  address: {base.get('address')[:16]}...")
        print(f"  decimals: {base.get('decimals')}")
        print(f"  amount: {base.get('amount')} (raw amount)")
        print(f"  ui_amount: {base.get('ui_amount')} (human-readable amount)")
        print(f"  change_amount: {base.get('change_amount')} (raw change)")
        print(f"  ui_change_amount: {base.get('ui_change_amount')} (human-readable change)")
        print(f"  price: ${base.get('price'):.6f} (USD price per token)")
        print(f"  nearest_price: ${base.get('nearest_price'):.6f} (nearest USD price)")
        print(f"  type: {base.get('type')} (transfer type)")
        print(f"  type_swap: {base.get('type_swap')} (swap direction)")

    def analyze_swap_directions(self, transactions):
        """Analyze how swap directions work"""
        print(f"\n🔍 SWAP DIRECTION ANALYSIS")
        print("=" * 80)
        
        direction_patterns = defaultdict(list)
        
        for tx in transactions[:20]:  # Analyze first 20
            quote = tx.get('quote', {})
            base = tx.get('base', {})
            
            quote_change = Decimal(str(quote.get('ui_change_amount', 0)))
            base_change = Decimal(str(base.get('ui_change_amount', 0)))
            quote_type_swap = quote.get('type_swap')
            base_type_swap = base.get('type_swap')
            
            pattern = f"quote:{quote_type_swap}({quote_change:+.2f}), base:{base_type_swap}({base_change:+.2f})"
            direction_patterns[pattern].append({
                'quote_sym': quote.get('symbol'),
                'base_sym': base.get('symbol'),
                'quote_change': quote_change,
                'base_change': base_change,
                'tx_hash': tx.get('tx_hash')[:16]
            })
        
        print(f"Swap direction patterns found:")
        for pattern, examples in direction_patterns.items():
            print(f"\n  {pattern}:")
            for ex in examples[:3]:  # Show first 3 examples
                print(f"    {ex['quote_sym']} {ex['quote_change']:+.2f} ↔ {ex['base_sym']} {ex['base_change']:+.2f} ({ex['tx_hash']}...)")

    def analyze_price_accuracy(self, transactions):
        """Analyze price accuracy and consistency"""
        print(f"\n🔍 PRICE ACCURACY ANALYSIS")
        print("=" * 80)
        
        price_mismatches = []
        value_conservation_errors = []
        
        for tx in transactions[:50]:  # Analyze first 50
            quote = tx.get('quote', {})
            base = tx.get('base', {})
            
            quote_change = abs(Decimal(str(quote.get('ui_change_amount', 0))))
            base_change = abs(Decimal(str(base.get('ui_change_amount', 0))))
            quote_price = Decimal(str(quote.get('price', 0)))
            base_price = Decimal(str(base.get('price', 0)))
            volume_usd = Decimal(str(tx.get('volume_usd', 0)))
            
            # Calculate USD values from amounts and prices
            quote_usd_value = quote_change * quote_price
            base_usd_value = base_change * base_price
            
            # Check value conservation
            if quote_usd_value > 0 and base_usd_value > 0:
                diff_pct = abs(quote_usd_value - base_usd_value) / max(quote_usd_value, base_usd_value) * 100
                
                if diff_pct > 2.0:  # More than 2% difference
                    value_conservation_errors.append({
                        'tx_hash': tx.get('tx_hash')[:16],
                        'quote_sym': quote.get('symbol'),
                        'base_sym': base.get('symbol'),
                        'quote_usd': quote_usd_value,
                        'base_usd': base_usd_value,
                        'diff_pct': diff_pct,
                        'volume_usd': volume_usd
                    })
            
            # Check if volume_usd matches calculated values
            expected_volume = max(quote_usd_value, base_usd_value)
            if expected_volume > 0:
                volume_diff_pct = abs(volume_usd - expected_volume) / expected_volume * 100
                if volume_diff_pct > 5.0:  # More than 5% difference
                    price_mismatches.append({
                        'tx_hash': tx.get('tx_hash')[:16],
                        'calculated_volume': expected_volume,
                        'reported_volume': volume_usd,
                        'diff_pct': volume_diff_pct
                    })
        
        print(f"Price accuracy results:")
        print(f"  Transactions analyzed: 50")
        print(f"  Value conservation errors (>2%): {len(value_conservation_errors)}")
        print(f"  Volume calculation mismatches (>5%): {len(price_mismatches)}")
        
        if value_conservation_errors:
            print(f"\n  ⚠️ Value conservation issues:")
            for err in value_conservation_errors[:3]:
                print(f"    {err['tx_hash']}: {err['quote_sym']} ${err['quote_usd']:.2f} vs {err['base_sym']} ${err['base_usd']:.2f} ({err['diff_pct']:.1f}% diff)")
        
        if price_mismatches:
            print(f"\n  ⚠️ Volume calculation issues:")
            for mm in price_mismatches[:3]:
                print(f"    {mm['tx_hash']}: calculated ${mm['calculated_volume']:.2f} vs reported ${mm['reported_volume']:.2f} ({mm['diff_pct']:.1f}% diff)")

    def analyze_multi_instruction_aggregation(self, transactions):
        """Analyze how to properly aggregate multi-instruction transactions"""
        print(f"\n🔍 MULTI-INSTRUCTION AGGREGATION ANALYSIS")
        print("=" * 80)
        
        # Load the duplicate analysis
        try:
            with open('duplicate_analysis_143_groups.json', 'r') as f:
                duplicate_data = json.load(f)
        except FileNotFoundError:
            print("❌ Duplicate analysis file not found. Run previous analysis first.")
            return
        
        print(f"Analyzing {len(duplicate_data)} duplicate transaction groups...")
        
        # Find complex multi-instruction transactions (more than 2 entries)
        complex_txs = {hash: data for hash, data in duplicate_data.items() if data['count'] > 2}
        
        print(f"Complex multi-instruction transactions: {len(complex_txs)}")
        
        for hash, data in list(complex_txs.items())[:3]:  # Analyze first 3
            print(f"\n  📊 TRANSACTION HASH: {hash[:32]}...")
            print(f"     Instructions: {data['count']}")
            
            transactions_data = data['transactions']
            
            # Calculate net effect
            net_quote_changes = defaultdict(Decimal)
            net_base_changes = defaultdict(Decimal)
            total_volume_usd = Decimal('0')
            
            print(f"     Instruction breakdown:")
            for i, tx in enumerate(transactions_data):
                quote = tx.get('quote', {})
                base = tx.get('base', {})
                
                quote_sym = quote.get('symbol')
                base_sym = base.get('symbol')
                quote_change = Decimal(str(quote.get('ui_change_amount', 0)))
                base_change = Decimal(str(base.get('ui_change_amount', 0)))
                volume_usd = Decimal(str(tx.get('volume_usd', 0)))
                
                print(f"       [{i+1}] {quote_sym} {quote_change:+.2f} ↔ {base_sym} {base_change:+.2f} (${volume_usd:.2f})")
                
                # Accumulate net changes
                net_quote_changes[quote_sym] += quote_change
                net_base_changes[base_sym] += base_change
                total_volume_usd += volume_usd
            
            print(f"     Net effects:")
            all_changes = {}
            for sym, change in net_quote_changes.items():
                all_changes[sym] = all_changes.get(sym, Decimal('0')) + change
            for sym, change in net_base_changes.items():
                all_changes[sym] = all_changes.get(sym, Decimal('0')) + change
            
            for sym, net_change in all_changes.items():
                if net_change != 0:
                    print(f"       NET: {sym} {net_change:+.2f}")
            
            print(f"     Total volume: ${total_volume_usd:.2f}")
            
            # Determine the correct P&L event(s) to generate
            self._determine_pnl_events(all_changes, total_volume_usd)

    def _determine_pnl_events(self, net_changes, total_volume_usd):
        """Determine what P&L events should be generated from net changes"""
        print(f"     🎯 P&L Event Generation Logic:")
        
        # Separate positive and negative changes
        increases = {sym: change for sym, change in net_changes.items() if change > 0}
        decreases = {sym: change for sym, change in net_changes.items() if change < 0}
        
        print(f"       Tokens received: {[(sym, f'{change:+.2f}') for sym, change in increases.items()]}")
        print(f"       Tokens spent: {[(sym, f'{change:+.2f}') for sym, change in decreases.items()]}")
        
        # Determine event type based on SOL involvement
        sol_involved = self.sol_address in net_changes
        sol_change = net_changes.get(self.sol_address, Decimal('0'))
        
        if sol_involved:
            if sol_change > 0:
                # SOL was received -> Token was sold
                sold_tokens = [sym for sym, change in decreases.items() if sym != self.sol_address]
                print(f"       → SELL event(s): {sold_tokens} for SOL")
                print(f"       → Event count: {len(sold_tokens)}")
            else:
                # SOL was spent -> Token was bought
                bought_tokens = [sym for sym, change in increases.items() if sym != self.sol_address]
                print(f"       → BUY event(s): {bought_tokens} with SOL")
                print(f"       → Event count: {len(bought_tokens)}")
        else:
            # No SOL involved -> Token-to-token swap
            print(f"       → Token-to-token swap: {len(decreases)} SELL + {len(increases)} BUY events")
            print(f"       → Event count: {len(decreases) + len(increases)}")

    def analyze_correct_pnl_calculation_approach(self, transactions):
        """Analyze the correct approach for P&L calculation"""
        print(f"\n🔍 CORRECT P&L CALCULATION APPROACH")
        print("=" * 80)
        
        print(f"📋 KEY PRINCIPLES FOR ACCURATE P&L:")
        print(f"")
        print(f"1. 🔧 TRANSACTION AGGREGATION:")
        print(f"   - Group by tx_hash (✅ current Rust does this)")
        print(f"   - Deduplicate identical entries (❌ current Rust missing)")
        print(f"   - Sum net effects across ALL instructions (❌ current Rust missing)")
        print(f"   - Calculate ONE set of events per transaction (❌ current Rust creates multiple)")
        print(f"")
        print(f"2. 💰 USD VALUE CALCULATION:")
        print(f"   - Use embedded BirdEye prices (✅ current Rust attempts this)")
        print(f"   - Apply prices to NET amounts, not individual instructions (❌ current Rust wrong)")
        print(f"   - Validate value conservation across transaction (❌ current Rust missing)")
        print(f"   - Use volume_usd as validation check (❌ current Rust missing)")
        print(f"")
        print(f"3. 🎯 EVENT GENERATION:")
        print(f"   - SOL involved → 1 event per net token change (❌ current Rust wrong)")
        print(f"   - No SOL → 2 events (SELL all spent + BUY all received) (❌ current Rust wrong)")
        print(f"   - Events should represent NET economic effect (❌ current Rust wrong)")
        print(f"")
        print(f"4. 🔍 VALIDATION:")
        print(f"   - Total USD in ≈ Total USD out per transaction (❌ current Rust missing)")
        print(f"   - Token conservation within transaction (❌ current Rust missing)")
        print(f"   - Cross-check with volume_usd field (❌ current Rust missing)")

    def provide_rust_implementation_roadmap(self):
        """Provide specific fixes needed in Rust implementation"""
        print(f"\n🔧 RUST IMPLEMENTATION FIXES NEEDED")
        print("=" * 80)
        
        print(f"📋 CRITICAL FIXES REQUIRED:")
        print(f"")
        print(f"1. 🏗️ FIX ProcessedSwap::aggregate_transaction_swaps_with_embedded_prices")
        print(f"   Current issues:")
        print(f"   - ❌ Only uses first transaction in group")
        print(f"   - ❌ Doesn't deduplicate identical entries")
        print(f"   - ❌ Doesn't sum ALL instruction effects")
        print(f"   ")
        print(f"   Required fixes:")
        print(f"   - ✅ Deduplicate identical transactions first")
        print(f"   - ✅ Sum ALL quote/base changes for same tx_hash")
        print(f"   - ✅ Calculate USD value from NET amounts")
        print(f"   - ✅ Validate value conservation")
        print(f"")
        print(f"2. 🎯 FIX Event Generation Logic")
        print(f"   Current issues:")
        print(f"   - ❌ Creates events based on individual instructions")
        print(f"   - ❌ Wrong USD values (uses individual amounts, not net)")
        print(f"   - ❌ Creates too many events per transaction")
        print(f"   ")
        print(f"   Required fixes:")
        print(f"   - ✅ Generate events based on NET transaction effect")
        print(f"   - ✅ Use proper USD values from aggregated amounts")
        print(f"   - ✅ Create correct number of events per transaction")
        print(f"")
        print(f"3. 🔍 ADD Validation Logic")
        print(f"   Missing features:")
        print(f"   - ✅ Add value conservation checks")
        print(f"   - ✅ Add duplicate detection and removal")
        print(f"   - ✅ Add transaction integrity validation")
        print(f"   - ✅ Add logging for debugging over-generation")
        print(f"")
        print(f"4. 🧮 FIX FIFO P&L Engine")
        print(f"   Current issues:")
        print(f"   - ❌ Receives wrong events (too many, wrong values)")
        print(f"   - ❌ USD-only approach corrupted by wrong inputs")
        print(f"   ")
        print(f"   Required fixes:")
        print(f"   - ✅ Will work correctly once events are fixed")
        print(f"   - ✅ May need additional validation")

    def run_complete_analysis(self, filename):
        """Run complete P&L data analysis"""
        print(f"🚀 COMPREHENSIVE PNL DATA FIELD ANALYSIS")
        print(f"📁 Data source: {filename}")
        print("=" * 80)
        
        with open(filename, 'r') as f:
            data = json.load(f)
        
        transactions = data['transactions']
        print(f"Total transactions: {len(transactions)}")
        
        # Run all analyses
        self.analyze_field_structure(transactions)
        self.analyze_swap_directions(transactions)
        self.analyze_price_accuracy(transactions)
        self.analyze_multi_instruction_aggregation(transactions)
        self.analyze_correct_pnl_calculation_approach(transactions)
        self.provide_rust_implementation_roadmap()

def main():
    analyzer = PnLDataFieldAnalyzer()
    filename = "deep_analysis_transactions_2000_20250704_091138.json"
    analyzer.run_complete_analysis(filename)

if __name__ == "__main__":
    main()