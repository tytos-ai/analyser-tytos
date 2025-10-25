# Investigation Summary - Double-Counting Bug

## What I Know FOR CERTAIN

### From Logs Analysis

**Parser creates 12 Solano transactions with implicit pricing:**
1. ✅ tx 6eca8a81: 9,498,861 tokens, $1068.58 (SOL IN - SELL)
2. ✅ tx ce65bbd8: 541,359 tokens, $192.24 (SOL OUT - BUY)
3. ✅ tx 1d54673a: 2,503,750 tokens, $1928.16 (SOL OUT - BUY)
4. ✅ tx ffe7ad79: 2,643,985 tokens, $1928.16 (SOL OUT - BUY)
5. ✅ tx 200767a6: 2,785,793 tokens, $1928.16 (SOL OUT - BUY)
6. ✅ tx ddb27539: 1,023,973 tokens, $694.48 (SOL OUT - BUY)
7. ✅ tx d6d545d5: 12,708,273 tokens, $271.24 (SOL IN - SELL)
8. ✅ tx 83632799: 140,827 tokens, $9.75 (SOL OUT - BUY)
9. ✅ tx 47073c3b: 299,088 tokens, $19.55 (SOL OUT - BUY)
10. ✅ tx dc71e3a2: 131,402 tokens, $9.26 (SOL OUT - BUY)
11. ✅ tx ce396d1f: 131,420 tokens, $9.27 (SOL OUT - BUY)
12. ✅ tx 239f0fba: 12,005,534 tokens, $409.83 (SOL OUT - BUY)

**PNL Engine reports:**
- 20 buy events (not 12!)
- 44,414,270 total tokens bought (not ~22M!)
- 22,207,135 remaining (exactly half!)

### The Math

If parser creates 12 events and PNL sees 20:
- **Missing: 8 events**
- **Or: Events are being counted/processed twice somewhere**

Sum of all 12 quantities from logs:
9,498,861 + 541,359 + 2,503,750 + 2,643,985 + 2,785,793 + 1,023,973 + 12,708,273 + 140,827 + 299,088 + 131,402 + 131,420 + 12,005,534 = **44,414,265** tokens

This matches the PNL "total bought" of **44,414,270** tokens! (tiny rounding diff)

So ALL 12 events (including the 2 SELL events!) are being counted as BUY events!

## THE ACTUAL BUG

**The 2 SELL transactions are being labeled as BUY!**

Looking at transactions:
- tx 6eca8a81: SOL IN → should be SELL Solano, but is counted as BUY
- tx d6d545d5: SOL IN → should be SELL Solano, but is counted as BUY

If we remove these 2 from the buy count:
- 20 - 2 = 18 (still not 10, so there's another issue)
- OR each of the 10 real BUY transactions is creating 2 events

## WHERE IS THE BUG?

Let me trace the direction logic in `convert_transfer_with_implicit_price`:

```rust
// Line 1487-1494
let event_type = match transfer.direction.as_str() {
    "in" | "self" => NewEventType::Buy,
    "out" => NewEventType::Sell,
    _ => {
        warn!("Unknown direction in implicit pricing: {}", transfer.direction);
        return None;
    }
};
```

For tx 6eca8a81:
- SOL is IN (received)
- Solano is OUT (sold)
- The Solano transfer has `direction = "out"`
- So `event_type = NewEventType::Sell` ✅ CORRECT!

**So why does PNL see it as a BUY?**

## HYPOTHESIS: Enrichment or Post-Processing

The enrichment process or some other post-processing step might be:
1. Creating duplicate events
2. Flipping the direction of events
3. Adding extra events from the same transaction

## WHAT TO CHECK NEXT

1. **Find where events go after parsing** - trace from zerion_client output to PNL engine input
2. **Check enrichment process** - see if BirdEye enrichment adds/modifies events
3. **Check API server** - see if it's doing any event processing
4. **Add logging to PNL engine** - log each individual buy event to see their transaction hashes
