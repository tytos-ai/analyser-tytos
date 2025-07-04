#!/usr/bin/env python3
"""
Data Structure Analysis - Determine parsing logic from actual data patterns
"""

import json
import requests
from collections import defaultdict
from typing import Dict, List, Any

class DataStructureAnalyzer:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        
    def get_sample_data(self, limit=200) -> List[Dict]:
        """Get comprehensive sample data"""
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
    
    def analyze_data_structure_patterns(self):
        """Analyze data to understand structure patterns"""
        
        print("🔍 Data Structure Pattern Analysis")
        print("=" * 50)
        
        transactions = self.get_sample_data()
        print(f"Analyzing {len(transactions)} transactions...\n")
        
        # Pattern tracking
        patterns = {
            'direction_rules': defaultdict(list),
            'quote_base_relationships': defaultdict(list), 
            'sol_involvement_patterns': defaultdict(list),
            'amount_sign_patterns': defaultdict(list),
            'type_swap_patterns': defaultdict(list),
            'volume_relationship_patterns': []
        }
        
        sol_address = "So11111111111111111111111111111111111111112"
        
        for i, tx in enumerate(transactions):
            if i >= 30:  # Analyze first 30 in detail
                break
                
            quote = tx.get('quote', {})
            base = tx.get('base', {})
            
            # Detailed analysis of each transaction
            analysis = self.analyze_single_transaction(tx, sol_address)
            
            # Store the complete analysis
            patterns['direction_rules'].append(analysis)
            
            # Print detailed breakdown for first 10
            if i < 10:
                self.print_transaction_breakdown(tx, analysis, i+1)
        
        # Analyze discovered patterns
        self.analyze_discovered_patterns(patterns)
        
    def analyze_single_transaction(self, tx: Dict, sol_address: str) -> Dict:
        """Analyze a single transaction to understand its structure"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        analysis = {
            'tx_hash': tx.get('tx_hash', '')[:16],
            'quote_info': {
                'symbol': quote.get('symbol'),
                'address': quote.get('address'),
                'change_amount': quote.get('ui_change_amount'),
                'type_swap': quote.get('type_swap'),
                'is_sol': quote.get('address') == sol_address
            },
            'base_info': {
                'symbol': base.get('symbol'),
                'address': base.get('address'),
                'change_amount': base.get('ui_change_amount'),
                'type_swap': base.get('type_swap'),
                'is_sol': base.get('address') == sol_address
            },
            'direction_analysis': self.analyze_direction_pattern(quote, base),
            'sol_pattern': self.analyze_sol_pattern(quote, base, sol_address),
            'math_verification': self.verify_transaction_math(quote, base, tx.get('volume_usd', 0))
        }
        
        return analysis
    
    def analyze_direction_pattern(self, quote: Dict, base: Dict) -> Dict:
        """Analyze how direction is determined from the data"""
        
        quote_change = quote.get('ui_change_amount', 0)
        base_change = base.get('ui_change_amount', 0)
        quote_type_swap = quote.get('type_swap', '')
        base_type_swap = base.get('type_swap', '')
        
        # Determine the actual direction from signs
        if quote_change < 0 and base_change > 0:
            actual_direction = "quote_to_base"
            token_out = quote.get('symbol')
            token_in = base.get('symbol')
            amount_out = abs(quote_change)
            amount_in = base_change
        elif quote_change > 0 and base_change < 0:
            actual_direction = "base_to_quote"  
            token_out = base.get('symbol')
            token_in = quote.get('symbol')
            amount_out = abs(base_change)
            amount_in = quote_change
        else:
            actual_direction = "unclear"
            token_out = "unknown"
            token_in = "unknown"
            amount_out = 0
            amount_in = 0
        
        # Check consistency with type_swap indicators
        type_swap_consistent = (
            (quote_change < 0 and quote_type_swap == "from") or
            (quote_change > 0 and quote_type_swap == "to")
        ) and (
            (base_change < 0 and base_type_swap == "from") or
            (base_change > 0 and base_type_swap == "to")
        )
        
        return {
            'actual_direction': actual_direction,
            'token_out': token_out,
            'token_in': token_in,
            'amount_out': amount_out,
            'amount_in': amount_in,
            'type_swap_consistent': type_swap_consistent,
            'quote_change_sign': 'neg' if quote_change < 0 else 'pos' if quote_change > 0 else 'zero',
            'base_change_sign': 'neg' if base_change < 0 else 'pos' if base_change > 0 else 'zero'
        }
    
    def analyze_sol_pattern(self, quote: Dict, base: Dict, sol_address: str) -> Dict:
        """Analyze SOL involvement patterns"""
        
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        if quote_is_sol and base_is_sol:
            pattern = "both_sol"  # Should not happen
        elif quote_is_sol:
            pattern = "quote_is_sol"
        elif base_is_sol:
            pattern = "base_is_sol"
        else:
            pattern = "no_sol"
        
        # Determine transaction type based on SOL position and direction
        quote_change = quote.get('ui_change_amount', 0)
        base_change = base.get('ui_change_amount', 0)
        
        if pattern == "quote_is_sol":
            if quote_change < 0:  # SOL spent
                transaction_type = "sol_to_token"  # BUY token
                sol_amount = abs(quote_change)
                token_symbol = base.get('symbol')
                token_amount = base_change
            else:  # SOL received  
                transaction_type = "token_to_sol"  # SELL token
                sol_amount = quote_change
                token_symbol = base.get('symbol')
                token_amount = abs(base_change)
        elif pattern == "base_is_sol":
            if base_change > 0:  # SOL received
                transaction_type = "token_to_sol"  # SELL token
                sol_amount = base_change
                token_symbol = quote.get('symbol')
                token_amount = abs(quote_change)
            else:  # SOL spent
                transaction_type = "sol_to_token"  # BUY token
                sol_amount = abs(base_change)
                token_symbol = quote.get('symbol')
                token_amount = quote_change
        else:
            transaction_type = "token_to_token"
            sol_amount = 0
            token_symbol = "multiple"
            token_amount = 0
        
        return {
            'pattern': pattern,
            'transaction_type': transaction_type,
            'sol_amount': sol_amount,
            'token_symbol': token_symbol,
            'token_amount': token_amount
        }
    
    def verify_transaction_math(self, quote: Dict, base: Dict, volume_usd: float) -> Dict:
        """Verify mathematical relationships in the data"""
        
        quote_usd = abs(quote.get('ui_change_amount', 0)) * quote.get('price', 0)
        base_usd = abs(base.get('ui_change_amount', 0)) * base.get('price', 0)
        
        return {
            'quote_usd_value': quote_usd,
            'base_usd_value': base_usd,
            'reported_volume_usd': volume_usd,
            'quote_base_match': abs(quote_usd - base_usd) < (max(quote_usd, base_usd) * 0.01),
            'volume_matches_quote': abs(volume_usd - quote_usd) < (max(volume_usd, quote_usd) * 0.01),
            'volume_matches_base': abs(volume_usd - base_usd) < (max(volume_usd, base_usd) * 0.01)
        }
    
    def print_transaction_breakdown(self, tx: Dict, analysis: Dict, num: int):
        """Print detailed breakdown of transaction structure"""
        
        print(f"\n📋 Transaction #{num} Structure Breakdown:")
        print(f"   Hash: {analysis['tx_hash']}...")
        print(f"   Source: {tx.get('source')}")
        
        # Quote analysis
        quote_info = analysis['quote_info']
        print(f"   Quote: {quote_info['symbol']} ({quote_info['type_swap']}) = {quote_info['change_amount']} [SOL: {quote_info['is_sol']}]")
        
        # Base analysis  
        base_info = analysis['base_info']
        print(f"   Base:  {base_info['symbol']} ({base_info['type_swap']}) = {base_info['change_amount']} [SOL: {base_info['is_sol']}]")
        
        # Direction analysis
        direction = analysis['direction_analysis']
        print(f"   Direction: {direction['token_out']} → {direction['token_in']} ({direction['actual_direction']})")
        print(f"   Amount: {direction['amount_out']} → {direction['amount_in']}")
        print(f"   Type_swap consistent: {direction['type_swap_consistent']}")
        
        # SOL pattern
        sol_pattern = analysis['sol_pattern']
        print(f"   SOL Pattern: {sol_pattern['pattern']} → {sol_pattern['transaction_type']}")
        if sol_pattern['sol_amount'] > 0:
            print(f"   SOL Amount: {sol_pattern['sol_amount']}, Token: {sol_pattern['token_amount']} {sol_pattern['token_symbol']}")
        
        # Math verification
        math_check = analysis['math_verification']
        print(f"   Math: Quote=${math_check['quote_usd_value']:.2f}, Base=${math_check['base_usd_value']:.2f}, Match={math_check['quote_base_match']}")
    
    def analyze_discovered_patterns(self, patterns: Dict):
        """Analyze the discovered patterns to understand data structure rules"""
        
        print(f"\n📊 Discovered Data Structure Rules:")
        print("=" * 45)
        
        # Collect all direction analyses
        direction_analyses = []
        sol_patterns = []
        math_verifications = []
        
        for tx_data in patterns.get('direction_rules', []):
            if isinstance(tx_data, dict) and 'direction_analysis' in tx_data:
                direction_analyses.append(tx_data['direction_analysis'])
        
        for tx_data in patterns.get('direction_rules', []):
            if isinstance(tx_data, dict) and 'sol_pattern' in tx_data:
                sol_patterns.append(tx_data['sol_pattern'])
                
        for tx_data in patterns.get('direction_rules', []):
            if isinstance(tx_data, dict) and 'math_verification' in tx_data:
                math_verifications.append(tx_data['math_verification'])
        
        # If patterns are stored differently, collect from main data
        if not direction_analyses:
            transactions = self.get_sample_data(50)
            sol_address = "So11111111111111111111111111111111111111112"
            
            for tx in transactions[:20]:  # Analyze 20 transactions
                analysis = self.analyze_single_transaction(tx, sol_address)
                direction_analyses.append(analysis['direction_analysis'])
                sol_patterns.append(analysis['sol_pattern'])
                math_verifications.append(analysis['math_verification'])
        
        # Analyze type_swap consistency
        consistent_type_swap = sum(1 for d in direction_analyses if d.get('type_swap_consistent', False))
        total_analyzed = len(direction_analyses)
        
        print(f"🎯 Direction Determination Rules:")
        print(f"   Type_swap consistency: {consistent_type_swap}/{total_analyzed} ({consistent_type_swap/total_analyzed*100:.1f}%)")
        
        # Analyze sign patterns
        sign_patterns = defaultdict(int)
        for d in direction_analyses:
            pattern = f"{d.get('quote_change_sign', 'unk')}_{d.get('base_change_sign', 'unk')}"
            sign_patterns[pattern] += 1
        
        print(f"   Sign patterns found:")
        for pattern, count in sign_patterns.items():
            print(f"     {pattern}: {count} transactions")
        
        # Analyze SOL patterns
        sol_pattern_counts = defaultdict(int)
        transaction_type_counts = defaultdict(int)
        
        for sp in sol_patterns:
            sol_pattern_counts[sp.get('pattern', 'unknown')] += 1
            transaction_type_counts[sp.get('transaction_type', 'unknown')] += 1
        
        print(f"\n🪙 SOL Involvement Patterns:")
        for pattern, count in sol_pattern_counts.items():
            print(f"   {pattern}: {count} transactions")
        
        print(f"\n📈 Transaction Type Distribution:")
        for tx_type, count in transaction_type_counts.items():
            print(f"   {tx_type}: {count} transactions")
        
        # Mathematical consistency
        math_consistent = sum(1 for m in math_verifications if m.get('quote_base_match', False))
        
        print(f"\n🧮 Mathematical Consistency:")
        print(f"   Quote/Base USD match: {math_consistent}/{total_analyzed} ({math_consistent/total_analyzed*100:.1f}%)")
        
        # Generate parsing rules
        self.generate_parsing_rules(direction_analyses, sol_patterns, math_verifications)
    
    def generate_parsing_rules(self, direction_analyses: List, sol_patterns: List, math_verifications: List):
        """Generate definitive parsing rules based on data analysis"""
        
        print(f"\n📜 DEFINITIVE PARSING RULES (Based on Data):")
        print("=" * 55)
        
        # Rule 1: Direction determination
        consistent_signs = sum(1 for d in direction_analyses 
                             if d.get('quote_change_sign') == 'neg' and d.get('base_change_sign') == 'pos' or
                                d.get('quote_change_sign') == 'pos' and d.get('base_change_sign') == 'neg')
        
        print(f"RULE 1 - Direction Determination:")
        print(f"  Use ui_change_amount signs to determine direction")
        print(f"  Opposite signs found in {consistent_signs}/{len(direction_analyses)} transactions")
        print(f"  Negative change_amount = token spent/sold")
        print(f"  Positive change_amount = token received/bought")
        
        # Rule 2: SOL involvement
        sol_involved = sum(1 for sp in sol_patterns if sp.get('pattern') in ['quote_is_sol', 'base_is_sol'])
        no_sol = sum(1 for sp in sol_patterns if sp.get('pattern') == 'no_sol')
        
        print(f"\nRULE 2 - SOL Involvement:")
        print(f"  SOL involved: {sol_involved} transactions")
        print(f"  Token-to-token: {no_sol} transactions")
        print(f"  Check address against 'So11111111111111111111111111111111111111112'")
        
        # Rule 3: Event generation
        sol_to_token = sum(1 for sp in sol_patterns if sp.get('transaction_type') == 'sol_to_token')
        token_to_sol = sum(1 for sp in sol_patterns if sp.get('transaction_type') == 'token_to_sol')
        token_to_token = sum(1 for sp in sol_patterns if sp.get('transaction_type') == 'token_to_token')
        
        print(f"\nRULE 3 - Event Generation:")
        print(f"  SOL → Token (BUY events): {sol_to_token}")
        print(f"  Token → SOL (SELL events): {token_to_sol}")
        print(f"  Token → Token (dual events): {token_to_token}")
        
        # Rule 4: Mathematical relationships
        math_consistent = sum(1 for m in math_verifications if m.get('quote_base_match', False))
        
        print(f"\nRULE 4 - Price/Volume Relationships:")
        print(f"  Quote/Base USD values match: {math_consistent}/{len(math_verifications)} transactions")
        print(f"  Use embedded prices for USD calculations")
        print(f"  Convert USD to SOL using current SOL price")

def main():
    analyzer = DataStructureAnalyzer()
    analyzer.analyze_data_structure_patterns()

if __name__ == "__main__":
    main()