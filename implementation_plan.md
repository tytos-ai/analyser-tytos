# 🔧 CONCRETE IMPLEMENTATION PLAN

## **STEP-BY-STEP IMPLEMENTATION**

### **Step 1: Add SOL Price Resolution Helper**

**File**: `job_orchestrator/src/lib.rs`

```rust
impl ProcessedSwap {
    /// Get SOL price in USD from transaction data or fetch externally
    fn resolve_sol_price_usd(
        transactions: &[GeneralTraderTransaction],
        timestamp: DateTime<Utc>
    ) -> Result<Decimal> {
        let sol_mint = "So11111111111111111111111111111111111111112";
        
        // Strategy 1: Extract SOL price from transaction data
        for tx in transactions {
            if tx.quote.address == sol_mint && tx.quote.price.is_some() {
                return Ok(Decimal::try_from(tx.quote.price.unwrap())?);
            }
            if tx.base.address == sol_mint && tx.base.price.is_some() {
                return Ok(Decimal::try_from(tx.base.price.unwrap())?);
            }
        }
        
        // Strategy 2: Use quote_price or base_price from transaction level
        if let Some(tx) = transactions.first() {
            if tx.quote.address == sol_mint {
                return Ok(Decimal::try_from(tx.quote_price)?);
            }
            // Add logic to fetch historical price if needed
        }
        
        // Strategy 3: Default fallback (could fetch from external API)
        // For now, use a reasonable default or return error
        Err(OrchestratorError::JobExecution(
            "Unable to determine SOL price for token-to-token swap".to_string()
        ))
    }
}
```

### **Step 2: Fix sol_equivalent Calculation**

**Modify**: `aggregate_transaction_swaps` method

```rust
// Current buggy code (lines 149-151):
} else {
    // No SOL involved, use USD equivalent via token price
    amount_out * token_price  // ❌ Results in USD
};

// Fixed code:
} else {
    // Token → Token swap: Convert USD value to SOL equivalent
    let usd_value = amount_out * token_price;
    let sol_price_usd = Self::resolve_sol_price_usd(transactions, timestamp)?;
    usd_value / sol_price_usd  // ✅ Results in SOL
};
```

### **Step 3: Create Dual Event Method**

**Add new method**: `to_financial_events` (returning Vec)

```rust
impl ProcessedSwap {
    /// Convert ProcessedSwap to FinancialEvent(s)
    /// Returns single event for SOL swaps, dual events for token-to-token
    pub fn to_financial_events(&self, wallet_address: &str) -> Vec<FinancialEvent> {
        let sol_mint = "So11111111111111111111111111111111111111112";
        
        if self.token_in == sol_mint {
            // SOL → Token swap: Single BUY event
            vec![self.create_buy_event(wallet_address)]
        } else if self.token_out == sol_mint {
            // Token → SOL swap: Single SELL event
            vec![self.create_sell_event(wallet_address)]
        } else {
            // Token → Token swap: Dual events
            vec![
                self.create_sell_event_for_token_in(wallet_address),
                self.create_buy_event_for_token_out(wallet_address)
            ]
        }
    }
    
    fn create_sell_event_for_token_in(&self, wallet_address: &str) -> FinancialEvent {
        FinancialEvent {
            id: Uuid::new_v4(),
            transaction_id: self.tx_hash.clone(),
            wallet_address: wallet_address.to_string(),
            event_type: EventType::Sell,
            token_mint: self.token_in.clone(),     // Token being sold
            token_amount: self.amount_in,          // Amount sold
            sol_amount: self.sol_equivalent,       // SOL equivalent (now correct)
            timestamp: self.timestamp,
            transaction_fee: Decimal::ZERO,
            metadata: EventMetadata {
                price_per_token: Some(self.price_per_token),
                extra: {
                    let mut extra = HashMap::new();
                    extra.insert("swap_type".to_string(), "token_to_token_sell".to_string());
                    extra.insert("counterpart_token".to_string(), self.token_out.clone());
                    extra
                },
                ..Default::default()
            },
        }
    }
    
    fn create_buy_event_for_token_out(&self, wallet_address: &str) -> FinancialEvent {
        FinancialEvent {
            id: Uuid::new_v4(),
            transaction_id: self.tx_hash.clone(),
            wallet_address: wallet_address.to_string(),
            event_type: EventType::Buy,
            token_mint: self.token_out.clone(),    // Token being bought
            token_amount: self.amount_out,         // Amount received
            sol_amount: self.sol_equivalent,       // SOL equivalent (same value)
            timestamp: self.timestamp,
            transaction_fee: Decimal::ZERO,
            metadata: EventMetadata {
                price_per_token: Some(self.price_per_token),
                extra: {
                    let mut extra = HashMap::new();
                    extra.insert("swap_type".to_string(), "token_to_token_buy".to_string());
                    extra.insert("counterpart_token".to_string(), self.token_in.clone());
                    extra
                },
                ..Default::default()
            },
        }
    }
}
```

### **Step 4: Update Calling Code**

**Modify**: All places that call `to_financial_event` to handle multiple events

```rust
// Current calling pattern:
let event = swap.to_financial_event(wallet_address);
events.push(event);

// New calling pattern:
let financial_events = swap.to_financial_events(wallet_address);
events.extend(financial_events);
```

### **Step 5: Add Comprehensive Tests**

**File**: `job_orchestrator/tests/token_to_token_fixes.rs`

```rust
//! Tests for token-to-token swap fixes

use super::*;

#[tokio::test]
async fn test_token_to_token_sol_equivalent_units() {
    // Create mock USDC → RENDER swap
    let mock_tx = create_mock_token_to_token_transaction();
    
    let swaps = ProcessedSwap::from_birdeye_transactions(&[mock_tx]).unwrap();
    let swap = &swaps[0];
    
    // Verify sol_equivalent is in SOL units, not USD
    // If RENDER costs $20 and SOL costs $150, then 50 RENDER = $1000 = 6.67 SOL
    assert!(swap.sol_equivalent > Decimal::from(6));
    assert!(swap.sol_equivalent < Decimal::from(7));
    assert!(swap.sol_equivalent != Decimal::from(1000)); // Not USD!
}

#[tokio::test]
async fn test_token_to_token_dual_events() {
    let mock_tx = create_mock_token_to_token_transaction();
    let swaps = ProcessedSwap::from_birdeye_transactions(&[mock_tx]).unwrap();
    let swap = &swaps[0];
    
    let events = swap.to_financial_events("test_wallet");
    
    assert_eq!(events.len(), 2, "Should create exactly 2 events");
    
    // First event should be SELL of input token
    assert_eq!(events[0].event_type, EventType::Sell);
    assert_eq!(events[0].token_mint, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"); // USDC
    assert_eq!(events[0].token_amount, Decimal::from(1000)); // 1000 USDC sold
    
    // Second event should be BUY of output token
    assert_eq!(events[1].event_type, EventType::Buy);
    assert_eq!(events[1].token_mint, "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof"); // RENDER
    assert_eq!(events[1].token_amount, Decimal::from(50)); // 50 RENDER bought
    
    // Both should have same SOL equivalent
    assert_eq!(events[0].sol_amount, events[1].sol_amount);
}

fn create_mock_token_to_token_transaction() -> GeneralTraderTransaction {
    // Mock USDC → RENDER transaction
    GeneralTraderTransaction {
        quote: TokenTransactionSide {
            symbol: "USDC".to_string(),
            address: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            ui_change_amount: -1000.0, // Spent 1000 USDC
            price: Some(1.0), // $1 per USDC
            // ... other fields
        },
        base: TokenTransactionSide {
            symbol: "RENDER".to_string(),
            address: "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof".to_string(),
            ui_change_amount: 50.0, // Received 50 RENDER
            price: Some(20.0), // $20 per RENDER
            // ... other fields
        },
        quote_price: 150.0, // $150 per SOL (for conversion)
        // ... other transaction fields
    }
}
```

---

## **🔄 MIGRATION STRATEGY**

### **Phase 1: Backward Compatible Implementation**
1. Keep existing `to_financial_event` method
2. Add new `to_financial_events` method
3. Add feature flag to switch between old/new behavior

### **Phase 2: Gradual Migration**
```rust
// Add feature flag
#[cfg(feature = "token_to_token_fixes")]
let events = swap.to_financial_events(wallet_address);
#[cfg(not(feature = "token_to_token_fixes"))]  
let events = vec![swap.to_financial_event(wallet_address)];
```

### **Phase 3: Complete Cutover**
1. Update all callers to use new method
2. Remove old method
3. Remove feature flag

---

## **⚡ QUICK START IMPLEMENTATION**

Want to start immediately? Here's the minimal viable fix:

### **Immediate Fix for sol_equivalent**
```rust
// In aggregate_transaction_swaps, replace line 150:
// OLD: amount_out * token_price
// NEW: 
let usd_value = amount_out * token_price;
let sol_price = Decimal::try_from(first_tx.quote_price).unwrap_or(Decimal::from(150)); // Fallback
usd_value / sol_price
```

This single line change fixes the critical unit mismatch immediately!

---

**This implementation plan provides both a quick fix and a comprehensive long-term solution. We can start with the immediate sol_equivalent fix and then implement the dual event system for complete token-to-token swap support.**