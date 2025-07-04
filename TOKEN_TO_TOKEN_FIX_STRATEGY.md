# 🛠️ TOKEN-TO-TOKEN SWAP FIX STRATEGY

## **PROBLEM SUMMARY**

Two critical bugs prevent proper handling of token-to-token swaps:

1. **Unit Mismatch**: `sol_equivalent` stores USD but `FinancialEvent.sol_amount` expects SOL
2. **Missing SELL Event**: Token-to-token swaps only create BUY events, missing SELL events

---

## **🎯 STRATEGIC APPROACH**

### **Option A: SOL-Centric Conversion Strategy** ⭐ **RECOMMENDED**
Convert all token values to SOL equivalents using SOL price at transaction time.

### **Option B: Dual Event Strategy**
Create both SELL and BUY events for token-to-token swaps.

### **Option C: USD-Native Strategy** 
Redesign system to be USD-native instead of SOL-centric.

---

## **📋 DETAILED SOLUTION: SOL-CENTRIC CONVERSION**

### **Core Principle**
All `FinancialEvent.sol_amount` values must represent actual SOL quantities, even for token-to-token swaps.

### **Implementation Strategy**

#### **Step 1: Fix sol_equivalent Calculation**

**Current (Buggy):**
```rust
// Token → Token: amount_out * token_price = USD value ❌
let sol_equiv = amount_out * token_price;  // Results in USD
```

**Fixed:**
```rust
// Token → Token: Convert USD value to SOL using SOL price ✅
let usd_value = amount_out * token_price;
let sol_price_usd = get_sol_price_from_transaction(transactions);
let sol_equiv = usd_value / sol_price_usd;  // Results in SOL
```

#### **Step 2: Dual Event Creation for Token-to-Token**

**Current (Incomplete):**
```rust
// Only creates BUY event ❌
EventType::Buy for token_out
```

**Fixed:**
```rust
// Creates both SELL and BUY events ✅
1. EventType::Sell for token_in
2. EventType::Buy for token_out
```

#### **Step 3: SOL Price Extraction Strategy**

**Challenge**: Need SOL price for token-to-token swaps where SOL isn't involved.

**Solutions**:
1. **From Transaction Data**: If one side was SOL, use that price
2. **From External API**: Fetch SOL price at transaction timestamp  
3. **From Price Context**: Use SOL price from nearby transactions

---

## **🔧 IMPLEMENTATION PLAN**

### **Phase 1: Core Infrastructure** (1-2 hours)

#### **1.1 Add SOL Price Resolution**
```rust
// New function in ProcessedSwap
fn get_sol_price_at_transaction(
    transactions: &[GeneralTraderTransaction]
) -> Result<Decimal> {
    // Try to find SOL price from transaction data
    // Fallback to external price fetch if needed
}
```

#### **1.2 Fix sol_equivalent Calculation**
```rust
// Update aggregate_transaction_swaps method
let sol_equiv = if token_in == sol_mint || token_out == sol_mint {
    // SOL involved: use actual SOL amount
    actual_sol_amount
} else {
    // Token-to-token: convert USD to SOL
    let usd_value = amount_out * token_price;
    let sol_price = get_sol_price_at_transaction(transactions)?;
    usd_value / sol_price  // Now returns SOL quantity
};
```

### **Phase 2: Dual Event Creation** (2-3 hours)

#### **2.1 Modify to_financial_event Method**
```rust
pub fn to_financial_events(&self, wallet_address: &str) -> Vec<FinancialEvent> {
    let sol_mint = "So11111111111111111111111111111111111111112";
    
    if self.token_in == sol_mint {
        // SOL → Token: Single BUY event
        vec![create_buy_event(...)]
    } else if self.token_out == sol_mint {
        // Token → SOL: Single SELL event  
        vec![create_sell_event(...)]
    } else {
        // Token → Token: Dual events
        vec![
            create_sell_event_for_token_in(...),
            create_buy_event_for_token_out(...)
        ]
    }
}
```

#### **2.2 Update Calling Code**
```rust
// Update all callers to handle Vec<FinancialEvent>
let events = swap.to_financial_events(wallet_address);
for event in events {
    // Process each event
}
```

### **Phase 3: Price Resolution Enhancement** (1-2 hours)

#### **3.1 SOL Price Fallback Strategy**
```rust
async fn resolve_sol_price_usd(
    &self,
    timestamp: DateTime<Utc>,
    transactions: &[GeneralTraderTransaction]
) -> Result<Decimal> {
    // Strategy 1: Extract from transaction if SOL involved
    if let Some(sol_price) = extract_sol_price_from_transactions(transactions) {
        return Ok(sol_price);
    }
    
    // Strategy 2: Fetch historical price from BirdEye
    if let Some(price) = self.price_fetcher.fetch_historical_price(
        "So11111111111111111111111111111111111111112",
        timestamp,
        Some("usd")
    ).await? {
        return Ok(price);
    }
    
    // Strategy 3: Use current price as fallback
    let current_prices = self.price_fetcher.fetch_prices(
        &["So11111111111111111111111111111111111111112"],
        Some("usd")
    ).await?;
    
    current_prices.get("So11111111111111111111111111111111111111112")
        .copied()
        .ok_or_else(|| Error::PriceNotFound)
}
```

---

## **📊 VALIDATION STRATEGY**

### **Test Cases to Implement**

#### **Test 1: Token-to-Token Unit Verification**
```rust
#[test]
fn test_token_to_token_sol_equivalent_units() {
    // USDC → RENDER swap
    // Verify sol_equivalent is in SOL units, not USD
    let swap = create_usdc_to_render_swap();
    assert!(swap.sol_equivalent < 100.0); // Should be ~6.67 SOL, not $1000
}
```

#### **Test 2: Dual Event Creation**
```rust
#[test]
fn test_token_to_token_dual_events() {
    let swap = create_usdc_to_render_swap();
    let events = swap.to_financial_events("wallet");
    
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, EventType::Sell); // USDC
    assert_eq!(events[1].event_type, EventType::Buy);  // RENDER
}
```

#### **Test 3: FIFO Integrity**
```rust
#[test]
fn test_fifo_with_token_to_token() {
    // Buy USDC → Swap USDC→RENDER → Verify P&L realized on USDC
    let events = vec![
        buy_usdc_event(),
        ...token_to_token_swap_events(),
    ];
    
    let report = fifo_engine.calculate_pnl(events).await?;
    // Verify USDC P&L is realized, not stuck in holdings
}
```

---

## **⚠️ EDGE CASES TO HANDLE**

### **1. SOL Price Unavailable**
- **Scenario**: Token-to-token swap, no SOL price data
- **Solution**: Graceful degradation, use approximate conversion or skip transaction

### **2. Circular Dependencies**
- **Scenario**: Token A → Token B → Token A in single transaction
- **Solution**: Process events in chronological order

### **3. Multi-Hop Swaps**
- **Scenario**: Token A → Token B → Token C in single transaction
- **Solution**: Create event chain: Sell(A) → Buy(B) → Sell(B) → Buy(C)

---

## **🚀 IMPLEMENTATION TIMELINE**

| Phase | Duration | Components |
|-------|----------|------------|
| **Phase 1** | 2 hours | SOL price resolution, fix unit calculation |
| **Phase 2** | 3 hours | Dual event creation, update callers |
| **Phase 3** | 2 hours | Price fallback strategies |
| **Testing** | 2 hours | Comprehensive test suite |
| **Integration** | 1 hour | End-to-end validation |
| **Total** | **10 hours** | Complete fix implementation |

---

## **✅ SUCCESS CRITERIA**

1. **✅ Unit Consistency**: All `sol_amount` fields contain SOL quantities
2. **✅ Complete Accounting**: Token-to-token swaps create both SELL and BUY events
3. **✅ FIFO Integrity**: P&L properly realized on all token dispositions
4. **✅ Backward Compatibility**: SOL ↔ Token swaps continue working perfectly
5. **✅ Test Coverage**: Comprehensive tests for all swap scenarios

---

## **🎯 RISK MITIGATION**

### **Low Risk Implementation**
1. **Feature Flag**: Implement behind feature flag for gradual rollout
2. **Validation Mode**: Run both old and new logic, compare results
3. **Incremental Rollout**: Test with small datasets first

### **Rollback Strategy**
- Keep existing code paths intact during development
- Comprehensive test suite ensures no regressions
- Clear commit history for easy rollback if needed

---

**This strategy provides a robust, maintainable solution that fixes both critical issues while preserving system integrity and backward compatibility.**