#[cfg(test)]
mod comprehensive_tests {
    use crate::new_parser::{NewFinancialEvent, NewEventType};
    use crate::new_pnl_engine::NewPnLEngine;
    use rust_decimal::Decimal;
    use rust_decimal::prelude::FromPrimitive;
    use chrono::DateTime;
    
    /// Test complex FIFO scenario with multiple buys and sells
    #[tokio::test]
    async fn test_complex_fifo_scenario() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        let events = vec![
            // Buy 100 tokens @ $1.00 (timestamp 1000)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(1),
                usd_value: Decimal::from(100),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            // Buy 200 tokens @ $2.00 (timestamp 2000)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(200),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from(400),
                timestamp: DateTime::from_timestamp(2000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
            // Sell 150 tokens @ $3.00 (timestamp 3000)
            // Should match: 100 @ $1 (P&L = $200) + 50 @ $2 (P&L = $50)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(150),
                usd_price_per_token: Decimal::from(3),
                usd_value: Decimal::from(450),
                timestamp: DateTime::from_timestamp(3000, 0).unwrap(),
                transaction_hash: "tx3".to_string(),
            },
            // Buy 50 tokens @ $1.50 (timestamp 4000)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(50),
                usd_price_per_token: Decimal::from_f64(1.5).unwrap(),
                usd_value: Decimal::from(75),
                timestamp: DateTime::from_timestamp(4000, 0).unwrap(),
                transaction_hash: "tx4".to_string(),
            },
            // Sell 100 tokens @ $4.00 (timestamp 5000)
            // Should match: 150 @ $2 (P&L = $300) + 50 @ $1.50 (P&L = $125)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(200),
                usd_price_per_token: Decimal::from(4),
                usd_value: Decimal::from(800),
                timestamp: DateTime::from_timestamp(5000, 0).unwrap(),
                transaction_hash: "tx5".to_string(),
            },
        ];
        
        let result = engine.calculate_token_pnl(events, Some(Decimal::from(5))).await.unwrap();
        
        // Should have 4 matched trades
        assert_eq!(result.matched_trades.len(), 4);
        
        // First trade: 100 @ $1 -> 100 @ $3 = $200 profit
        assert_eq!(result.matched_trades[0].matched_quantity, Decimal::from(100));
        assert_eq!(result.matched_trades[0].realized_pnl_usd, Decimal::from(200));
        
        // Second trade: 50 @ $2 -> 50 @ $3 = $50 profit
        assert_eq!(result.matched_trades[1].matched_quantity, Decimal::from(50));
        assert_eq!(result.matched_trades[1].realized_pnl_usd, Decimal::from(50));
        
        // Third trade: 150 @ $2 -> 150 @ $4 = $300 profit
        assert_eq!(result.matched_trades[2].matched_quantity, Decimal::from(150));
        assert_eq!(result.matched_trades[2].realized_pnl_usd, Decimal::from(300));
        
        // Fourth trade: 50 @ $1.50 -> 50 @ $4 = $125 profit
        assert_eq!(result.matched_trades[3].matched_quantity, Decimal::from(50));
        assert_eq!(result.matched_trades[3].realized_pnl_usd, Decimal::from(125));
        
        // Total realized P&L: $200 + $50 + $300 + $125 = $675
        assert_eq!(result.total_realized_pnl_usd, Decimal::from(675));
        
        // Should have no remaining position (all tokens sold)
        assert!(result.remaining_position.is_none());
        
        // Unrealized P&L: $0 (no remaining position)
        assert_eq!(result.total_unrealized_pnl_usd, Decimal::ZERO);
        
        // Total P&L: $675 (realized) + $0 (unrealized) = $675
        assert_eq!(result.total_pnl_usd, Decimal::from(675));
        
        // Win rate: 4/4 = 100%
        assert_eq!(result.win_rate_percentage, Decimal::from(100));
    }
    
    /// Test phantom buy scenario for unmatched sells
    #[tokio::test]
    async fn test_phantom_buy_scenario() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        let events = vec![
            // Buy 100 tokens @ $2.00
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from(200),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            // Sell 200 tokens @ $3.00
            // 100 should match against the buy, 100 should be unmatched (phantom buy)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(200),
                usd_price_per_token: Decimal::from(3),
                usd_value: Decimal::from(600),
                timestamp: DateTime::from_timestamp(2000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
        ];
        
        let result = engine.calculate_token_pnl(events, None).await.unwrap();
        
        // Should have 1 matched trade
        assert_eq!(result.matched_trades.len(), 1);
        assert_eq!(result.matched_trades[0].matched_quantity, Decimal::from(100));
        assert_eq!(result.matched_trades[0].realized_pnl_usd, Decimal::from(100)); // (3-2) * 100
        
        // Should have 1 unmatched sell
        assert_eq!(result.unmatched_sells.len(), 1);
        assert_eq!(result.unmatched_sells[0].unmatched_quantity, Decimal::from(100));
        assert_eq!(result.unmatched_sells[0].phantom_buy_price, Decimal::from(3)); // Same as sell price
        assert_eq!(result.unmatched_sells[0].phantom_pnl_usd, Decimal::ZERO); // Zero P&L
        
        // Total realized P&L: $100 (matched) + $0 (phantom) = $100
        assert_eq!(result.total_realized_pnl_usd, Decimal::from(100));
        
        // No remaining position
        assert!(result.remaining_position.is_none());
        
        // Total P&L: $100 (realized) + $0 (unrealized) = $100
        assert_eq!(result.total_pnl_usd, Decimal::from(100));
    }
    
    /// Test multi-token portfolio analysis
    #[tokio::test]
    async fn test_multi_token_portfolio() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        // Create events for two different tokens
        let mut events_by_token = std::collections::HashMap::new();
        
        // Token 1 events
        let token1_events = vec![
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TOK1".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(1),
                usd_value: Decimal::from(100),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TOK1".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(50),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from(100),
                timestamp: DateTime::from_timestamp(2000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
        ];
        
        // Token 2 events
        let token2_events = vec![
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token2".to_string(),
                token_symbol: "TOK2".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(200),
                usd_price_per_token: Decimal::from(3),
                usd_value: Decimal::from(600),
                timestamp: DateTime::from_timestamp(1500, 0).unwrap(),
                transaction_hash: "tx3".to_string(),
            },
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token2".to_string(),
                token_symbol: "TOK2".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(4),
                usd_value: Decimal::from(400),
                timestamp: DateTime::from_timestamp(2500, 0).unwrap(),
                transaction_hash: "tx4".to_string(),
            },
        ];
        
        events_by_token.insert("token1".to_string(), token1_events);
        events_by_token.insert("token2".to_string(), token2_events);
        
        // Current prices for unrealized P&L
        let mut current_prices = std::collections::HashMap::new();
        current_prices.insert("token1".to_string(), Decimal::from(5)); // Token1 @ $5
        current_prices.insert("token2".to_string(), Decimal::from(6)); // Token2 @ $6
        
        let result = engine.calculate_portfolio_pnl(events_by_token, Some(current_prices)).await.unwrap();
        
        // Should have 2 tokens analyzed
        assert_eq!(result.tokens_analyzed, 2);
        assert_eq!(result.token_results.len(), 2);
        
        // Token1 analysis:
        // - 1 matched trade: 50 @ $1 -> 50 @ $2 = $50 profit
        // - Remaining position: 50 @ $1 cost basis
        // - Unrealized P&L: 50 * (5 - 1) = $200
        let token1_result = result.token_results.iter().find(|r| r.token_symbol == "TOK1").unwrap();
        assert_eq!(token1_result.matched_trades.len(), 1);
        assert_eq!(token1_result.total_realized_pnl_usd, Decimal::from(50));
        assert_eq!(token1_result.total_unrealized_pnl_usd, Decimal::from(200));
        
        // Token2 analysis:
        // - 1 matched trade: 100 @ $3 -> 100 @ $4 = $100 profit
        // - Remaining position: 100 @ $3 cost basis
        // - Unrealized P&L: 100 * (6 - 3) = $300
        let token2_result = result.token_results.iter().find(|r| r.token_symbol == "TOK2").unwrap();
        assert_eq!(token2_result.matched_trades.len(), 1);
        assert_eq!(token2_result.total_realized_pnl_usd, Decimal::from(100));
        assert_eq!(token2_result.total_unrealized_pnl_usd, Decimal::from(300));
        
        // Portfolio totals:
        // - Total realized P&L: $50 + $100 = $150
        // - Total unrealized P&L: $200 + $300 = $500
        // - Total P&L: $150 + $500 = $650
        assert_eq!(result.total_realized_pnl_usd, Decimal::from(150));
        assert_eq!(result.total_unrealized_pnl_usd, Decimal::from(500));
        assert_eq!(result.total_pnl_usd, Decimal::from(650));
        
        // Portfolio win rate: 2/2 = 100%
        assert_eq!(result.overall_win_rate_percentage, Decimal::from(100));
        
        // Total trades: 2
        assert_eq!(result.total_trades, 2);
    }
    
    /// Test hold time calculations
    #[tokio::test]
    async fn test_hold_time_calculations() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        let events = vec![
            // Buy at timestamp 1000
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(1),
                usd_value: Decimal::from(100),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            // Sell at timestamp 4000 (held for 3000 seconds = 50 minutes)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from(200),
                timestamp: DateTime::from_timestamp(4000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
        ];
        
        let result = engine.calculate_token_pnl(events, None).await.unwrap();
        
        // Should have 1 matched trade
        assert_eq!(result.matched_trades.len(), 1);
        
        // Hold time should be 3000 seconds
        assert_eq!(result.matched_trades[0].hold_time_seconds, 3000);
        
        // Average hold time should be 50 minutes (3000 seconds / 60)
        assert_eq!(result.avg_hold_time_minutes, Decimal::from(50));
        assert_eq!(result.min_hold_time_minutes, Decimal::from(50));
        assert_eq!(result.max_hold_time_minutes, Decimal::from(50));
    }
    
    /// Test zero quantities and invalid data handling
    #[tokio::test]
    async fn test_error_handling() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        // Test empty events
        let result = engine.calculate_token_pnl(vec![], None).await;
        assert!(result.is_err());
        
        // Test events with zero quantities (should be filtered out by parser)
        let events = vec![
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::ZERO,
                usd_price_per_token: Decimal::from(1),
                usd_value: Decimal::ZERO,
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
        ];
        
        let result = engine.calculate_token_pnl(events, None).await.unwrap();
        
        // Should have no trades since quantity is zero
        assert_eq!(result.matched_trades.len(), 0);
        assert_eq!(result.total_realized_pnl_usd, Decimal::ZERO);
        assert!(result.remaining_position.is_none());
    }
    
    /// Test unrealized P&L calculation formula compliance with documentation
    #[tokio::test]
    async fn test_unrealized_pnl_calculation_formula() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        let events = vec![
            // Buy 100 tokens @ $2.00 (cost basis = $2.00)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(100),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from(200),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            // Buy 50 tokens @ $4.00 (cost basis = $4.00)
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from(50),
                usd_price_per_token: Decimal::from(4),
                usd_value: Decimal::from(200),
                timestamp: DateTime::from_timestamp(2000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
            // Sell 50 tokens @ $6.00
            // Should match against first buy (50 @ $2.00)
            // Remaining: 50 @ $2.00 + 50 @ $4.00 = 100 tokens @ $3.00 avg
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Sell,
                quantity: Decimal::from(50),
                usd_price_per_token: Decimal::from(6),
                usd_value: Decimal::from(300),
                timestamp: DateTime::from_timestamp(3000, 0).unwrap(),
                transaction_hash: "tx3".to_string(),
            },
        ];
        
        // Current price is $5.00
        let current_price = Decimal::from(5);
        let result = engine.calculate_token_pnl(events, Some(current_price)).await.unwrap();
        
        // Verify remaining position
        assert!(result.remaining_position.is_some());
        let position = result.remaining_position.unwrap();
        assert_eq!(position.quantity, Decimal::from(100)); // 50 remaining from first buy + 50 from second buy
        assert_eq!(position.avg_cost_basis_usd, Decimal::from(3)); // (50*2 + 50*4) / 100 = 3
        assert_eq!(position.total_cost_basis_usd, Decimal::from(300)); // 50*2 + 50*4 = 300
        
        // Test the specific formula from documentation:
        // (current_price - weighted_avg_cost_basis) × remaining_quantity
        // = (5 - 3) × 100 = 2 × 100 = 200
        let expected_unrealized_pnl = (current_price - position.avg_cost_basis_usd) * position.quantity;
        assert_eq!(expected_unrealized_pnl, Decimal::from(200));
        
        // Verify our calculation matches the expected result
        assert_eq!(result.total_unrealized_pnl_usd, Decimal::from(200));
        
        // Also verify the old formula would give the same result in this case
        // (current_price × quantity) - total_cost_basis = (5 × 100) - 300 = 500 - 300 = 200
        let old_formula_result = (current_price * position.quantity) - position.total_cost_basis_usd;
        assert_eq!(old_formula_result, Decimal::from(200));
        
        // Both formulas should give the same result in this case
        assert_eq!(result.total_unrealized_pnl_usd, old_formula_result);
    }
    
    /// Test unrealized P&L calculation with fractional quantities
    #[tokio::test]
    async fn test_unrealized_pnl_fractional_quantities() {
        let engine = NewPnLEngine::new("test_wallet".to_string());
        
        let events = vec![
            // Buy 10.5 tokens @ $1.50
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from_f64(10.5).unwrap(),
                usd_price_per_token: Decimal::from_f64(1.50).unwrap(),
                usd_value: Decimal::from_f64(15.75).unwrap(),
                timestamp: DateTime::from_timestamp(1000, 0).unwrap(),
                transaction_hash: "tx1".to_string(),
            },
            // Sell 3.3 tokens @ $2.00
            NewFinancialEvent {
                wallet_address: "test".to_string(),
                token_address: "token1".to_string(),
                token_symbol: "TEST".to_string(),
                event_type: NewEventType::Buy,
                quantity: Decimal::from_f64(3.3).unwrap(),
                usd_price_per_token: Decimal::from(2),
                usd_value: Decimal::from_f64(6.60).unwrap(),
                timestamp: DateTime::from_timestamp(2000, 0).unwrap(),
                transaction_hash: "tx2".to_string(),
            },
        ];
        
        // Current price is $3.00
        let current_price = Decimal::from(3);
        let result = engine.calculate_token_pnl(events, Some(current_price)).await.unwrap();
        
        // Verify remaining position
        assert!(result.remaining_position.is_some());
        let position = result.remaining_position.unwrap();
        
        // Remaining quantity: 10.5 + 3.3 = 13.8
        let expected_quantity = Decimal::from_f64(13.8).unwrap();
        assert_eq!(position.quantity, expected_quantity);
        
        // Total cost: 15.75 + 6.60 = 22.35
        let expected_total_cost = Decimal::from_f64(22.35).unwrap();
        assert_eq!(position.total_cost_basis_usd, expected_total_cost);
        
        // Avg cost basis: 22.35 / 13.8 ≈ 1.6196
        let expected_avg_cost = expected_total_cost / expected_quantity;
        assert_eq!(position.avg_cost_basis_usd, expected_avg_cost);
        
        // Test documentation formula: (current_price - avg_cost_basis) × quantity
        let expected_unrealized_pnl = (current_price - expected_avg_cost) * expected_quantity;
        assert_eq!(result.total_unrealized_pnl_usd, expected_unrealized_pnl);
        
        // Verify this is different from old formula in some cases due to precision
        let old_formula_result = (current_price * expected_quantity) - expected_total_cost;
        
        // In this case, both should be mathematically equivalent
        assert!((result.total_unrealized_pnl_usd - old_formula_result).abs() < Decimal::from_f64(0.0001).unwrap());
    }
}