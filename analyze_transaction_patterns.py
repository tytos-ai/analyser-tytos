#!/usr/bin/env python3
"""
Deep Transaction Structure Analysis for P&L Calculation
Analyzes BirdEye transaction data to understand all patterns and edge cases
"""

import json
import requests
from collections import defaultdict, Counter
from typing import Dict, List, Any, Set
import time

class TransactionPatternAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        
        # Pattern tracking
        self.patterns = {
            'swap_directions': Counter(),
            'type_combinations': Counter(),
            'source_patterns': Counter(),
            'token_pair_patterns': Counter(),
            'price_consistency': [],
            'volume_calculations': [],
            'edge_cases': [],
            'field_variations': defaultdict(set),
            'mathematical_checks': [],
        }
        
    def fetch_diverse_transactions(self) -> List[Dict]:
        """Fetch transactions from multiple wallets to get diverse patterns"""
        
        # Mix of different wallet types
        diverse_wallets = [
            "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",  # Very active trader
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",  # Different pattern
            "7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5",  # Another pattern
            "A6yAe6LF1taeEbNaiL1Kp4x2FYYtJhVFoNPmqPBmEMzs",  # Different volume
        ]
        
        all_transactions = []
        
        for wallet in diverse_wallets:
            print(f"Fetching from wallet {wallet[:8]}...")
            
            # Get recent transactions with different time ranges
            params = {
                "address": wallet,
                "limit": 50  # Get more samples per wallet
            }
            
            try:
                response = requests.get(self.base_url, headers=self.headers, params=params)
                if response.status_code == 200:
                    data = response.json()
                    if data.get("success") and data.get("data", {}).get("items"):
                        transactions = data["data"]["items"]
                        print(f"  Found {len(transactions)} transactions")
                        all_transactions.extend(transactions)
                    else:
                        print(f"  No transactions found")
                else:
                    print(f"  Error: {response.status_code}")
                    
            except Exception as e:
                print(f"  Error fetching {wallet}: {e}")
                
            time.sleep(0.3)  # Rate limiting
            
        print(f"\nTotal transactions collected: {len(all_transactions)}")
        return all_transactions
        
    def analyze_transaction_structure(self, tx: Dict) -> Dict[str, Any]:
        """Deep analysis of transaction structure for P&L insights"""
        
        analysis = {
            'tx_hash': tx.get('tx_hash'),
            'basic_info': {
                'source': tx.get('source'),
                'tx_type': tx.get('tx_type'),
                'block_time': tx.get('block_unix_time'),
                'volume_usd': tx.get('volume_usd'),
                'volume': tx.get('volume'),
            }
        }
        
        # Quote/Base analysis
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        analysis['quote'] = {
            'symbol': quote.get('symbol'),
            'address': quote.get('address'),
            'type': quote.get('type'),
            'type_swap': quote.get('type_swap'),
            'ui_change_amount': quote.get('ui_change_amount'),
            'price': quote.get('price'),
            'nearest_price': quote.get('nearest_price'),
        }
        
        analysis['base'] = {
            'symbol': base.get('symbol'),
            'address': base.get('address'),
            'type': base.get('type'),
            'type_swap': base.get('type_swap'),
            'ui_change_amount': base.get('ui_change_amount'),
            'price': base.get('price'),
            'nearest_price': base.get('nearest_price'),
        }
        
        # P&L Analysis patterns
        analysis['pnl_patterns'] = self.analyze_pnl_patterns(tx)
        
        # Mathematical verification
        analysis['math_check'] = self.verify_transaction_math(tx)
        
        # Edge case detection
        analysis['edge_cases'] = self.detect_edge_cases(tx)
        
        return analysis
        
    def analyze_pnl_patterns(self, tx: Dict) -> Dict[str, Any]:
        """Analyze patterns relevant to P&L calculation"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        # Determine swap direction based on change amounts
        quote_change = quote.get('ui_change_amount', 0)
        base_change = base.get('ui_change_amount', 0)
        
        patterns = {
            'swap_direction': None,
            'token_in': None,
            'token_out': None,
            'amount_in': 0,
            'amount_out': 0,
            'is_sol_involved': False,
            'is_token_to_token': False,
            'price_source': 'embedded',
        }
        
        # Determine direction
        if quote_change < 0 and base_change > 0:
            patterns['swap_direction'] = 'quote_to_base'
            patterns['token_in'] = quote.get('symbol')
            patterns['token_out'] = base.get('symbol')
            patterns['amount_in'] = abs(quote_change)
            patterns['amount_out'] = base_change
        elif quote_change > 0 and base_change < 0:
            patterns['swap_direction'] = 'base_to_quote'
            patterns['token_in'] = base.get('symbol')
            patterns['token_out'] = quote.get('symbol')
            patterns['amount_in'] = abs(base_change)
            patterns['amount_out'] = quote_change
        else:
            patterns['swap_direction'] = 'unclear'
            
        # SOL involvement
        sol_address = "So11111111111111111111111111111111111111112"
        patterns['is_sol_involved'] = (
            quote.get('address') == sol_address or 
            base.get('address') == sol_address
        )
        patterns['is_token_to_token'] = not patterns['is_sol_involved']
        
        return patterns
        
    def verify_transaction_math(self, tx: Dict) -> Dict[str, Any]:
        """Verify mathematical consistency of transaction data"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        verification = {
            'volume_usd_match': False,
            'price_consistency': False,
            'change_amount_signs': False,
            'calculations': {},
            'discrepancies': []
        }
        
        try:
            # Calculate USD values
            quote_usd = abs(quote.get('ui_change_amount', 0)) * quote.get('price', 0)
            base_usd = abs(base.get('ui_change_amount', 0)) * base.get('price', 0)
            reported_volume = tx.get('volume_usd', 0)
            
            verification['calculations'] = {
                'quote_usd': quote_usd,
                'base_usd': base_usd,
                'reported_volume_usd': reported_volume,
                'quote_base_diff': abs(quote_usd - base_usd),
                'volume_quote_diff': abs(reported_volume - quote_usd),
                'volume_base_diff': abs(reported_volume - base_usd),
            }
            
            # Check if USD values roughly match (within 1% tolerance)
            tolerance = 0.01
            if abs(quote_usd - base_usd) / max(quote_usd, base_usd, 0.01) < tolerance:
                verification['volume_usd_match'] = True
            else:
                verification['discrepancies'].append(f"Quote/Base USD mismatch: {quote_usd:.2f} vs {base_usd:.2f}")
                
            # Check price consistency
            quote_price = quote.get('price', 0)
            quote_nearest = quote.get('nearest_price', 0)
            base_price = base.get('price', 0)
            base_nearest = base.get('nearest_price', 0)
            
            if (abs(quote_price - quote_nearest) / max(quote_price, 0.01) < 0.05 and
                abs(base_price - base_nearest) / max(base_price, 0.01) < 0.05):
                verification['price_consistency'] = True
            else:
                verification['discrepancies'].append("Price/nearest_price inconsistency")
                
            # Check change amount signs
            quote_change = quote.get('ui_change_amount', 0)
            base_change = base.get('ui_change_amount', 0)
            
            if (quote_change * base_change) < 0:  # Should have opposite signs
                verification['change_amount_signs'] = True
            else:
                verification['discrepancies'].append("Change amounts have same sign")
                
        except Exception as e:
            verification['discrepancies'].append(f"Math verification error: {e}")
            
        return verification
        
    def detect_edge_cases(self, tx: Dict) -> List[str]:
        """Detect edge cases that might affect P&L calculation"""
        
        edge_cases = []
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        # Missing or zero prices
        if not quote.get('price') or quote.get('price') == 0:
            edge_cases.append("Zero/missing quote price")
        if not base.get('price') or base.get('price') == 0:
            edge_cases.append("Zero/missing base price")
            
        # Missing symbols
        if not quote.get('symbol') or quote.get('symbol') == 'UNKNOWN':
            edge_cases.append("Missing/unknown quote symbol")
        if not base.get('symbol') or base.get('symbol') == 'UNKNOWN':
            edge_cases.append("Missing/unknown base symbol")
            
        # Zero amounts
        if abs(quote.get('ui_change_amount', 0)) == 0:
            edge_cases.append("Zero quote change amount")
        if abs(base.get('ui_change_amount', 0)) == 0:
            edge_cases.append("Zero base change amount")
            
        # Unusual type combinations
        quote_type = quote.get('type', '')
        base_type = base.get('type', '')
        if quote_type != base_type:
            edge_cases.append(f"Mismatched types: {quote_type} vs {base_type}")
            
        # Same token swap (shouldn't happen)
        if quote.get('address') == base.get('address'):
            edge_cases.append("Same token swap detected")
            
        # Very large or small amounts
        quote_amount = abs(quote.get('ui_change_amount', 0))
        base_amount = abs(base.get('ui_change_amount', 0))
        
        if quote_amount > 1_000_000 or base_amount > 1_000_000:
            edge_cases.append("Very large amount")
        if quote_amount < 0.000001 or base_amount < 0.000001:
            edge_cases.append("Very small amount")
            
        return edge_cases
        
    def run_comprehensive_analysis(self):
        """Run comprehensive analysis and generate report"""
        
        print("🔍 Starting comprehensive transaction pattern analysis...")
        
        # Fetch diverse transaction data
        transactions = self.fetch_diverse_transactions()
        
        if not transactions:
            print("❌ No transactions found!")
            return
            
        print(f"\n📊 Analyzing {len(transactions)} transactions...")
        
        all_analyses = []
        
        for i, tx in enumerate(transactions):
            if i % 20 == 0:
                print(f"  Processing transaction {i+1}/{len(transactions)}")
                
            analysis = self.analyze_transaction_structure(tx)
            all_analyses.append(analysis)
            
            # Update pattern counters
            self.update_patterns(tx, analysis)
            
        # Generate comprehensive report
        self.generate_pattern_report(all_analyses)
        
        # Save detailed analysis
        with open('detailed_transaction_analysis.json', 'w') as f:
            json.dump(all_analyses, f, indent=2)
            
        print(f"\n✅ Analysis complete! Results saved to detailed_transaction_analysis.json")
        
    def update_patterns(self, tx: Dict, analysis: Dict):
        """Update pattern tracking from transaction analysis"""
        
        # Basic patterns
        pnl = analysis['pnl_patterns']
        self.patterns['swap_directions'][pnl['swap_direction']] += 1
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        type_combo = f"{quote.get('type')}|{base.get('type')}"
        self.patterns['type_combinations'][type_combo] += 1
        
        self.patterns['source_patterns'][tx.get('source', 'unknown')] += 1
        
        token_pair = f"{pnl['token_in']} → {pnl['token_out']}"
        self.patterns['token_pair_patterns'][token_pair] += 1
        
        # Mathematical checks
        math_check = analysis['math_check']
        self.patterns['mathematical_checks'].append({
            'tx_hash': tx.get('tx_hash'),
            'volume_match': math_check['volume_usd_match'],
            'price_consistent': math_check['price_consistency'],
            'signs_correct': math_check['change_amount_signs'],
            'discrepancies': math_check['discrepancies']
        })
        
        # Edge cases
        if analysis['edge_cases']:
            self.patterns['edge_cases'].extend(analysis['edge_cases'])
            
        # Field variations
        for field in ['type', 'type_swap']:
            if quote.get(field):
                self.patterns['field_variations'][f'quote_{field}'].add(quote[field])
            if base.get(field):
                self.patterns['field_variations'][f'base_{field}'].add(base[field])
                
    def generate_pattern_report(self, analyses: List[Dict]):
        """Generate comprehensive pattern analysis report"""
        
        print("\n" + "="*80)
        print("🎯 COMPREHENSIVE TRANSACTION PATTERN ANALYSIS")
        print("="*80)
        
        total_transactions = len(analyses)
        
        print(f"\n📊 Dataset Overview:")
        print(f"  Total Transactions Analyzed: {total_transactions}")
        
        # Swap Direction Patterns
        print(f"\n🔄 Swap Direction Patterns:")
        for direction, count in self.patterns['swap_directions'].most_common():
            pct = count / total_transactions * 100
            print(f"  {direction}: {count} ({pct:.1f}%)")
            
        # Type Combinations
        print(f"\n🏷️ Transaction Type Combinations:")
        for combo, count in self.patterns['type_combinations'].most_common():
            pct = count / total_transactions * 100
            print(f"  {combo}: {count} ({pct:.1f}%)")
            
        # DEX Sources
        print(f"\n🏪 DEX Source Distribution:")
        for source, count in self.patterns['source_patterns'].most_common():
            pct = count / total_transactions * 100
            print(f"  {source}: {count} ({pct:.1f}%)")
            
        # Mathematical Consistency
        math_checks = self.patterns['mathematical_checks']
        volume_matches = sum(1 for c in math_checks if c['volume_match'])
        price_consistent = sum(1 for c in math_checks if c['price_consistent'])
        signs_correct = sum(1 for c in math_checks if c['signs_correct'])
        
        print(f"\n🧮 Mathematical Consistency:")
        print(f"  Volume USD matches: {volume_matches}/{total_transactions} ({volume_matches/total_transactions*100:.1f}%)")
        print(f"  Price consistency: {price_consistent}/{total_transactions} ({price_consistent/total_transactions*100:.1f}%)")
        print(f"  Correct change signs: {signs_correct}/{total_transactions} ({signs_correct/total_transactions*100:.1f}%)")
        
        # Edge Cases
        edge_case_counter = Counter(self.patterns['edge_cases'])
        if edge_case_counter:
            print(f"\n⚠️ Edge Cases Found:")
            for case, count in edge_case_counter.most_common():
                print(f"  {case}: {count} occurrences")
        else:
            print(f"\n✅ No edge cases detected!")
            
        # Field Variations
        print(f"\n🔍 Field Variations:")
        for field, values in self.patterns['field_variations'].items():
            print(f"  {field}: {sorted(values)}")
            
        # Most Common Token Pairs
        print(f"\n💱 Most Common Token Pairs:")
        for pair, count in self.patterns['token_pair_patterns'].most_common(10):
            if count > 1:  # Only show pairs that appear multiple times
                print(f"  {pair}: {count} transactions")
                
        # P&L Calculation Recommendations
        print(f"\n💡 P&L Calculation Recommendations:")
        
        sol_involved = sum(1 for a in analyses if a['pnl_patterns']['is_sol_involved'])
        token_to_token = total_transactions - sol_involved
        
        print(f"  SOL-involved swaps: {sol_involved} ({sol_involved/total_transactions*100:.1f}%)")
        print(f"  Token-to-token swaps: {token_to_token} ({token_to_token/total_transactions*100:.1f}%)")
        
        if token_to_token > 0:
            print(f"  ✅ Dual event generation needed for {token_to_token} token-to-token swaps")
        
        # Show problematic transactions
        problematic = [c for c in math_checks if c['discrepancies']]
        if problematic:
            print(f"\n⚠️ Transactions with Mathematical Issues ({len(problematic)}):")
            for i, prob in enumerate(problematic[:5]):  # Show first 5
                print(f"  {i+1}. {prob['tx_hash'][:16]}... - {', '.join(prob['discrepancies'])}")
            if len(problematic) > 5:
                print(f"    ... and {len(problematic) - 5} more")
                

def main():
    analyzer = TransactionPatternAnalyzer()
    analyzer.run_comprehensive_analysis()

if __name__ == "__main__":
    main()