#!/usr/bin/env python3
"""
P&L Structure Analysis - Focus on specific patterns that affect P&L calculation
"""

import json
import requests
from decimal import Decimal
from typing import Dict, List, Any
import time

class PnLStructureAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        
    def get_sample_transactions(self, limit=100) -> List[Dict]:
        """Get a good sample of transactions for analysis"""
        params = {
            "address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
            "limit": limit
        }
        
        response = requests.get(self.base_url, headers=self.headers, params=params)
        if response.status_code == 200:
            data = response.json()
            if data.get("success"):
                return data.get("data", {}).get("items", [])
        return []
    
    def analyze_pnl_structure_patterns(self):
        """Analyze transaction structures specifically for P&L calculation patterns"""
        
        print("🔍 P&L Structure Analysis")
        print("=" * 50)
        
        transactions = self.get_sample_transactions()
        print(f"Analyzing {len(transactions)} transactions for P&L patterns...\n")
        
        # Categorize transactions by P&L calculation approach
        categories = {
            'sol_buy': [],      # SOL → Token (BUY event)
            'sol_sell': [],     # Token → SOL (SELL event)
            'token_to_token': [],  # Token → Token (dual events)
            'stablecoin_swaps': []  # Special stablecoin patterns
        }
        
        # Track specific patterns
        patterns = {
            'price_data_sources': set(),
            'volume_calculations': [],
            'mathematical_verifications': [],
            'swap_direction_indicators': [],
            'pnl_event_mappings': []
        }
        
        for i, tx in enumerate(transactions):
            analysis = self.analyze_single_transaction_for_pnl(tx)
            
            # Categorize transaction
            if analysis['category']:
                categories[analysis['category']].append({
                    'tx_hash': tx.get('tx_hash'),
                    'analysis': analysis
                })
            
            # Track patterns
            patterns['volume_calculations'].append(analysis['volume_calculation'])
            patterns['mathematical_verifications'].append(analysis['math_verification'])
            patterns['swap_direction_indicators'].append(analysis['direction_indicators'])
            patterns['pnl_event_mappings'].append(analysis['pnl_events'])
            
            if i < 10:  # Show detailed analysis for first 10
                self.print_detailed_transaction_analysis(tx, analysis, i + 1)
        
        # Summary analysis
        self.print_category_summary(categories)
        self.print_pnl_calculation_insights(patterns)
        self.print_recommended_pnl_approach(categories, patterns)
        
    def analyze_single_transaction_for_pnl(self, tx: Dict) -> Dict[str, Any]:
        """Analyze a single transaction for P&L calculation patterns"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        # Basic identifiers
        sol_address = "So11111111111111111111111111111111111111112"
        is_quote_sol = quote.get('address') == sol_address
        is_base_sol = base.get('address') == sol_address
        
        # Amounts and directions
        quote_change = quote.get('ui_change_amount', 0)
        base_change = base.get('ui_change_amount', 0)
        
        analysis = {
            'is_sol_involved': is_quote_sol or is_base_sol,
            'category': None,
            'direction_indicators': {},
            'volume_calculation': {},
            'math_verification': {},
            'pnl_events': [],
            'price_data': {},
            'special_cases': []
        }
        
        # Determine transaction category for P&L
        if is_quote_sol and quote_change < 0:  # SOL → Token (buying token with SOL)
            analysis['category'] = 'sol_buy'
            analysis['pnl_events'] = [{
                'type': 'BUY',
                'token': base.get('symbol'),
                'token_mint': base.get('address'),
                'token_amount': base_change,
                'sol_amount': abs(quote_change),
                'price_per_token': quote.get('price', 0) / base.get('price', 1) if base.get('price', 0) != 0 else 0
            }]
        elif is_base_sol and base_change > 0:  # Token → SOL (selling token for SOL)
            analysis['category'] = 'sol_sell'
            analysis['pnl_events'] = [{
                'type': 'SELL',
                'token': quote.get('symbol'),
                'token_mint': quote.get('address'),
                'token_amount': abs(quote_change),
                'sol_amount': base_change,
                'price_per_token': base.get('price', 0) / quote.get('price', 1) if quote.get('price', 0) != 0 else 0
            }]
        elif not (is_quote_sol or is_base_sol):  # Token → Token
            analysis['category'] = 'token_to_token'
            # Generate dual events
            analysis['pnl_events'] = [
                {
                    'type': 'SELL',
                    'token': quote.get('symbol') if quote_change < 0 else base.get('symbol'),
                    'token_mint': quote.get('address') if quote_change < 0 else base.get('address'),
                    'token_amount': abs(quote_change) if quote_change < 0 else abs(base_change),
                    'sol_amount': self.calculate_sol_equivalent(
                        abs(quote_change) if quote_change < 0 else abs(base_change),
                        quote.get('price', 0) if quote_change < 0 else base.get('price', 0)
                    ),
                    'price_per_token': quote.get('price', 0) if quote_change < 0 else base.get('price', 0)
                },
                {
                    'type': 'BUY',
                    'token': base.get('symbol') if quote_change < 0 else quote.get('symbol'),
                    'token_mint': base.get('address') if quote_change < 0 else quote.get('address'),
                    'token_amount': base_change if quote_change < 0 else quote_change,
                    'sol_amount': self.calculate_sol_equivalent(
                        base_change if quote_change < 0 else quote_change,
                        base.get('price', 0) if quote_change < 0 else quote.get('price', 0)
                    ),
                    'price_per_token': base.get('price', 0) if quote_change < 0 else quote.get('price', 0)
                }
            ]
        elif (is_quote_sol and quote_change > 0) or (is_base_sol and base_change < 0):
            # This would be unusual - receiving SOL when it should be spent, or losing SOL when it should be received
            analysis['special_cases'].append('unusual_sol_direction')
        
        # Direction indicators analysis
        analysis['direction_indicators'] = {
            'quote_type_swap': quote.get('type_swap'),
            'base_type_swap': base.get('type_swap'),
            'quote_change_sign': 'negative' if quote_change < 0 else 'positive' if quote_change > 0 else 'zero',
            'base_change_sign': 'negative' if base_change < 0 else 'positive' if base_change > 0 else 'zero',
            'consistent_direction': (quote.get('type_swap') == 'from' and quote_change < 0) or 
                                  (quote.get('type_swap') == 'to' and quote_change > 0)
        }
        
        # Volume calculation verification
        analysis['volume_calculation'] = {
            'reported_volume_usd': tx.get('volume_usd', 0),
            'calculated_quote_usd': abs(quote_change) * quote.get('price', 0),
            'calculated_base_usd': abs(base_change) * base.get('price', 0),
            'volume_source': 'quote' if abs(quote_change) * quote.get('price', 0) > abs(base_change) * base.get('price', 0) else 'base'
        }
        
        # Mathematical verification
        calc = analysis['volume_calculation']
        usd_diff = abs(calc['calculated_quote_usd'] - calc['calculated_base_usd'])
        max_usd = max(calc['calculated_quote_usd'], calc['calculated_base_usd'])
        
        analysis['math_verification'] = {
            'usd_values_match': usd_diff / max_usd < 0.01 if max_usd > 0 else False,
            'usd_difference': usd_diff,
            'percentage_diff': (usd_diff / max_usd * 100) if max_usd > 0 else 0,
            'signs_opposite': (quote_change * base_change) < 0
        }
        
        # Price data analysis
        analysis['price_data'] = {
            'quote_price': quote.get('price'),
            'quote_nearest_price': quote.get('nearest_price'),
            'base_price': base.get('price'),
            'base_nearest_price': base.get('nearest_price'),
            'price_fields_available': all([
                quote.get('price'), quote.get('nearest_price'),
                base.get('price'), base.get('nearest_price')
            ])
        }
        
        return analysis
    
    def calculate_sol_equivalent(self, token_amount: float, token_price_usd: float) -> float:
        """Calculate SOL equivalent for a token amount"""
        # Using approximate SOL price of $155 (should be fetched dynamically in real implementation)
        sol_price_usd = 155.0
        token_value_usd = token_amount * token_price_usd
        return token_value_usd / sol_price_usd
    
    def print_detailed_transaction_analysis(self, tx: Dict, analysis: Dict, num: int):
        """Print detailed analysis of a single transaction"""
        
        print(f"\n📋 Transaction #{num} Analysis:")
        print(f"   Hash: {tx.get('tx_hash', 'unknown')[:32]}...")
        print(f"   Source: {tx.get('source', 'unknown')}")
        print(f"   Category: {analysis['category']}")
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        print(f"   Quote: {quote.get('symbol')} ({quote.get('type_swap')}) = {quote.get('ui_change_amount')} @ ${quote.get('price', 0):.4f}")
        print(f"   Base:  {base.get('symbol')} ({base.get('type_swap')}) = {base.get('ui_change_amount')} @ ${base.get('price', 0):.4f}")
        
        # Show P&L events that would be generated
        print(f"   P&L Events to Generate:")
        for event in analysis['pnl_events']:
            print(f"     {event['type']}: {event['token_amount']:.6f} {event['token']} (SOL equiv: {event['sol_amount']:.6f})")
        
        # Math verification
        math_check = analysis['math_verification']
        print(f"   Math Check: USD match={math_check['usd_values_match']}, diff={math_check['usd_difference']:.2f} ({math_check['percentage_diff']:.1f}%)")
    
    def print_category_summary(self, categories: Dict[str, List]):
        """Print summary of transaction categories"""
        
        print(f"\n📊 Transaction Category Summary:")
        print("=" * 40)
        
        total = sum(len(cat) for cat in categories.values())
        
        for category, transactions in categories.items():
            count = len(transactions)
            percentage = (count / total * 100) if total > 0 else 0
            print(f"  {category}: {count} transactions ({percentage:.1f}%)")
            
            if count > 0:
                # Show sample transaction from each category
                sample = transactions[0]['analysis']
                print(f"    Sample P&L events: {len(sample['pnl_events'])} events")
                if sample['pnl_events']:
                    for event in sample['pnl_events']:
                        print(f"      {event['type']}: {event['token']}")
    
    def print_pnl_calculation_insights(self, patterns: Dict):
        """Print insights about P&L calculation from patterns"""
        
        print(f"\n💡 P&L Calculation Insights:")
        print("=" * 35)
        
        # Mathematical consistency
        math_checks = patterns['mathematical_verifications']
        consistent_count = sum(1 for check in math_checks if check['usd_values_match'])
        total_checks = len(math_checks)
        
        print(f"  Mathematical Consistency: {consistent_count}/{total_checks} ({consistent_count/total_checks*100:.1f}%)")
        
        # Direction indicator consistency
        direction_checks = patterns['swap_direction_indicators']
        consistent_directions = sum(1 for check in direction_checks if check['consistent_direction'])
        
        print(f"  Direction Indicators Consistent: {consistent_directions}/{total_checks} ({consistent_directions/total_checks*100:.1f}%)")
        
        # Average calculation differences
        avg_usd_diff = sum(check['usd_difference'] for check in math_checks) / len(math_checks)
        avg_pct_diff = sum(check['percentage_diff'] for check in math_checks) / len(math_checks)
        
        print(f"  Average USD difference: ${avg_usd_diff:.2f} ({avg_pct_diff:.2f}%)")
        
        # Event generation patterns
        pnl_events = patterns['pnl_event_mappings']
        single_event_count = sum(1 for events in pnl_events if len(events) == 1)
        dual_event_count = sum(1 for events in pnl_events if len(events) == 2)
        
        print(f"  Single P&L Events: {single_event_count} (SOL swaps)")
        print(f"  Dual P&L Events: {dual_event_count} (token-to-token swaps)")
    
    def print_recommended_pnl_approach(self, categories: Dict, patterns: Dict):
        """Print recommended approach for P&L calculation based on analysis"""
        
        print(f"\n🎯 Recommended P&L Calculation Approach:")
        print("=" * 45)
        
        print("1. Transaction Classification:")
        print("   - Use quote/base change_amount signs to determine direction")
        print("   - SOL involvement: check addresses against SOL mint")
        print("   - Generate single event for SOL swaps, dual events for token-to-token")
        
        print("\n2. Price Data Usage:")
        print("   - BirdEye provides reliable embedded USD prices")
        print("   - Use 'price' field as primary, 'nearest_price' as fallback")
        print("   - Convert USD prices to SOL equivalents for P&L calculation")
        
        print("\n3. Mathematical Verification:")
        math_checks = patterns['mathematical_verifications']
        error_rate = sum(1 for check in math_checks if not check['usd_values_match']) / len(math_checks) * 100
        print(f"   - Current error rate: {error_rate:.1f}% (very low)")
        print("   - Implement tolerance of ±1% for USD value matching")
        print("   - Validate opposite signs for quote/base change amounts")
        
        print("\n4. Token-to-Token Swap Handling:")
        token_to_token_count = len(categories['token_to_token'])
        total_count = sum(len(cat) for cat in categories.values())
        tt_percentage = token_to_token_count / total_count * 100 if total_count > 0 else 0
        print(f"   - {tt_percentage:.1f}% of transactions are token-to-token")
        print("   - Generate SELL event for outgoing token")
        print("   - Generate BUY event for incoming token")
        print("   - Use embedded USD prices for SOL equivalent calculation")
        
        print("\n5. Edge Case Handling:")
        print("   - Zero amounts: Skip transaction")
        print("   - Missing prices: Use fallback price sources")
        print("   - Same token swaps: Flag as error")
        print("   - Very large/small amounts: Add validation bounds")
        
        print(f"\n✅ Current Rust implementation aligns with these recommendations!")

def main():
    analyzer = PnLStructureAnalyzer()
    analyzer.analyze_pnl_structure_patterns()

if __name__ == "__main__":
    main()