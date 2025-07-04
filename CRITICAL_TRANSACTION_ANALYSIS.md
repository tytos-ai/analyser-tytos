# 🚨 CRITICAL TRANSACTION DATA ANALYSIS

## **TRANSACTION STRUCTURE BREAKDOWN**

### **Core Structure:**
```json
{
  "quote": { /* SOL side */ },
  "base": { /* BNSOL side */ },
  "base_price": 155.5613863476638,  // USD per BNSOL
  "quote_price": 146.96600120510325, // USD per SOL
  "volume_usd": 1304914.8430134251,
  "volume": 8369.439226307,
  "block_unix_time": 1751414738
}
```

### **🔍 QUOTE OBJECT (SOL):**
```json
"quote": {
  "symbol": "SOL",
  "decimals": 9,
  "address": "So11111111111111111111111111111111111111112",
  "amount": 8879025300500,           // Raw amount (with decimals)
  "type": "transfer",               // transfer/split
  "type_swap": "from",              // from/to
  "ui_amount": 8879.0253005,        // Human readable amount
  "price": 146.96600120510325,      // USD price per SOL
  "nearest_price": 146.96600120510325,
  "change_amount": -8879025300500,   // SIGNED change (negative = outflow)
  "ui_change_amount": -8879.0253005  // SIGNED UI change
}
```

### **🔍 BASE OBJECT (BNSOL):**
```json
"base": {
  "symbol": "BNSOL",
  "decimals": 9,
  "address": "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85",
  "amount": 8369439226307,           // Raw amount (with decimals)
  "type": "mintTo",                 // mintTo/burn
  "type_swap": "to",                // from/to
  "ui_amount": 8369.439226307,      // Human readable amount
  "price": 155.5613863476638,       // USD price per BNSOL
  "nearest_price": 155.5613863476638,
  "change_amount": 8369439226307,    // SIGNED change (positive = inflow)
  "ui_change_amount": 8369.439226307 // SIGNED UI change
}
```

## **🚨 CRITICAL FINDINGS:**

### **1. TRANSACTION TYPES:**
- **BUY BNSOL**: `quote.type_swap = "from"` (SOL out) + `base.type_swap = "to"` (BNSOL in)
- **SELL BNSOL**: `base.type_swap = "from"` (BNSOL out) + `quote.type_swap = "to"` (SOL in)

### **2. AMOUNT FIELDS:**
- **`amount`**: Raw amount with decimals (e.g., 8879025300500 = 8879.0253005 SOL)
- **`ui_amount`**: Human-readable amount (8879.0253005 SOL)
- **`change_amount`**: SIGNED amount (negative = outflow, positive = inflow)
- **`ui_change_amount`**: SIGNED human-readable amount

### **3. PRICE FIELDS:**
- **`price`**: USD price per token at transaction time
- **`base_price`**: USD price per BNSOL (duplicates base.price)
- **`quote_price`**: USD price per SOL (duplicates quote.price)

### **4. CRITICAL DATA INTERPRETATION:**

#### **EXAMPLE TRANSACTION 1 (BUY BNSOL):**
- SOL Out: `-8879.0253005` SOL at `$146.97` per SOL
- BNSOL In: `+8369.439226307` BNSOL at `$155.56` per BNSOL
- SOL Cost: `8879.0253005 SOL` (actual SOL spent)
- USD Cost: `8879.0253005 × $146.97 = $1,304,914.84`

#### **EXAMPLE TRANSACTION 9 (SELL BNSOL):**
- BNSOL Out: `-1134.86998308` BNSOL at `$149.68` per BNSOL
- SOL In: `+1201.603133996` SOL at `$141.22` per SOL
- SOL Revenue: `1201.603133996 SOL` (actual SOL received)
- USD Revenue: `1201.603133996 × $141.22 = $169,688.82`

## **⚠️ CRITICAL ERRORS TO CHECK:**

### **1. PRICE UNIT CONFUSION:**
- ✅ `price` fields are in USD (confirmed)
- ❌ Our system might mix SOL and USD units

### **2. SWAP DIRECTION DETECTION:**
- ✅ Use `type_swap` field to determine direction
- ❌ Our system might use wrong logic

### **3. AMOUNT INTERPRETATION:**
- ✅ Use `ui_change_amount` for signed amounts
- ❌ Our system might use unsigned amounts

### **4. SOL vs USD CALCULATION:**
- ✅ SOL amounts: Use `ui_change_amount`
- ✅ USD amounts: Use `ui_change_amount × price`
- ❌ Our system might confuse these

## **🎯 NEXT STEPS:**
1. Verify our transaction parsing logic against this structure
2. Check if we handle `type_swap` correctly
3. Verify we use `ui_change_amount` not `ui_amount`
4. Confirm price unit handling (USD vs SOL)
5. Test with both BUY and SELL transactions