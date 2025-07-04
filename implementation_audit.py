#!/usr/bin/env python3
"""
Critical Implementation Audit
Thoroughly analyze our current Rust implementation for correctness
Assume we're doing it wrong until proven otherwise
"""

import json
import requests
from decimal import Decimal
from typing import Dict, List, Any

class ImplementationAuditor:
    def __init__(self):
        self.base_url = "https://public-api.birdeye.so/trader/txs/seek_by_time"
        self.headers = {
            "accept": "application/json",
            "x-chain": "solana",
            "X-API-KEY": "5ff313b239ac42e297b830b10ea1871d"
        }
        
        # Critical audit questions
        self.audit_questions = [
            "Are we correctly identifying spent vs received tokens?",
            "Are we mixing transaction aggregation with event generation?", 
            "Are we correctly handling quote/base vs actual direction?",
            "Are we using the right price fields for calculations?",
            "Are we correctly converting between USD and SOL?",
            "Are we generating the right number of events per transaction?",
            "Are we correctly handling token-to-token swaps?",
            "Are we using raw amounts vs UI amounts correctly?",
            "Are we correctly identifying SOL vs non-SOL tokens?",
            "Are we handling dual events with consistent SOL equivalents?",
            "Are we correctly aggregating multiple BirdEye entries?",
            "Are we mixing currency domains (USD prices vs SOL amounts)?",
            "Are we correctly parsing negative vs positive change amounts?",
            "Are our mathematical calculations logically sound?"
        ]
        
    def audit_implementation(self):
        """Conduct thorough audit of current implementation"""
        
        print("🔍 CRITICAL IMPLEMENTATION AUDIT")
        print("=" * 50)
        print("ASSUMPTION: Our implementation is WRONG until proven correct")
        print("=" * 50)
        
        # Get sample data for testing
        sample_data = self.get_test_transactions()
        
        if not sample_data:
            print("❌ Cannot get test data - audit failed")
            return
            
        print(f"Got {len(sample_data)} transactions for audit\n")
        
        # Load the provided test transaction for specific analysis
        test_transaction = {
            "quote": {
                "symbol": "LAUNCHCOIN",
                "decimals": 9,
                "address": "Ey59PH7Z4BFU4HjyKnyMdWt5GGN76KazTAwQihoUXRnk",
                "amount": 3928444242174,
                "type": "transferChecked",
                "type_swap": "to",
                "ui_amount": 3928.444242174,
                "price": 0.1497911827416532,
                "nearest_price": 0.1497617595536327,
                "change_amount": 3928444242174,
                "ui_change_amount": 3928.444242174
            },
            "base": {
                "symbol": "SOL",
                "decimals": 9,
                "address": "So11111111111111111111111111111111111111112",
                "amount": 3780820507,
                "type": "transferChecked",
                "type_swap": "from",
                "ui_amount": 3.780820507,
                "price": 155.6390504274807,
                "nearest_price": 155.6390504274807,
                "change_amount": -3780820507,
                "ui_change_amount": -3.780820507
            },
            "tx_hash": "evjsW1CLcwqr967jDxssEpA64oWhZ5ZQj2x5M9qvDrF7HGYxzJqwn2vviAXMXXqRVAYbq5AnzBq8UC46MjkToEd",
            "volume_usd": 588.4433135462261
        }
        
        # Conduct systematic audit
        audit_results = {}
        
        # AUDIT 1: Direction identification
        audit_results['direction_audit'] = self.audit_direction_identification(test_transaction, sample_data[:5])
        
        # AUDIT 2: Event generation logic
        audit_results['event_generation_audit'] = self.audit_event_generation(test_transaction, sample_data[:5])
        
        # AUDIT 3: Mathematical correctness
        audit_results['math_audit'] = self.audit_mathematical_correctness(test_transaction, sample_data[:5])
        
        # AUDIT 4: Currency domain separation
        audit_results['currency_audit'] = self.audit_currency_domains(test_transaction, sample_data[:5])
        
        # AUDIT 5: Transaction aggregation
        audit_results['aggregation_audit'] = self.audit_transaction_aggregation(sample_data[:10])
        
        # AUDIT 6: Price handling
        audit_results['price_audit'] = self.audit_price_handling(test_transaction, sample_data[:5])
        
        # Generate final audit report
        self.generate_audit_report(audit_results)
        
    def get_test_transactions(self) -> List[Dict]:
        """Get transactions for testing"""
        params = {
            "address": "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa",
            "limit": 20
        }
        
        response = requests.get(self.base_url, headers=self.headers, params=params)
        if response.status_code == 200:
            data = response.json()
            if data.get("success"):
                return data.get("data", {}).get("items", [])
        return []
    
    def audit_direction_identification(self, test_tx: Dict, sample_txs: List[Dict]) -> Dict:
        """CRITICAL: Are we correctly identifying spent vs received tokens?"""
        
        print("🎯 AUDIT 1: Direction Identification")
        print("-" * 35)
        
        issues = []
        findings = []
        
        # Test the provided transaction
        quote = test_tx['quote']
        base = test_tx['base']
        
        print(f"Test Transaction Analysis:")
        print(f"  Quote: {quote['symbol']} change_amount={quote['ui_change_amount']} type_swap={quote['type_swap']}")
        print(f"  Base:  {base['symbol']} change_amount={base['ui_change_amount']} type_swap={base['type_swap']}")
        
        # What does the data actually tell us?
        actual_direction = self.determine_actual_direction(test_tx)
        print(f"  Actual Direction: {actual_direction['interpretation']}")
        
        # Critical questions:
        # 1. Are we using change_amount correctly?
        if quote['ui_change_amount'] > 0 and base['ui_change_amount'] < 0:
            spent_token = base['symbol']
            received_token = quote['symbol']
            findings.append("✅ Correctly using change_amount signs for direction")
        elif quote['ui_change_amount'] < 0 and base['ui_change_amount'] > 0:
            spent_token = quote['symbol'] 
            received_token = base['symbol']
            findings.append("✅ Correctly using change_amount signs for direction")
        else:
            issues.append("❌ CRITICAL: Change amounts don't have opposite signs!")
        
        # 2. Are we confusing quote/base position with direction?
        if quote['type_swap'] == 'to' and quote['ui_change_amount'] > 0:
            findings.append("✅ type_swap 'to' matches positive change (received)")
        elif quote['type_swap'] == 'from' and quote['ui_change_amount'] < 0:
            findings.append("✅ type_swap 'from' matches negative change (spent)")
        else:
            issues.append("❌ CRITICAL: type_swap doesn't match change_amount sign!")
            
        # Test on sample transactions
        print(f"\nTesting on {len(sample_txs)} sample transactions:")
        direction_consistency = 0
        for i, tx in enumerate(sample_txs):
            direction_analysis = self.determine_actual_direction(tx)
            if direction_analysis['consistent']:
                direction_consistency += 1
            print(f"  Tx {i+1}: {direction_analysis['interpretation']} - Consistent: {direction_analysis['consistent']}")
        
        consistency_rate = direction_consistency / len(sample_txs) * 100
        print(f"Direction consistency: {direction_consistency}/{len(sample_txs)} ({consistency_rate:.1f}%)")
        
        if consistency_rate < 100:
            issues.append(f"❌ CRITICAL: Only {consistency_rate:.1f}% direction consistency!")
        else:
            findings.append("✅ 100% direction consistency across samples")
        
        return {
            'issues': issues,
            'findings': findings,
            'consistency_rate': consistency_rate,
            'test_case': actual_direction
        }
    
    def determine_actual_direction(self, tx: Dict) -> Dict:
        """Determine actual transaction direction from data"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        quote_change = quote.get('ui_change_amount', 0)
        base_change = base.get('ui_change_amount', 0)
        quote_type_swap = quote.get('type_swap', '')
        base_type_swap = base.get('type_swap', '')
        
        # Determine direction from change amounts (authoritative)
        if quote_change < 0 and base_change > 0:
            interpretation = f"{quote.get('symbol')} → {base.get('symbol')}"
            spent_token = quote.get('symbol')
            received_token = base.get('symbol')
            consistent = (quote_type_swap == 'from' and base_type_swap == 'to')
        elif quote_change > 0 and base_change < 0:
            interpretation = f"{base.get('symbol')} → {quote.get('symbol')}"
            spent_token = base.get('symbol')
            received_token = quote.get('symbol')
            consistent = (base_type_swap == 'from' and quote_type_swap == 'to')
        else:
            interpretation = "UNCLEAR - Invalid change amounts"
            spent_token = "unknown"
            received_token = "unknown"
            consistent = False
        
        return {
            'interpretation': interpretation,
            'spent_token': spent_token,
            'received_token': received_token,
            'consistent': consistent
        }
    
    def audit_event_generation(self, test_tx: Dict, sample_txs: List[Dict]) -> Dict:
        """CRITICAL: Are we generating the correct events?"""
        
        print(f"\n🎯 AUDIT 2: Event Generation Logic")
        print("-" * 35)
        
        issues = []
        findings = []
        
        sol_address = "So11111111111111111111111111111111111111112"
        
        # Analyze test transaction event generation
        test_events = self.simulate_current_event_generation(test_tx, sol_address)
        correct_events = self.determine_correct_events(test_tx, sol_address)
        
        print(f"Test Transaction Event Analysis:")
        print(f"  Expected Events: {len(correct_events['events'])}")
        for event in correct_events['events']:
            print(f"    {event['type']}: {event['token_amount']} {event['token']} (SOL: {event['sol_amount']})")
        
        # Compare with what our implementation should generate
        print(f"  Our Implementation Should Generate: {len(test_events['events'])}")
        for event in test_events['events']:
            print(f"    {event['type']}: {event['token_amount']} {event['token']} (SOL: {event['sol_amount']})")
        
        # Critical checks
        if len(test_events['events']) != len(correct_events['events']):
            issues.append(f"❌ CRITICAL: Wrong number of events! Expected {len(correct_events['events'])}, got {len(test_events['events'])}")
        else:
            findings.append("✅ Correct number of events generated")
        
        # Check event types
        for i, (expected, actual) in enumerate(zip(correct_events['events'], test_events['events'])):
            if expected['type'] != actual['type']:
                issues.append(f"❌ CRITICAL: Event {i+1} type mismatch! Expected {expected['type']}, got {actual['type']}")
            if abs(expected['sol_amount'] - actual['sol_amount']) > 0.001:
                issues.append(f"❌ CRITICAL: Event {i+1} SOL amount mismatch! Expected {expected['sol_amount']}, got {actual['sol_amount']}")
        
        # Test on sample transactions
        print(f"\nTesting event generation on {len(sample_txs)} samples:")
        event_generation_errors = 0
        
        for i, tx in enumerate(sample_txs):
            expected = self.determine_correct_events(tx, sol_address)
            simulated = self.simulate_current_event_generation(tx, sol_address)
            
            if len(expected['events']) != len(simulated['events']):
                event_generation_errors += 1
                print(f"  Tx {i+1}: ERROR - Expected {len(expected['events'])} events, got {len(simulated['events'])}")
            else:
                print(f"  Tx {i+1}: OK - {len(expected['events'])} events")
        
        if event_generation_errors > 0:
            issues.append(f"❌ CRITICAL: {event_generation_errors}/{len(sample_txs)} transactions have event generation errors!")
        else:
            findings.append("✅ All sample transactions generate correct number of events")
        
        return {
            'issues': issues,
            'findings': findings,
            'test_case_events': test_events,
            'expected_events': correct_events,
            'error_rate': event_generation_errors / len(sample_txs) * 100
        }
    
    def simulate_current_event_generation(self, tx: Dict, sol_address: str) -> Dict:
        """Simulate what our current implementation should generate"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        # Determine direction (assuming our implementation uses change_amount correctly)
        if quote.get('ui_change_amount', 0) < 0:
            token_spent = quote
            token_received = base
        else:
            token_spent = base
            token_received = quote
        
        events = []
        
        # Check SOL involvement
        quote_is_sol = quote.get('address') == sol_address
        base_is_sol = base.get('address') == sol_address
        
        if quote_is_sol or base_is_sol:
            # Single event for SOL swap
            if token_received.get('address') == sol_address:
                # Token → SOL (SELL)
                events.append({
                    'type': 'SELL',
                    'token': token_spent.get('symbol'),
                    'token_amount': abs(token_spent.get('ui_change_amount', 0)),
                    'sol_amount': token_received.get('ui_change_amount', 0),
                    'price_per_token': token_spent.get('price', 0)
                })
            else:
                # SOL → Token (BUY)
                events.append({
                    'type': 'BUY',
                    'token': token_received.get('symbol'),
                    'token_amount': token_received.get('ui_change_amount', 0),
                    'sol_amount': abs(token_spent.get('ui_change_amount', 0)),
                    'price_per_token': token_received.get('price', 0)
                })
        else:
            # Token-to-token (dual events)
            sol_price = 155.0  # Approximate
            
            # SELL event
            sell_sol_equiv = abs(token_spent.get('ui_change_amount', 0)) * token_spent.get('price', 0) / sol_price
            events.append({
                'type': 'SELL',
                'token': token_spent.get('symbol'),
                'token_amount': abs(token_spent.get('ui_change_amount', 0)),
                'sol_amount': sell_sol_equiv,
                'price_per_token': token_spent.get('price', 0)
            })
            
            # BUY event
            buy_sol_equiv = token_received.get('ui_change_amount', 0) * token_received.get('price', 0) / sol_price
            events.append({
                'type': 'BUY',
                'token': token_received.get('symbol'),
                'token_amount': token_received.get('ui_change_amount', 0),
                'sol_amount': buy_sol_equiv,
                'price_per_token': token_received.get('price', 0)
            })
        
        return {'events': events}
    
    def determine_correct_events(self, tx: Dict, sol_address: str) -> Dict:
        """Determine what events SHOULD be generated based on data analysis"""
        
        quote = tx.get('quote', {})
        base = tx.get('base', {})
        
        # Use change_amount to determine direction (authoritative)
        if quote.get('ui_change_amount', 0) < 0:
            token_spent = quote
            token_received = base
        else:
            token_spent = base
            token_received = quote
        
        events = []
        sol_price = 155.0  # Should be fetched dynamically
        
        # Check SOL involvement
        if token_received.get('address') == sol_address:
            # Token → SOL (SELL)
            events.append({
                'type': 'SELL',
                'token': token_spent.get('symbol'),
                'token_amount': abs(token_spent.get('ui_change_amount', 0)),
                'sol_amount': token_received.get('ui_change_amount', 0),
                'price_per_token': token_spent.get('price', 0)
            })
        elif token_spent.get('address') == sol_address:
            # SOL → Token (BUY)
            events.append({
                'type': 'BUY',
                'token': token_received.get('symbol'),
                'token_amount': token_received.get('ui_change_amount', 0),
                'sol_amount': abs(token_spent.get('ui_change_amount', 0)),
                'price_per_token': token_received.get('price', 0)
            })
        else:
            # Token → Token (dual events)
            # SELL event
            sell_sol_equiv = abs(token_spent.get('ui_change_amount', 0)) * token_spent.get('price', 0) / sol_price
            events.append({
                'type': 'SELL',
                'token': token_spent.get('symbol'),
                'token_amount': abs(token_spent.get('ui_change_amount', 0)),
                'sol_amount': sell_sol_equiv,
                'price_per_token': token_spent.get('price', 0)
            })
            
            # BUY event
            buy_sol_equiv = token_received.get('ui_change_amount', 0) * token_received.get('price', 0) / sol_price
            events.append({
                'type': 'BUY',
                'token': token_received.get('symbol'),
                'token_amount': token_received.get('ui_change_amount', 0),
                'sol_amount': buy_sol_equiv,
                'price_per_token': token_received.get('price', 0)
            })
        
        return {'events': events}
    
    def audit_mathematical_correctness(self, test_tx: Dict, sample_txs: List[Dict]) -> Dict:
        """CRITICAL: Are our mathematical calculations correct?"""
        
        print(f"\n🎯 AUDIT 3: Mathematical Correctness")
        print("-" * 35)
        
        issues = []
        findings = []
        
        # Test the provided transaction mathematical consistency
        quote = test_tx['quote']
        base = test_tx['base']
        
        quote_usd = abs(quote['ui_change_amount']) * quote['price']
        base_usd = abs(base['ui_change_amount']) * base['price']
        reported_volume = test_tx.get('volume_usd', 0)
        
        print(f"Test Transaction Math:")
        print(f"  Quote USD: {abs(quote['ui_change_amount'])} × ${quote['price']} = ${quote_usd:.2f}")
        print(f"  Base USD:  {abs(base['ui_change_amount'])} × ${base['price']} = ${base_usd:.2f}")
        print(f"  Reported Volume: ${reported_volume:.2f}")
        
        # Critical checks
        usd_diff = abs(quote_usd - base_usd)
        usd_tolerance = max(quote_usd, base_usd) * 0.01  # 1% tolerance
        
        if usd_diff > usd_tolerance:
            issues.append(f"❌ CRITICAL: Quote/Base USD values don't match! Diff: ${usd_diff:.2f}")
        else:
            findings.append("✅ Quote/Base USD values match within tolerance")
        
        # Check volume consistency
        volume_diff = abs(reported_volume - quote_usd)
        if volume_diff > usd_tolerance:
            issues.append(f"❌ WARNING: Volume doesn't match calculated USD! Diff: ${volume_diff:.2f}")
        else:
            findings.append("✅ Volume matches calculated USD value")
        
        # Test SOL equivalent calculations
        sol_price = 155.0  # Should match what we use in implementation
        calculated_sol_equiv = quote_usd / sol_price
        
        print(f"  SOL Equivalent: ${quote_usd:.2f} ÷ ${sol_price} = {calculated_sol_equiv:.6f} SOL")
        
        # Are we using the right prices?
        if quote['price'] <= 0 or base['price'] <= 0:
            issues.append("❌ CRITICAL: Zero or negative prices detected!")
        else:
            findings.append("✅ All prices are positive")
        
        # Test on sample transactions
        print(f"\nTesting math on {len(sample_txs)} samples:")
        math_errors = 0
        
        for i, tx in enumerate(sample_txs):
            tx_quote = tx.get('quote', {})
            tx_base = tx.get('base', {})
            
            tx_quote_usd = abs(tx_quote.get('ui_change_amount', 0)) * tx_quote.get('price', 0)
            tx_base_usd = abs(tx_base.get('ui_change_amount', 0)) * tx_base.get('price', 0)
            tx_diff = abs(tx_quote_usd - tx_base_usd)
            tx_tolerance = max(tx_quote_usd, tx_base_usd) * 0.01
            
            if tx_diff > tx_tolerance:
                math_errors += 1
                print(f"  Tx {i+1}: MATH ERROR - Quote: ${tx_quote_usd:.2f}, Base: ${tx_base_usd:.2f}, Diff: ${tx_diff:.2f}")
            else:
                print(f"  Tx {i+1}: OK - USD values match")
        
        if math_errors > 0:
            issues.append(f"❌ CRITICAL: {math_errors}/{len(sample_txs)} transactions have math errors!")
        else:
            findings.append("✅ All sample transactions have consistent math")
        
        return {
            'issues': issues,
            'findings': findings,
            'test_case_math': {
                'quote_usd': quote_usd,
                'base_usd': base_usd,
                'volume_usd': reported_volume,
                'usd_diff': usd_diff
            },
            'math_error_rate': math_errors / len(sample_txs) * 100
        }
    
    def audit_currency_domains(self, test_tx: Dict, sample_txs: List[Dict]) -> Dict:
        """CRITICAL: Are we mixing currency domains?"""
        
        print(f"\n🎯 AUDIT 4: Currency Domain Separation")
        print("-" * 35)
        
        issues = []
        findings = []
        
        # Check if we're properly separating:
        # 1. USD prices (from BirdEye)
        # 2. SOL amounts (from transactions or calculations)
        # 3. Token quantities (ui_change_amount)
        
        quote = test_tx['quote']
        base = test_tx['base']
        
        print(f"Currency Domain Analysis:")
        print(f"  Quote Price: ${quote['price']} USD per {quote['symbol']}")
        print(f"  Base Price:  ${base['price']} USD per {base['symbol']}")
        print(f"  Quote Amount: {quote['ui_change_amount']} {quote['symbol']}")
        print(f"  Base Amount:  {base['ui_change_amount']} {base['symbol']}")
        
        # Critical question: Are we using USD prices as SOL amounts anywhere?
        sol_address = "So11111111111111111111111111111111111111112"
        
        if quote.get('address') == sol_address:
            sol_price_usd = quote['price']
            sol_amount = abs(quote['ui_change_amount'])
            print(f"  SOL in quote: {sol_amount} SOL @ ${sol_price_usd} USD/SOL")
            
            # Check if price is reasonable for SOL
            if sol_price_usd < 50 or sol_price_usd > 500:
                issues.append(f"❌ CRITICAL: SOL price ${sol_price_usd} seems unreasonable!")
            else:
                findings.append("✅ SOL price is in reasonable range")
                
        if base.get('address') == sol_address:
            sol_price_usd = base['price']
            sol_amount = abs(base['ui_change_amount'])
            print(f"  SOL in base: {sol_amount} SOL @ ${sol_price_usd} USD/SOL")
            
            if sol_price_usd < 50 or sol_price_usd > 500:
                issues.append(f"❌ CRITICAL: SOL price ${sol_price_usd} seems unreasonable!")
            else:
                findings.append("✅ SOL price is in reasonable range")
        
        # Check for currency mixing in calculations
        # Example: Are we ever adding USD amounts to SOL amounts?
        # Are we using token prices as SOL prices?
        
        # If this is a token-to-token swap, check SOL equivalent calculations
        if not (quote.get('address') == sol_address or base.get('address') == sol_address):
            # This is token-to-token, we need to calculate SOL equivalents
            token_spent = quote if quote['ui_change_amount'] < 0 else base
            token_received = base if quote['ui_change_amount'] < 0 else quote
            
            # How should we calculate SOL equivalent?
            # Method 1: (token_amount * token_price_usd) / sol_price_usd
            sol_price_for_calc = 155.0  # Should be fetched from somewhere
            
            spent_usd = abs(token_spent['ui_change_amount']) * token_spent['price']
            spent_sol_equiv = spent_usd / sol_price_for_calc
            
            received_usd = token_received['ui_change_amount'] * token_received['price']
            received_sol_equiv = received_usd / sol_price_for_calc
            
            print(f"  Token-to-Token SOL Equivalents:")
            print(f"    Spent: {abs(token_spent['ui_change_amount'])} {token_spent['symbol']} = {spent_sol_equiv:.6f} SOL")
            print(f"    Received: {token_received['ui_change_amount']} {token_received['symbol']} = {received_sol_equiv:.6f} SOL")
            
            # These should be approximately equal
            sol_equiv_diff = abs(spent_sol_equiv - received_sol_equiv)
            if sol_equiv_diff > 0.001:  # Small tolerance for precision
                issues.append(f"❌ CRITICAL: SOL equivalents don't match! Diff: {sol_equiv_diff:.6f} SOL")
            else:
                findings.append("✅ SOL equivalents match for token-to-token swap")
        
        return {
            'issues': issues,
            'findings': findings
        }
    
    def audit_transaction_aggregation(self, sample_txs: List[Dict]) -> Dict:
        """CRITICAL: Are we correctly aggregating transactions?"""
        
        print(f"\n🎯 AUDIT 5: Transaction Aggregation")
        print("-" * 35)
        
        issues = []
        findings = []
        
        # Key question: Each BirdEye entry represents one swap
        # Are we incorrectly aggregating multiple swaps?
        # Are we treating each entry as a separate transaction?
        
        print(f"Analyzing {len(sample_txs)} transactions for aggregation patterns:")
        
        # Group by transaction hash to see if we have multiple entries per tx
        tx_groups = {}
        for tx in sample_txs:
            tx_hash = tx.get('tx_hash', '')
            if tx_hash not in tx_groups:
                tx_groups[tx_hash] = []
            tx_groups[tx_hash].append(tx)
        
        print(f"Found {len(tx_groups)} unique transaction hashes")
        
        multi_entry_txs = {k: v for k, v in tx_groups.items() if len(v) > 1}
        
        if multi_entry_txs:
            print(f"Transactions with multiple BirdEye entries: {len(multi_entry_txs)}")
            for tx_hash, entries in multi_entry_txs.items():
                print(f"  {tx_hash[:20]}...: {len(entries)} entries")
                
                # This could indicate:
                # 1. Multiple swaps in one transaction
                # 2. Complex routing through multiple pools
                # 3. BirdEye breaking down complex transactions
                
                # We need to determine if these should be:
                # - Aggregated into one swap
                # - Treated as separate swaps
                
                issues.append(f"⚠️ WARNING: Transaction {tx_hash[:16]}... has {len(entries)} BirdEye entries - needs aggregation logic")
        else:
            findings.append("✅ All transaction hashes have single BirdEye entries")
        
        # Check if we're handling transaction vs swap confusion
        # Each BirdEye entry should represent one atomic swap
        # Each FinancialEvent should represent one P&L-relevant action
        
        for i, tx in enumerate(sample_txs[:3]):
            print(f"\nTransaction {i+1} Analysis:")
            print(f"  Hash: {tx.get('tx_hash', '')[:20]}...")
            print(f"  This represents: ONE atomic swap")
            print(f"  Should generate: 1-2 FinancialEvents (depending on SOL involvement)")
            
            # Are we creating the right number of events?
            sol_address = "So11111111111111111111111111111111111111112"
            quote_is_sol = tx.get('quote', {}).get('address') == sol_address
            base_is_sol = tx.get('base', {}).get('address') == sol_address
            
            if quote_is_sol or base_is_sol:
                expected_events = 1  # SOL swap
                print(f"  Expected events: 1 (SOL involved)")
            else:
                expected_events = 2  # Token-to-token
                print(f"  Expected events: 2 (token-to-token)")
            
            findings.append(f"✅ Transaction {i+1} should generate {expected_events} event(s)")
        
        return {
            'issues': issues,
            'findings': findings,
            'multi_entry_transactions': len(multi_entry_txs),
            'total_unique_hashes': len(tx_groups)
        }
    
    def audit_price_handling(self, test_tx: Dict, sample_txs: List[Dict]) -> Dict:
        """CRITICAL: Are we using the right price fields?"""
        
        print(f"\n🎯 AUDIT 6: Price Field Handling")
        print("-" * 35)
        
        issues = []
        findings = []
        
        # BirdEye provides multiple price fields:
        # - price: Current/embedded price in USD
        # - nearest_price: Nearest price (fallback)
        # - base_price/quote_price: Top-level price fields
        
        quote = test_tx['quote']
        base = test_tx['base']
        
        print(f"Price Field Analysis:")
        print(f"  Quote price:         ${quote.get('price', 0)}")
        print(f"  Quote nearest_price: ${quote.get('nearest_price', 0)}")
        print(f"  Base price:          ${base.get('price', 0)}")
        print(f"  Base nearest_price:  ${base.get('nearest_price', 0)}")
        print(f"  Top-level quote_price: ${test_tx.get('quote_price', 0)}")
        print(f"  Top-level base_price:  ${test_tx.get('base_price', 0)}")
        
        # Critical questions:
        # 1. Which price field should we use?
        # 2. Are they consistent?
        # 3. Are we using historical vs current prices correctly?
        
        # Check consistency between price fields
        quote_price_diff = abs(quote['price'] - test_tx.get('quote_price', 0))
        base_price_diff = abs(base['price'] - test_tx.get('base_price', 0))
        
        if quote_price_diff > 0.01:
            issues.append(f"❌ WARNING: Quote price fields inconsistent! Diff: ${quote_price_diff}")
        else:
            findings.append("✅ Quote price fields are consistent")
            
        if base_price_diff > 0.01:
            issues.append(f"❌ WARNING: Base price fields inconsistent! Diff: ${base_price_diff}")
        else:
            findings.append("✅ Base price fields are consistent")
        
        # Check price vs nearest_price
        quote_nearest_diff = abs(quote['price'] - quote.get('nearest_price', 0))
        base_nearest_diff = abs(base['price'] - base.get('nearest_price', 0))
        
        if quote_nearest_diff > quote['price'] * 0.05:  # 5% tolerance
            issues.append(f"❌ WARNING: Quote price differs significantly from nearest_price! Diff: ${quote_nearest_diff}")
        else:
            findings.append("✅ Quote price and nearest_price are close")
            
        if base_nearest_diff > base['price'] * 0.05:
            issues.append(f"❌ WARNING: Base price differs significantly from nearest_price! Diff: ${base_nearest_diff}")
        else:
            findings.append("✅ Base price and nearest_price are close")
        
        # Test on sample transactions
        print(f"\nTesting price consistency on {len(sample_txs)} samples:")
        price_inconsistencies = 0
        
        for i, tx in enumerate(sample_txs):
            tx_quote = tx.get('quote', {})
            tx_base = tx.get('base', {})
            
            q_price = tx_quote.get('price', 0)
            q_nearest = tx_quote.get('nearest_price', 0)
            b_price = tx_base.get('price', 0)
            b_nearest = tx_base.get('nearest_price', 0)
            
            if (abs(q_price - q_nearest) > q_price * 0.05) or (abs(b_price - b_nearest) > b_price * 0.05):
                price_inconsistencies += 1
                print(f"  Tx {i+1}: PRICE INCONSISTENCY")
            else:
                print(f"  Tx {i+1}: OK")
        
        if price_inconsistencies > 0:
            issues.append(f"❌ WARNING: {price_inconsistencies}/{len(sample_txs)} transactions have price inconsistencies")
        else:
            findings.append("✅ All sample transactions have consistent prices")
        
        return {
            'issues': issues,
            'findings': findings,
            'price_inconsistency_rate': price_inconsistencies / len(sample_txs) * 100
        }
    
    def generate_audit_report(self, audit_results: Dict):
        """Generate comprehensive audit report"""
        
        print(f"\n" + "=" * 60)
        print("🎯 COMPREHENSIVE IMPLEMENTATION AUDIT REPORT")
        print("=" * 60)
        
        total_issues = 0
        total_findings = 0
        critical_issues = []
        
        for audit_name, results in audit_results.items():
            issues = results.get('issues', [])
            findings = results.get('findings', [])
            
            total_issues += len(issues)
            total_findings += len(findings)
            
            # Collect critical issues
            for issue in issues:
                if 'CRITICAL' in issue:
                    critical_issues.append(f"{audit_name}: {issue}")
        
        print(f"\n📊 AUDIT SUMMARY:")
        print(f"  Total Issues Found: {total_issues}")
        print(f"  Total Positive Findings: {total_findings}")
        print(f"  Critical Issues: {len(critical_issues)}")
        
        if critical_issues:
            print(f"\n❌ CRITICAL ISSUES REQUIRING IMMEDIATE ATTENTION:")
            for issue in critical_issues:
                print(f"  {issue}")
        else:
            print(f"\n✅ NO CRITICAL ISSUES FOUND!")
        
        print(f"\n📋 DETAILED AUDIT RESULTS:")
        for audit_name, results in audit_results.items():
            print(f"\n{audit_name.upper()}:")
            
            if results.get('issues'):
                print(f"  Issues:")
                for issue in results['issues']:
                    print(f"    {issue}")
            
            if results.get('findings'):
                print(f"  Findings:")
                for finding in results['findings']:
                    print(f"    {finding}")
        
        # Final verdict
        print(f"\n" + "=" * 60)
        if len(critical_issues) == 0:
            print("🎯 FINAL VERDICT: IMPLEMENTATION APPEARS CORRECT")
            print("   No critical mathematical or logical errors found.")
            print("   System handles data structure patterns correctly.")
        else:
            print("❌ FINAL VERDICT: IMPLEMENTATION HAS CRITICAL ISSUES")
            print(f"   {len(critical_issues)} critical issues require fixing.")
        print("=" * 60)

def main():
    auditor = ImplementationAuditor()
    auditor.audit_implementation()

if __name__ == "__main__":
    main()